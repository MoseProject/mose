// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Asynchronous preview worker: a dedicated thread that exclusively owns a [`Preview`] pipeline,
//! fed by bounded latest-value mailboxes.
//!
//! Frontend-neutral on purpose -- both the egui shell (`ui`) and the Tauri shell (`desktop`)
//! drive preview through this one type, so seek/open/live-update coalescing rules cannot drift
//! between them. Keeping the mailbox independent of the pipeline makes its no-backlog invariant
//! testable without a media fixture and prevents the worker from holding a mutex across a seek.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use crate::preview::{Preview, VideoFrame};
use crate::preview_live_updates::LivePreviewUpdate;
use crate::timeline::{ClipInstance, ShapeClip, TextClip};
use tracing::warn;

/// A one-item mailbox where a newer value replaces work that has not started yet.
///
/// `replace` and `take` only move `T` while holding the mutex. Callers must release the guard
/// before doing I/O, decoding, or any GStreamer operation.
pub struct LatestMailbox<T> {
    value: Mutex<Option<T>>,
}

impl<T> Default for LatestMailbox<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> LatestMailbox<T> {
    pub fn new() -> Self {
        Self {
            value: Mutex::new(None),
        }
    }

    /// Replaces the pending value and returns the obsolete one, if there was one.
    pub fn replace(&self, value: T) -> Option<T> {
        self.value.lock().ok()?.replace(value)
    }

    /// Removes the newest pending value for processing.
    pub fn take(&self) -> Option<T> {
        self.value.lock().ok()?.take()
    }
}

/// A seek expressed in the source-coordinate system the currently loaded pipeline expects.
pub enum PreviewSeek {
    Single { offset_secs: f64, rate: f64 },
    Composited { offsets: Vec<f64>, rates: Vec<f64> },
}

/// A decoded frame paired with the UI intent that caused it.
pub struct PreviewFrameReady {
    pub generation: u64,
    pub position_secs: Option<f64>,
    pub audio_level: crate::AudioLevel,
    pub frame: VideoFrame,
}

pub enum PreviewStatus {
    Ready { generation: u64 },
    Failed { generation: u64, message: String },
}

/// Whether a worker result still belongs to the UI's latest preview intent.
pub fn generation_is_current(expected: u64, received: u64) -> bool {
    expected == received
}

/// Stable identity for a replaceable live command.
///
/// Commands with the same key replace one another before the worker has run them. The namespace
/// is a static internal identifier, never user-provided data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LiveUpdateKey {
    namespace: &'static str,
    clip_id: Option<u64>,
}

impl LiveUpdateKey {
    pub const fn for_clip(namespace: &'static str, clip_id: u64) -> Self {
        Self {
            namespace,
            clip_id: Some(clip_id),
        }
    }

    pub const fn global(namespace: &'static str) -> Self {
        Self {
            namespace,
            clip_id: None,
        }
    }
}

trait Coalescible {
    type Key: Eq;

    fn coalescing_key(&self) -> Self::Key;
}

trait PreviewCommand: Send {
    fn key(&self) -> LiveUpdateKey;
    fn apply(self: Box<Self>, preview: &mut Preview);
}

struct ClosurePreviewCommand<F> {
    key: LiveUpdateKey,
    apply: F,
}

impl<F> PreviewCommand for ClosurePreviewCommand<F>
where
    F: FnOnce(&mut Preview) + Send + 'static,
{
    fn key(&self) -> LiveUpdateKey {
        self.key.clone()
    }

    fn apply(self: Box<Self>, preview: &mut Preview) {
        (self.apply)(preview);
    }
}

impl Coalescible for Box<dyn PreviewCommand> {
    type Key = LiveUpdateKey;

    fn coalescing_key(&self) -> Self::Key {
        self.key()
    }
}

/// Coalesces replaceable values while retaining unrelated keys.
///
/// The fixed capacity protects the UI thread from a stalled consumer. When it is full, the
/// oldest item is discarded; callers retain durable state outside this ephemeral mailbox.
struct PendingUpdates<T: Coalescible> {
    updates: Mutex<Vec<T>>,
}

impl<T: Coalescible> PendingUpdates<T> {
    const CAPACITY: usize = 64;

    fn new() -> Self {
        Self {
            updates: Mutex::new(Vec::with_capacity(Self::CAPACITY)),
        }
    }

