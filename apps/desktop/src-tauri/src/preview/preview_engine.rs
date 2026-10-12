// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Playback coordinator handle for the Tauri shell.
//!
//! A webview has no per-frame UI loop on the Rust side, so this module owns the handle the
//! commands talk to: a dedicated thread (the Bush playback loop in [`crate::bush_adapter`])
//! receives timeline snapshots and transport commands over a channel. Decoded frames are
//! JPEG-encoded into a one-slot [`LatestFrame`] served by the `oca://` protocol; the webview
//! learns a new one exists from the throttled [`PREVIEW_TICK`] event.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use avcore::{MediaAsset, Project};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};

/// Event carrying a [`PreviewTick`].
pub const PREVIEW_TICK: &str = "preview-tick";

/// Event telling the webview the playback thread crashed ([`PreviewRecovery`]).
pub const PREVIEW_RECOVERY: &str = "preview-recovery";

/// How often the supervisor wakes to check on the playback thread while no command arrives.
const SUPERVISE_INTERVAL: Duration = Duration::from_millis(500);
/// A playback thread that crashes this many times within [`RESTART_WINDOW`] is not restarted
/// again: something about the project makes it crash every time.
const MAX_RESTARTS: usize = 5;
const RESTART_WINDOW: Duration = Duration::from_secs(60);

/// What the webview is told after a playback crash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewRecovery {
    /// The thread was started again with the scene it had; `false` means it was given up on.
    pub restarted: bool,
}

/// Upper bound on tick events per second while playing -- enough for a smooth playhead
/// without flooding the IPC bridge.
pub(crate) const TICK_INTERVAL: Duration = Duration::from_millis(16);
/// How long the engine parks waiting for a command while nothing is playing.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(8);
/// Fallback fps for frame stepping when the clip's asset has none (same guess as `avcore::render`).
pub(crate) const FALLBACK_FPS: f64 = 30.0;

/// The most recent encoded preview frame, shared with the `oca://` protocol handler.
pub type LatestFrame = Arc<Mutex<Option<Arc<Vec<u8>>>>>;

pub(crate) enum Command {
    /// Shared, so the supervisor keeps the latest copy for a restart without cloning it.
    Content {
        project: Arc<Project>,
    },
    Seek(f64),
    Step(i64),
    Playing(bool),
    /// The playhead is being dragged: seeks may show an earlier frame of a long GOP rather than
    /// wait for the exact one. Off again, the exact frame is shown.
    Scrubbing(bool),
    /// The longest side the preview should composite at (the size the monitor really shows).
    MaxDim(u32),
}

/// Snapshot of transport state for the webview.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTick {
    /// Increments every time [`LatestFrame`] changes.
    pub frame_seq: u64,
    pub playhead_secs: f64,
    pub playing: bool,
    /// A clip covers the timeline (whether or not it decoded).
    pub has_clip: bool,
    /// The engine has produced a frame for the current scene.
    pub available: bool,
    pub error: Option<String>,
    pub peak_l: f32,
    pub peak_r: f32,
    /// Momentary loudness in LUFS while playing; `None` when silent or stopped.
    pub momentary_lufs: Option<f32>,
}

pub struct PreviewEngine {
    tx: Sender<Command>,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
}

impl PreviewEngine {
    pub fn spawn<R: Runtime>(app: AppHandle<R>) -> Self {
        Self::spawn_with(app, |rx, app, latest, playhead| {
            crate::bush_adapter::run(rx, app, latest, playhead)
        })
    }

    /// [`spawn`] with the playback loop passed in, so tests can run it with a fake audio output.
    pub(crate) fn spawn_with<R: Runtime>(
        app: AppHandle<R>,
        run: impl Fn(mpsc::Receiver<Command>, AppHandle<R>, LatestFrame, Arc<AtomicU64>)
            + Send
            + Sync
            + 'static,
    ) -> Self {
        let (tx, commands) = mpsc::channel();
        let latest: LatestFrame = Arc::new(Mutex::new(None));
        let playhead_bits = Arc::new(AtomicU64::new(0f64.to_bits()));
        let engine_latest = Arc::clone(&latest);
        let engine_playhead = Arc::clone(&playhead_bits);
        std::thread::Builder::new()
            .name("oca-preview-supervisor".into())
            .spawn(move || {
                supervise(&app, commands, &run, engine_latest, engine_playhead);
            })
            .expect("spawning the preview supervisor thread");
        Self {
            tx,
            latest,
            playhead_bits,
        }
    }

    /// An engine with no playback thread behind it: the receiver hands back every command a
    /// test's code under test sent, so assertions can read them in order.
    #[cfg(test)]
    pub(crate) fn detached() -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel();
        let engine = Self {
            tx,
            latest: Arc::new(Mutex::new(None)),
            playhead_bits: Arc::new(AtomicU64::new(0f64.to_bits())),
        };
        (engine, rx)
    }

    /// The shared frame slot itself, for tests that poison its lock.
    #[cfg(test)]
    pub(crate) fn latest_slot(&self) -> LatestFrame {
        Arc::clone(&self.latest)
    }

    /// Pretends the engine produced `bytes` as its latest preview frame.
    #[cfg(test)]
    pub(crate) fn set_latest_frame(&self, bytes: Vec<u8>) {
        *self.latest.lock().unwrap() = Some(Arc::new(bytes));
    }

    /// Pretends playback reached `secs`, which is what edits read as the live playhead.
    #[cfg(test)]
    pub(crate) fn set_playhead(&self, secs: f64) {
        self.playhead_bits.store(secs.to_bits(), Ordering::Release);
    }

    #[cfg(test)]
    pub fn set_content(&self, project: Project) {
        self.set_content_shared(Arc::new(project));
    }

    /// [`set_content`] for a copy that is also held elsewhere, so it is not cloned again.
    pub fn set_content_shared(&self, project: Arc<Project>) {
        let _ = self.tx.send(Command::Content { project });
    }

    pub fn set_max_dim(&self, dim: u32) {
        let _ = self.tx.send(Command::MaxDim(dim));
    }

    pub fn seek(&self, position_secs: f64) {
        let _ = self.tx.send(Command::Seek(position_secs));
    }

    /// Marks the start and end of a playhead drag; see [`Command::Scrubbing`].
    pub fn set_scrubbing(&self, scrubbing: bool) {
        let _ = self.tx.send(Command::Scrubbing(scrubbing));
    }

    pub fn step(&self, delta_frames: i64) {
        let _ = self.tx.send(Command::Step(delta_frames));
    }

    pub fn set_playing(&self, playing: bool) {
        let _ = self.tx.send(Command::Playing(playing));
    }

    /// The live playhead. Authoritative over `Timeline::playhead_secs` in the session, which
    /// only catches up when an edit or save needs it.
    pub fn playhead(&self) -> f64 {
        f64::from_bits(self.playhead_bits.load(Ordering::Acquire))
    }

    pub fn latest_frame(&self) -> Option<Arc<Vec<u8>>> {
        crate::state::lock_or_recover(&self.latest)
            .as_ref()
            .map(Arc::clone)
    }
}

/// Runs the playback loop on its own thread and keeps it alive: when it panics (a decoder edge
/// case, say) the loop is started again with the project it had and the playhead where it was,
/// so one crash does not leave the preview dead until the app restarts. Commands arrive here and
/// are forwarded, which is what lets a new loop take over the queue.
fn supervise<R: Runtime>(
    app: &AppHandle<R>,
    commands: mpsc::Receiver<Command>,
    run: &(impl Fn(mpsc::Receiver<Command>, AppHandle<R>, LatestFrame, Arc<AtomicU64>)
          + Send
          + Sync
          + 'static),
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
) {
    let mut project: Option<Arc<Project>> = None;
    let mut max_dim: Option<u32> = None;
    let mut crashes: Vec<Instant> = Vec::new();
    let mut first = true;
    loop {
        let (inner_tx, inner_rx) = mpsc::channel();
        if !first {
            if let Some(project) = &project {
                let _ = inner_tx.send(Command::Content {
                    project: Arc::clone(project),
                });
                let secs = f64::from_bits(playhead_bits.load(Ordering::Acquire));
                let _ = inner_tx.send(Command::Seek(secs));
            }
            if let Some(dim) = max_dim {
                let _ = inner_tx.send(Command::MaxDim(dim));
            }
        }
        first = false;
        let worker = {
            let app = app.clone();
            let latest = Arc::clone(&latest);
            let playhead_bits = Arc::clone(&playhead_bits);
            std::thread::scope(|scope| {
                let handle = std::thread::Builder::new()
                    .name("oca-preview-engine".into())
                    .spawn_scoped(scope, || run(inner_rx, app, latest, playhead_bits))
                    .expect("spawning the preview engine thread");
                loop {
                    match commands.recv_timeout(SUPERVISE_INTERVAL) {
                        Ok(command) => {
                            if let Command::Content { project: content } = &command {
                                project = Some(Arc::clone(content));
                            }
                            if let Command::MaxDim(dim) = &command {
                                max_dim = Some(*dim);
                            }
                            let _ = inner_tx.send(command);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if handle.is_finished() {
                                break;
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // The handle was dropped: let the loop see its channel close.
                            drop(inner_tx);
                            let _ = handle.join();
                            return None;
                        }
                    }
                }
                Some(handle.join())
            })
        };
        match worker {
            // Every sender is gone, or the loop ended by itself: nothing left to supervise.
            None | Some(Ok(())) => return,
            Some(Err(_)) => {
                let now = Instant::now();
                crashes.retain(|at| now.duration_since(*at) < RESTART_WINDOW);
                crashes.push(now);
                let restarted = crashes.len() <= MAX_RESTARTS;
                tracing::error!(restarted, "the preview playback thread crashed");
                let _ = app.emit(PREVIEW_RECOVERY, PreviewRecovery { restarted });
                if !restarted {
                    return;
                }
            }
        }
    }
}

/// The source a preview should decode for `asset`: its editing proxy when one exists, else the
/// original -- `None` if neither is on disk (moved/deleted media).
pub(crate) fn preview_source_path(asset: &MediaAsset) -> Option<PathBuf> {
    // The proxy is a cache and can be cleaned away while the original is still there, so a proxy
    // that is not on disk falls back to the original instead of hiding the clip.
    asset
        .proxy_path
        .iter()
        .chain(std::iter::once(&asset.source_path))
        .find(|path| path.exists())
        .cloned()
}

#[cfg(test)]
#[path = "preview_engine_test.rs"]
mod tests;