    fn replace(&self, update: T) {
        let Ok(mut updates) = self.updates.lock() else {
            return;
        };
        if let Some(existing) = updates
            .iter_mut()
            .find(|existing| update.coalescing_key() == existing.coalescing_key())
        {
            *existing = update;
        } else {
            if updates.len() == Self::CAPACITY {
                updates.remove(0);
            }
            updates.push(update);
        }
    }

    fn take_all(&self) -> Vec<T> {
        let Ok(mut updates) = self.updates.lock() else {
            return Vec::new();
        };
        std::mem::take(&mut *updates)
    }
}

/// Owned inputs needed to build a GStreamer preview without borrowing UI/project state.
pub enum PreviewOpenRequest {
    Single {
        path: PathBuf,
        clip: ClipInstance,
        playhead_secs: f64,
        hardware_decode: bool,
    },
    Composited {
        path: PathBuf,
        clip: ClipInstance,
        overlays: Vec<(PathBuf, ClipInstance)>,
        audio: Vec<(PathBuf, ClipInstance)>,
        text: Vec<TextClip>,
        shapes: Vec<ShapeClip>,
        playhead_secs: f64,
        hardware_decode: bool,
    },
}

impl PreviewOpenRequest {
    fn open(self) -> Result<(Preview, PreviewSeek), crate::preview::PreviewError> {
        match self {
            Self::Single {
                path,
                clip,
                playhead_secs,
                hardware_decode,
            } => {
                let offset_secs = clip_seek_offset(&clip, playhead_secs);
                let rate = clip.speed_factor.max(0.01) as f64;
                Preview::open_with_hardware_decode(&path, Some(&clip), hardware_decode)
                    .map(|preview| (preview, PreviewSeek::Single { offset_secs, rate }))
            }
            Self::Composited {
                path,
                clip,
                overlays,
                audio,
                text,
                shapes,
                playhead_secs,
                hardware_decode,
            } => {
                let overlay_refs: Vec<_> = overlays
                    .iter()
                    .map(|(path, clip)| (path.as_path(), clip))
                    .collect();
                let audio_refs: Vec<_> = audio
                    .iter()
                    .map(|(path, clip)| (path.as_path(), clip))
                    .collect();
                let text_refs: Vec<_> = text
                    .iter()
                    .map(|clip| (clip, playhead_secs - clip.start_secs))
                    .collect();
                let shape_refs: Vec<_> = shapes.iter().collect();
                let (offsets, rates) = std::iter::once(&clip)
                    .chain(overlays.iter().map(|(_, clip)| clip))
                    .chain(audio.iter().map(|(_, clip)| clip))
                    .map(|clip| {
                        (
                            clip_seek_offset(clip, playhead_secs),
                            clip.speed_factor.max(0.01) as f64,
                        )
                    })
                    .unzip();
                Preview::open_composited_with_hardware_decode(
                    &path,
                    Some(&clip),
                    &overlay_refs,
                    &audio_refs,
                    &text_refs,
                    &shape_refs,
                    hardware_decode,
                )
                .map(|preview| (preview, PreviewSeek::Composited { offsets, rates }))
            }
        }
    }
}

/// This clip's timeline-to-source offset at `playhead_secs`, honoring `frozen` (always the held
/// anchor frame) and `speed_factor`. Every frontend seeking a preview branch must use this so
/// initial opens, scrubs, and speed edits share one synchronization rule.
pub fn clip_seek_offset(clip: &ClipInstance, playhead_secs: f64) -> f64 {
    if clip.frozen {
        clip.source_in_secs
    } else {
        clip.source_in_secs + (playhead_secs - clip.start_secs) * clip.speed_factor.max(0.01) as f64
    }
}

struct SeekRequest {
    generation: u64,
    seek: PreviewSeek,
}

/// Owns a dedicated thread that is the only caller of an active `Preview` pipeline.
///
/// Structural control is reliable; scrubs overwrite one pending request. The UI takes frames
/// from another one-slot mailbox, so decode can never make UI memory grow with stale frames.
pub struct PreviewWorker {
    opens: Arc<LatestMailbox<OpenRequest>>,
    playing: Arc<LatestMailbox<bool>>,
    seeks: Arc<LatestMailbox<SeekRequest>>,
    live_updates: Arc<PendingUpdates<Box<dyn PreviewCommand>>>,
    frames: Arc<LatestMailbox<PreviewFrameReady>>,
    status: Arc<LatestMailbox<PreviewStatus>>,
    wake_tx: mpsc::SyncSender<()>,
    shutdown: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

impl PreviewWorker {
    pub fn spawn() -> Self {
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let opens = Arc::new(LatestMailbox::new());
        let playing = Arc::new(LatestMailbox::new());
        let seeks = Arc::new(LatestMailbox::new());
        let live_updates = Arc::new(PendingUpdates::new());
        let frames = Arc::new(LatestMailbox::new());
        let status = Arc::new(LatestMailbox::new());
        let shutdown = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_opens = Arc::clone(&opens);
        let worker_playing = Arc::clone(&playing);
        let worker_seeks = Arc::clone(&seeks);
        let worker_live_updates = Arc::clone(&live_updates);
        let worker_frames = Arc::clone(&frames);
        let worker_status = Arc::clone(&status);
        let worker_shutdown = Arc::clone(&shutdown);
        let worker_stopped = Arc::clone(&stopped);
        std::thread::spawn(move || {
            worker_loop(
                wake_rx,
                WorkerMailboxes {
                    opens: worker_opens,
                    playing: worker_playing,
                    seeks: worker_seeks,
                    live_updates: worker_live_updates,
                    frames: worker_frames,
                    status: worker_status,
                },
                worker_shutdown,
                worker_stopped,
            )
        });
        Self {
            opens,
            playing,
            seeks,
            live_updates,
            frames,
            status,
            wake_tx,
            shutdown,
            stopped,
        }
    }

    pub fn open(&self, generation: u64, request: PreviewOpenRequest) {
        self.opens.replace(OpenRequest {
            generation,
            request: Box::new(request),
        });
        self.wake();
    }

    pub fn seek(&self, generation: u64, seek: PreviewSeek) {
        self.seeks.replace(SeekRequest { generation, seek });
        self.wake();
    }

    pub fn set_playing(&self, playing: bool) {
        self.playing.replace(playing);
        self.wake();
    }

    pub fn live<F>(&self, key: LiveUpdateKey, apply: F)
    where
        F: FnOnce(&mut Preview) + Send + 'static,
    {
        self.live_updates
            .replace(Box::new(ClosurePreviewCommand { key, apply }));
        self.wake();
    }

    /// Applies a shared, coalesced live property update without rebuilding the pipeline.
    pub fn live_update(&self, update: LivePreviewUpdate) {
        let key = match &update {
            LivePreviewUpdate::Balance { clip_id, .. } => {
                LiveUpdateKey::for_clip("balance", *clip_id)
            }
            LivePreviewUpdate::Blur { clip_id, .. } => LiveUpdateKey::for_clip("blur", *clip_id),
            LivePreviewUpdate::ChromaKey { clip_id, .. } => {
                LiveUpdateKey::for_clip("chroma-key", *clip_id)
            }
            LivePreviewUpdate::Crop { clip_id, .. } => LiveUpdateKey::for_clip("crop", *clip_id),
            LivePreviewUpdate::Pixelize { clip_id, .. } => {
                LiveUpdateKey::for_clip("pixelize", *clip_id)
            }
            LivePreviewUpdate::Shake { clip_id, .. } => LiveUpdateKey::for_clip("shake", *clip_id),
            LivePreviewUpdate::Mask { clip_id, .. } => LiveUpdateKey::for_clip("mask", *clip_id),
            LivePreviewUpdate::TextOverlay { clip, .. } => {
                LiveUpdateKey::for_clip("text-overlay", clip.id)
            }
            LivePreviewUpdate::ShapeOverlay { clip } => {
                LiveUpdateKey::for_clip("shape-overlay", clip.id)
            }
        };
        self.live(key, move |preview| update.apply(preview));
    }

    pub fn take_frame(&self) -> Option<PreviewFrameReady> {
        self.frames.take()
    }

    pub fn take_status(&self) -> Option<PreviewStatus> {
        self.status.take()
    }

    /// Requests a cooperative pipeline release and wakes a parked worker immediately.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Release);
        self.wake();
    }

    /// Whether the worker has released its pipeline after a shutdown request.
    pub fn has_stopped(&self) -> bool {
        self.stopped.load(Ordering::Acquire)
    }

    fn wake(&self) {
        let _ = self.wake_tx.try_send(());
    }
}

impl Drop for PreviewWorker {
    fn drop(&mut self) {
        if !self.has_stopped() {
            self.shutdown();
        }
    }
}

struct OpenRequest {
    generation: u64,
    request: Box<PreviewOpenRequest>,
}

struct WorkerMailboxes {
    opens: Arc<LatestMailbox<OpenRequest>>,
    playing: Arc<LatestMailbox<bool>>,
    seeks: Arc<LatestMailbox<SeekRequest>>,
    live_updates: Arc<PendingUpdates<Box<dyn PreviewCommand>>>,
    frames: Arc<LatestMailbox<PreviewFrameReady>>,
    status: Arc<LatestMailbox<PreviewStatus>>,
}

fn worker_loop(
    wake_rx: mpsc::Receiver<()>,
    mailboxes: WorkerMailboxes,
    shutdown: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
) {
    let WorkerMailboxes {
        opens,
        playing,
        seeks,
        live_updates,
        frames,
        status,
    } = mailboxes;
    let mut preview = None;
    let mut generation = 0;
    loop {
        if shutdown.load(Ordering::Acquire) {
            break;
        }
        if let Some(OpenRequest {
            generation: next,
            request,
        }) = opens.take()
        {
            generation = next;
            preview = match request.open() {
                Ok((preview, seek)) => {
                    let result = match seek {
                        PreviewSeek::Single { offset_secs, rate } => {
                            preview.seek_with_rate(offset_secs, rate)
                        }
                        PreviewSeek::Composited { offsets, rates } => {
                            preview.seek_composited(&offsets, &rates)
                        }
                    };
                    if let Err(error) = result {
                        warn!(%error, "failed to seek newly opened preview worker pipeline");
                    }
                    status.replace(PreviewStatus::Ready { generation });
                    Some(preview)
                }
                Err(error) => {
                    warn!(%error, "failed to open preview worker pipeline");
                    status.replace(PreviewStatus::Failed {
                        generation,
                        message: error.to_string(),
                    });
                    None
                }
            };
        }
        if let Some(playing) = playing.take() {
            if playing {
                if let Some(preview) = &mut preview {
                    if let Err(error) = preview.play() {
                        warn!(%error, "failed to play preview worker pipeline");
                    }
                }
            } else {
                if let Some(preview) = &preview {
                    if let Err(error) = preview.pause() {
                        warn!(%error, "failed to pause preview worker pipeline");
                    }
                }
            }
        }
        if let Some(preview) = &mut preview {
            for update in live_updates.take_all() {
                update.apply(preview);
            }
        }
        if let Some(request) = seeks.take() {
            if request.generation >= generation {
                if let Some(preview) = &preview {
                    let result = match request.seek {
                        PreviewSeek::Single { offset_secs, rate } => {
                            preview.seek_with_rate(offset_secs, rate)
                        }
                        PreviewSeek::Composited { offsets, rates } => {
                            preview.seek_composited(&offsets, &rates)
                        }
                    };
                    if let Err(error) = result {
                        warn!(%error, "failed to seek preview worker pipeline");
                    }
                }
                generation = request.generation;
            }
        }
        if let Some(preview) = &preview {
            if let Some(frame) = preview.current_frame() {
                frames.replace(PreviewFrameReady {
                    generation,
                    position_secs: preview.position_secs(),
                    audio_level: preview.current_audio_level(),
                    frame,
                });
            }
        }
        match wake_rx.recv_timeout(Duration::from_millis(8)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    stopped.store(true, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    use super::{
        generation_is_current, Coalescible, LatestMailbox, PendingUpdates, PreviewOpenRequest,
        PreviewSeek, PreviewStatus, PreviewWorker,
    };

    struct TestUpdate {
        key: u64,
        value: f64,
    }

    impl Coalescible for TestUpdate {
        type Key = u64;

        fn coalescing_key(&self) -> Self::Key {
            self.key
        }
    }

    fn assert_send<T: Send>() {}

    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    fn open_fixture(worker: &PreviewWorker, generation: u64) {
        open_fixture_at(worker, generation, 0.0);
    }

    fn open_fixture_at(worker: &PreviewWorker, generation: u64, playhead_secs: f64) {
        worker.open(
            generation,
            PreviewOpenRequest::Single {
                path: fixture("video.mp4"),
                clip: crate::timeline::ClipInstance::new(1, 1, 0.0, 0.0, 1.0),
                playhead_secs,
                hardware_decode: false,
            },
        );
    }

    fn wait_until<T>(description: &str, mut poll: impl FnMut() -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(value) = poll() {
                return value;
            }
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {description}"
            );
            std::thread::yield_now();
        }
    }

    #[test]
    fn preview_pipeline_can_be_owned_by_the_worker_thread() {
        assert_send::<crate::preview::Preview>();
    }

    #[test]
    fn latest_mailbox_keeps_only_the_newest_pending_request() {
        let mailbox = LatestMailbox::new();

        assert_eq!(mailbox.replace(10_u64), None);
        assert_eq!(mailbox.replace(11), Some(10));
        assert_eq!(mailbox.replace(12), Some(11));
        assert_eq!(mailbox.take(), Some(12));
        assert_eq!(mailbox.take(), None);
    }

    #[test]
    fn stale_worker_generation_is_rejected() {
        assert!(generation_is_current(12, 12));
        assert!(!generation_is_current(12, 11));
        assert!(!generation_is_current(12, 13));
    }

    #[test]
    fn pending_updates_coalesces_any_value_by_its_key() {
        let updates = PendingUpdates::new();
        updates.replace(TestUpdate { key: 1, value: 0.1 });
        updates.replace(TestUpdate { key: 2, value: 2.0 });
        updates.replace(TestUpdate { key: 1, value: 0.3 });

        let updates = updates.take_all();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].key, 1);
        assert!((updates[0].value - 0.3).abs() < f64::EPSILON);
        assert_eq!(updates[1].key, 2);
        assert!((updates[1].value - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn worker_opens_a_real_video_and_publishes_a_decoded_frame() {
        let worker = PreviewWorker::spawn();
        open_fixture(&worker, 1);

        wait_until("the real fixture pipeline to open", || {
            matches!(
                worker.take_status(),
                Some(PreviewStatus::Ready { generation: 1 })
            )
            .then_some(())
        });
        let frame = wait_until("a decoded frame from the real fixture", || {
            worker.take_frame()
        });

        assert_eq!(frame.generation, 1);
        assert!(frame.frame.width > 0);
        assert!(frame.frame.height > 0);
        assert!(!frame.frame.rgba.is_empty());
        worker.shutdown();
        wait_until("the worker to release its pipeline", || {
            worker.has_stopped().then_some(())
        });
    }

    #[test]
    fn rapid_seeks_publish_only_the_latest_generation() {
        let worker = PreviewWorker::spawn();
        open_fixture(&worker, 1);
        wait_until("the real fixture pipeline to open", || {
            matches!(
                worker.take_status(),
                Some(PreviewStatus::Ready { generation: 1 })
            )
            .then_some(())
        });
        wait_until("the initial decoded frame", || worker.take_frame());

        worker.seek(
            2,
            PreviewSeek::Single {
                offset_secs: 0.2,
                rate: 1.0,
            },
        );
        worker.seek(
            3,
            PreviewSeek::Single {
                offset_secs: 0.7,
                rate: 1.0,
            },
        );

        let frame = wait_until("the latest rapid seek frame", || {
            worker.take_frame().filter(|frame| frame.generation == 3)
        });
        assert_eq!(frame.generation, 3);
        worker.shutdown();
        wait_until("the worker to release its pipeline", || {
            worker.has_stopped().then_some(())
        });
    }

    #[test]
    fn replacing_the_preview_session_discards_frames_from_the_previous_clip() {
        let worker = PreviewWorker::spawn();
        open_fixture(&worker, 1);
        wait_until("the first fixture pipeline to open", || {
            matches!(
                worker.take_status(),
                Some(PreviewStatus::Ready { generation: 1 })
            )
            .then_some(())
        });
        wait_until("a frame from the first fixture session", || {
            worker.take_frame()
        });

        open_fixture_at(&worker, 2, 0.5);
        wait_until("the replacement fixture pipeline to open", || {
            matches!(
                worker.take_status(),
                Some(PreviewStatus::Ready { generation: 2 })
            )
            .then_some(())
        });
        let frame = wait_until("a frame from the replacement session", || {
            worker.take_frame().filter(|frame| frame.generation == 2)
        });

        assert_eq!(frame.generation, 2);
        worker.shutdown();
        wait_until("the worker to release its replacement pipeline", || {
            worker.has_stopped().then_some(())
        });
    }

    #[test]
    fn worker_reports_open_failures_without_panicking_the_ui_thread() {
        let worker = PreviewWorker::spawn();
        worker.open(
            9,
            PreviewOpenRequest::Single {
                path: fixture("missing-preview-source.mp4"),
                clip: crate::timeline::ClipInstance::new(1, 1, 0.0, 0.0, 1.0),
                playhead_secs: 0.0,
                hardware_decode: false,
            },
        );

        let message = wait_until("a structured preview open failure", || {
            match worker.take_status() {
                Some(PreviewStatus::Failed {
                    generation: 9,
                    message,
                }) => Some(message),
                _ => None,
            }
        });
        assert!(!message.is_empty());
        worker.shutdown();
        wait_until("the worker to stop after a failed open", || {
            worker.has_stopped().then_some(())
        });
    }
}
