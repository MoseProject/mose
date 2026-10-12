// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The Bush playback loop behind `PreviewEngine`: it reads the command channel, fills the
//! latest-frame slot and the playhead atomic, and emits the `preview-tick` event the webview
//! listens to.
//!
//! One thread owns a [`bush::Player`] (video, wall or audio clock) and, once playback starts, a
//! [`bush::DeviceSink`] with an [`bush::AudioPlayer`]. While audio plays, the device clock is the
//! master and video follows it. `Engine::submit` cancels the render in flight, so the loop asks
//! for the next frame only after the previous one arrived (or a safety timeout), otherwise a
//! render slower than the poll interval would never finish.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avcore::nested_sequence::{
    cache_dir_for_project, materialize_nested_sequences, NestedSequenceCache,
};
use avcore::timeline::{Timeline, TrackKind};
use avcore::{Lut3D as CoreLut, MediaAsset, MediaKind};
use bush::{
    DecodePolicy, Engine, EngineConfig, Player, PlayerEvent, RendererChoice, SceneSnapshot,
    SceneVersions, Version,
};
use tauri::{AppHandle, Emitter, Runtime};

use super::audio::{open_default_device, AudioOutput, OpenAudio};
use super::mapper::{map_timeline, Approximation, OverlayCache, SourceResolver};
use crate::frames::{encode_jpeg_rgba, PREVIEW_MAX_DIM, PREVIEW_MIN_DIM};
use crate::preview_engine::{
    preview_source_path, Command, LatestFrame, PreviewTick, FALLBACK_FPS, POLL_INTERVAL,
    PREVIEW_TICK, TICK_INTERVAL,
};

const DEFAULT_CANVAS: (u32, u32) = (1280, 720);
pub(super) const MAX_OPEN_SOURCES: usize = 6;
const FRAME_CACHE_BYTES: usize = 192 * 1024 * 1024;
/// A render that has not answered after this long is assumed lost and the next frame is asked for.
const OUTSTANDING_TIMEOUT: Duration = Duration::from_millis(250);
/// Poll cadence while playing: short enough to request the next frame right after one lands.
/// How long a seek made while the playhead is dragged may decode forward before it shows the newest
/// frame it has. About the cost of reaching a key frame, so a drag over a long-GOP file follows the
/// pointer instead of waiting a second per position.
const SCRUB_SEEK_BUDGET: Duration = Duration::from_millis(30);
const PLAYING_POLL_INTERVAL: Duration = Duration::from_millis(1);
/// How long the loop waits for a command when no render or tick is pending.
const IDLE_WAIT: Duration = Duration::from_millis(250);

/// Resolves proxies and parses LUTs once per path for the mapper.
#[derive(Default)]
struct AppResolver {
    luts: HashMap<String, Option<Arc<bush::Lut3D>>>,
}

impl SourceResolver for AppResolver {
    fn source_path(&mut self, asset: &MediaAsset) -> Option<std::path::PathBuf> {
        preview_source_path(asset)
    }

    fn lut(&mut self, path: &str) -> Option<Arc<bush::Lut3D>> {
        self.luts
            .entry(path.to_owned())
            .or_insert_with(|| {
                let parsed = CoreLut::load(Path::new(path)).ok()?;
                bush::Lut3D::new(parsed.size(), parsed.entries().to_vec())
                    .ok()
                    .map(Arc::new)
            })
            .clone()
    }
}

/// The canvas the preview composites at: the first video clip's resolution, shrunk to the preview
/// size so Bush never renders pixels the webview would throw away.
fn preview_canvas(timeline: &Timeline, media: &[MediaAsset], max_dim: u32) -> (u32, u32) {
    let resolution = timeline
        .tracks
        .iter()
        .filter(|track| track.visible && track.kind == TrackKind::Video)
        .flat_map(|track| track.clips.iter())
        .find_map(|clip| {
            media
                .iter()
                .find(|asset| asset.id == clip.resolved_asset_id())
                .filter(|asset| asset.kind == MediaKind::Video)
                .and_then(|asset| asset.resolution)
        });
    let Some((width, height)) = resolution.filter(|(w, h)| *w > 0 && *h > 0) else {
        return DEFAULT_CANVAS;
    };
    let scale = (max_dim as f32 / width.max(height) as f32).min(1.0);
    let even = |value: f32| ((value as u32).max(2)) & !1;
    (even(width as f32 * scale), even(height as f32 * scale))
}

fn renderer_choice() -> RendererChoice {
    match std::env::var("OCA_BUSH_RENDERER").as_deref() {
        Ok("gpu") => RendererChoice::GpuWithCpuFallback,
        _ => RendererChoice::Cpu,
    }
}

/// Software decode unless `OCA_BUSH_DECODE=hardware`. On the development machine Bush's
/// hardware path is slower (first frame 406 ms against 93 ms, render p50 6.1 ms against 4.3 ms
/// at 1080p), so it stays a developer switch until a zero-copy path exists.
fn hardware_decode_requested() -> bool {
    matches!(std::env::var("OCA_BUSH_DECODE").as_deref(), Ok("hardware"))
}

fn engine_config(
    canvas: (u32, u32),
    hardware_decode: bool,
    renderer: RendererChoice,
) -> EngineConfig {
    EngineConfig {
        canvas_width: canvas.0,
        canvas_height: canvas.1,
        max_open_sources: MAX_OPEN_SOURCES,
        frame_cache_bytes: FRAME_CACHE_BYTES,
        decode_policy: if hardware_decode {
            DecodePolicy::HardwareWithFallback
        } else {
            DecodePolicy::Software
        },
        renderer,
    }
}

fn new_engine(canvas: (u32, u32)) -> Option<Engine> {
    let hardware_decode = hardware_decode_requested();
    tracing::info!(hardware_decode, "bush engine decode policy");
    Engine::new(engine_config(canvas, hardware_decode, renderer_choice()))
        .map_err(|error| tracing::error!(?error, "bush engine failed to start"))
        .ok()
}

/// Whether two projects look the same in the preview: the same media and the same tracks in every
/// sequence (compound clips read the others). Everything else (names, export settings, layout,
/// the stored playhead) is destructured and ignored on purpose, so a new field in these structs
/// has to be placed on one side or the other here.
fn same_preview_content(a: &avcore::Project, b: &avcore::Project) -> bool {
    let avcore::Project {
        id,
        name: _,
        last_edited: _,
        summary: _,
        media_library,
        sequences,
        active_sequence,
        file_path,
        panel_layout: _,
        smart_bins: _,
        recent_asset_ids: _,
        frame_rate: _,
    } = a;
    if *id != b.id
        || *active_sequence != b.active_sequence
        || *file_path != b.file_path
        || *media_library != b.media_library
        || sequences.len() != b.sequences.len()
    {
        return false;
    }
    sequences.iter().zip(&b.sequences).all(|(x, y)| {
        let avcore::Sequence {
            id,
            name: _,
            timeline,
            export_settings: _,
        } = x;
        let avcore::Timeline {
            tracks,
            playhead_secs: _,
            markers,
            multicam_groups,
        } = timeline;
        *id == y.id
            && *tracks == y.timeline.tracks
            && *markers == y.timeline.markers
            && *multicam_groups == y.timeline.multicam_groups
    })
}

pub fn run<R: Runtime>(
    rx: mpsc::Receiver<Command>,
    app: AppHandle<R>,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
) {
    run_with_audio(rx, app, latest, playhead_bits, open_default_device);
}

/// [`run`] with the audio output factory passed in.
pub(crate) fn run_with_audio<R: Runtime>(
    rx: mpsc::Receiver<Command>,
    app: AppHandle<R>,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
    open_audio: OpenAudio,
) {
    let started = Instant::now();
    let mut resolver = AppResolver::default();
    let mut overlay_cache = OverlayCache::default();
    let mut timeline = Timeline {
        tracks: Vec::new(),
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    let mut media: Vec<MediaAsset> = Vec::new();
    // The project the scene was last built from.
    let mut shown: Option<Arc<avcore::Project>> = None;
    let mut nested_cache: HashMap<u64, NestedSequenceCache> = HashMap::new();
    let mut player: Option<Player> = None;
    let mut canvas = DEFAULT_CANVAS;
    let mut audio: Option<Box<dyn AudioOutput>> = None;
    let mut scene: Option<SceneSnapshot> = None;
    let mut approximations: BTreeSet<Approximation> = BTreeSet::new();
    let mut revision = 0u64;
    let mut playing = false;
    let mut available = false;
    let mut error: Option<String> = None;
    let mut frame_seq = 0u64;
    let mut dirty = true;
    // The playhead is being dragged: seeks settle for a nearby frame of a long GOP.
    let mut scrubbing = false;
    // The longest side to composite at: what the monitor shows, up to PREVIEW_MAX_DIM.
    let mut max_dim = PREVIEW_MAX_DIM;
    let mut last_tick = Instant::now();
    let mut outstanding_since: Option<Instant> = None;
    // When a rendered frame was handed to the JPEG thread and its picture has not shown up yet.
    let mut encoding_since: Option<Instant> = None;
    let mut stats = PlaybackStats::new();
    let encoder = spawn_encoder(Arc::clone(&latest));
    let mut encoded_seen = 0u64;
    // Frames are only worth rendering at the source's rate; asking faster just re-renders and
    // re-encodes the same picture.
    let mut min_frame_interval = Duration::ZERO;
    let mut last_submit = Instant::now();

    loop {
        let mut commands = Vec::new();
        // While playing, a finished render is the thing to react to, so poll tightly; idle, the
        // usual cadence is plenty. A tight `recv_timeout` is no use on Windows, where it is as
        // coarse as the system timer (a 1 ms wait takes 15.5 ms): that quantised the preview ticks
        // to 31 ms, so 60 fps playback showed 34 pictures a second. `sleep` is high resolution.
        if playing {
            match rx.try_recv() {
                Ok(command) => commands.push(command),
                Err(TryRecvError::Empty) => std::thread::sleep(PLAYING_POLL_INTERVAL),
                Err(TryRecvError::Disconnected) => break,
            }
        } else {
            // Nothing in flight: a command is the only thing worth waking for, so wait for one
            // instead of polling 60 times a second (idle wakeups are what keeps a laptop's cores
            // out of deep sleep).
            let in_flight = dirty
                || outstanding_since.is_some()
                || encoding_since.is_some_and(|since| since.elapsed() < OUTSTANDING_TIMEOUT);
            let wait = if in_flight { POLL_INTERVAL } else { IDLE_WAIT };
            match rx.recv_timeout(wait) {
                Ok(command) => commands.push(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
        commands.extend(rx.try_iter());

        let mut rebuild = false;
        for command in commands {
            match command {
                Command::Content { project: next } => {
                    // Renames, export settings, panel layout and a playhead that only caught up
                    // change nothing the preview shows: rebuilding the scene for them would
                    // re-render the frame and restart the decoders' bookkeeping for no reason.
                    if shown
                        .as_ref()
                        .is_some_and(|shown| same_preview_content(shown, &next))
                    {
                        continue;
                    }
                    timeline = next.timeline().clone();
                    shown = Some(Arc::clone(&next));
                    media = next.media_library.clone();
                    let cache_dir = cache_dir_for_project(&next);
                    match materialize_nested_sequences(
                        &next,
                        &timeline,
                        &cache_dir,
                        &mut nested_cache,
                    ) {
                        Ok(nested_assets) => media.extend(nested_assets.into_values()),
                        Err(error) => {
                            tracing::warn!(%error, "failed to materialize nested sequence for preview");
                        }
                    }
                    rebuild = true;
                }
                Command::MaxDim(requested) => {
                    let requested = requested.clamp(PREVIEW_MIN_DIM, PREVIEW_MAX_DIM);
                    if requested != max_dim {
                        max_dim = requested;
                        // A new canvas size: rebuild the scene at it (once there is one).
                        rebuild |= shown.is_some();
                    }
                }
                Command::Seek(position) => {
                    if let Some(player) = player.as_mut() {
                        seek(
                            player,
                            audio.as_deref_mut(),
                            position,
                            &mut outstanding_since,
                        );
                    } else {
                        playhead_bits.store(position.max(0.0).to_bits(), Ordering::Release);
                    }
                    dirty = true;
                }
                Command::Scrubbing(next) => {
                    scrubbing = next;
                    // Dropping out of it shows the exact frame at the playhead.
                    dirty = true;
                }
                Command::Step(delta_frames) => {
                    let fps = first_video_fps(&timeline, &media);
                    if let Some(player) = player.as_mut() {
                        let target = player.position_secs() + delta_frames as f64 / fps;
                        seek(player, audio.as_deref_mut(), target, &mut outstanding_since);
                    }
                    dirty = true;
                }
                Command::Playing(next) => {
                    if let Some(player) = player.as_mut() {
                        if next {
                            if let Err(error) = player.play(started.elapsed()) {
                                tracing::warn!(?error, "bush play failed");
                            } else {
                                playing = true;
                                if audio.is_none() {
                                    audio = scene.as_ref().and_then(open_audio);
                                }
                                if let Some(audio) = audio.as_mut() {
                                    audio.seek_to(player.position_secs());
                                }
                                outstanding_since = Some(Instant::now());
                            }
                        } else {
                            player.pause();
                            playing = false;
                            if let Some(audio) = audio.as_mut() {
                                audio.clear();
                            }
                        }
                    }
                    dirty = true;
                }
            }
        }

        if let Some(player) = player.as_ref() {
            player
                .engine()
                .set_seek_budget(scrubbing.then_some(SCRUB_SEEK_BUDGET));
        }
        if rebuild {
            revision += 1;
            min_frame_interval = Duration::from_secs_f64(0.9 / first_video_fps(&timeline, &media));
            let next_canvas = preview_canvas(&timeline, &media, max_dim);
            let duration = timeline.duration_secs();
            let versions = SceneVersions {
                source: Version(revision),
                scene: Version(revision),
                presentation: Version(revision),
            };
            match map_timeline(
                &timeline,
                &media,
                next_canvas,
                &mut resolver,
                &mut overlay_cache,
                versions,
            ) {
                Ok(mapped) => {
                    if mapped.approximations != approximations {
                        tracing::info!(approximations = ?mapped.approximations, "bush scene approximations");
                        approximations = mapped.approximations;
                    }
                    let next_scene = mapped.scene;
                    let overlays = mapped.overlays;
                    if duration > 0.0 && !next_scene.contributors().is_empty() {
                        if next_canvas != canvas {
                            player = None;
                            canvas = next_canvas;
                        }
                        let result = match player.as_mut() {
                            Some(player) => player
                                .set_scene(next_scene.clone(), overlays, duration)
                                .map(|_| ()),
                            None => new_engine(canvas)
                                .ok_or(bush::PlayerError::InvalidDuration(duration))
                                .and_then(|engine| {
                                    Player::new(engine, next_scene.clone(), duration)
                                })
                                .and_then(|mut created| {
                                    created.set_scene(next_scene.clone(), overlays, duration)?;
                                    created.seek(timeline.playhead_secs.clamp(0.0, duration))?;
                                    player = Some(created);
                                    Ok(())
                                }),
                        };
                        match result {
                            Ok(()) => {
                                error = None;
                                outstanding_since = Some(Instant::now());
                                if let Some(audio) = audio.as_mut() {
                                    audio.set_scene(next_scene.clone());
                                }
                            }
                            Err(failure) => error = Some(format!("{failure:?}")),
                        }
                        scene = Some(next_scene);
                    } else {
                        player = None;
                        scene = None;
                        playing = false;
                        available = false;
                        *crate::state::lock_or_recover(&latest) = None;
                        frame_seq = frame_seq.wrapping_add(1);
                    }
                }
                Err(failure) => error = Some(format!("scene rejected: {failure:?}")),
            }
            dirty = true;
        }

        if let Some(player) = player.as_mut() {
            // Finished renders first, so the next frame is requested the moment one arrives
            // instead of a poll interval later. Only the newest matters; older ones were
            // superseded. JPEG encoding runs on its own thread, overlapping the next render.
            let mut newest = None;
            while let Some(event) = player.poll() {
                match event {
                    PlayerEvent::Frame { frame, .. } => newest = Some(frame),
                    PlayerEvent::Ended { .. } => {
                        playing = false;
                        if let Some(audio) = audio.as_mut() {
                            audio.clear();
                        }
                        dirty = true;
                    }
                    PlayerEvent::Failed { message, .. } => {
                        error = Some(message);
                        outstanding_since = None;
                        dirty = true;
                    }
                }
            }
            if let Some(frame) = newest {
                outstanding_since = None;
                encoding_since = Some(Instant::now());
                let _ = encoder.tx.send(frame);
            }

            if playing {
                let now = started.elapsed();
                if let Some(audio) = audio.as_mut() {
                    audio.pump();
                }
                let ready = outstanding_since
                    .is_none_or(|since| since.elapsed() > OUTSTANDING_TIMEOUT)
                    && last_submit.elapsed() >= min_frame_interval;
                if ready {
                    let submitted = match audio.as_ref() {
                        Some(audio) => player.tick_synced(now, audio.position_secs()),
                        None => player.tick(now),
                    };
                    match submitted {
                        Ok(Some(_)) => {
                            outstanding_since = Some(Instant::now());
                            last_submit = Instant::now();
                        }
                        Ok(None) => {}
                        Err(failure) => error = Some(format!("{failure:?}")),
                    }
                }
            }

            playhead_bits.store(player.position_secs().to_bits(), Ordering::Release);
            if playing {
                let drift_ms = audio
                    .as_ref()
                    .map(|audio| player.drift_against(audio.position_secs()) * 1000.0);
                let underflows = audio.as_ref().map(|audio| audio.underflow_callbacks());
                stats.report(player.engine(), &encoder.shared, drift_ms, underflows);
            } else {
                stats.reset(&encoder.shared);
            }
        }

        let encoded = encoder.shared.encoded_seq.load(Ordering::Acquire);
        if encoded != encoded_seen {
            encoded_seen = encoded;
            encoding_since = None;
            available = true;
            error = None;
            dirty = true;
        }

        if dirty && (!playing || last_tick.elapsed() >= TICK_INTERVAL) {
            let (peak_l, peak_r) = audio
                .as_ref()
                .filter(|_| playing)
                .map_or((0.0, 0.0), |audio| audio.peaks());
            let lufs = audio
                .as_ref()
                .filter(|_| playing)
                .and_then(|audio| audio.momentary_lufs());
            let tick = PreviewTick {
                frame_seq: frame_seq.wrapping_add(encoded),
                playhead_secs: f64::from_bits(playhead_bits.load(Ordering::Acquire)),
                playing,
                has_clip: scene.is_some(),
                available,
                error: error.clone(),
                peak_l,
                peak_r,
                momentary_lufs: lufs,
            };
            if let Err(error) = app.emit(PREVIEW_TICK, tick) {
                tracing::warn!(%error, "failed to emit preview tick");
            }
            last_tick = Instant::now();
            dirty = false;
        }
    }
}

/// What the encoder thread publishes: a sequence number the loop watches, plus per-window
/// timing counters the stats report drains.
#[derive(Default)]
struct EncoderShared {
    encoded_seq: AtomicU64,
    frames: AtomicU64,
    micros_total: AtomicU64,
    micros_max: AtomicU64,
}

struct Encoder {
    tx: mpsc::Sender<bush::VideoFrame>,
    shared: Arc<EncoderShared>,
}

/// JPEG-encodes frames off the playback thread into the latest-frame slot. If frames queue up
/// while one encodes, only the newest is encoded.
fn spawn_encoder(latest: LatestFrame) -> Encoder {
    let (tx, rx) = mpsc::channel::<bush::VideoFrame>();
    let shared = Arc::new(EncoderShared::default());
    let worker = Arc::clone(&shared);
    std::thread::Builder::new()
        .name("oca-bush-jpeg".into())
        .spawn(move || {
            while let Ok(mut frame) = rx.recv() {
                while let Ok(newer) = rx.try_recv() {
                    frame = newer;
                }
                let started = Instant::now();
                let jpeg = encode_jpeg_rgba(
                    frame.width(),
                    frame.height(),
                    frame.pixels(),
                    PREVIEW_MAX_DIM,
                );
                let micros = started.elapsed().as_micros() as u64;
                worker.frames.fetch_add(1, Ordering::Relaxed);
                worker.micros_total.fetch_add(micros, Ordering::Relaxed);
                worker.micros_max.fetch_max(micros, Ordering::Relaxed);
                if let Some(jpeg) = jpeg {
                    *crate::state::lock_or_recover(&latest) = Some(Arc::new(jpeg));
                    worker.encoded_seq.fetch_add(1, Ordering::Release);
                }
            }
        })
        .expect("spawning the bush jpeg thread");
    Encoder { tx, shared }
}

/// Per-window playback health, logged every [`STATS_WINDOW`] while playing so a report says
/// whether a stall came from rendering, JPEG encoding or A/V drift.
struct PlaybackStats {
    window_start: Instant,
}

const STATS_WINDOW: Duration = Duration::from_secs(2);

impl PlaybackStats {
    fn new() -> Self {
        Self {
            window_start: Instant::now(),
        }
    }

    fn reset(&mut self, encoder: &EncoderShared) {
        self.window_start = Instant::now();
        encoder.frames.store(0, Ordering::Relaxed);
        encoder.micros_total.store(0, Ordering::Relaxed);
        encoder.micros_max.store(0, Ordering::Relaxed);
    }

    fn report(
        &mut self,
        engine: &Engine,
        encoder: &EncoderShared,
        drift_ms: Option<f64>,
        underflows: Option<u64>,
    ) {
        let elapsed = self.window_start.elapsed();
        if elapsed < STATS_WINDOW {
            return;
        }
        let latency = engine.latency();
        let counters = engine.stats();
        let frames = encoder.frames.load(Ordering::Relaxed);
        let fps = frames as f64 / elapsed.as_secs_f64();
        let encode_avg_ms = if frames == 0 {
            0.0
        } else {
            encoder.micros_total.load(Ordering::Relaxed) as f64 / 1000.0 / frames as f64
        };
        let encode_max_ms = encoder.micros_max.load(Ordering::Relaxed) as f64 / 1000.0;
        tracing::info!(
            target: "bush_stats",
            fps = format_args!("{fps:.1}"),
            render_p50_ms = latency.p50_micros as f64 / 1000.0,
            render_p95_ms = latency.p95_micros as f64 / 1000.0,
            render_max_ms = latency.max_micros as f64 / 1000.0,
            encode_avg_ms = format_args!("{encode_avg_ms:.2}"),
            encode_max_ms = encode_max_ms,
            cache_hits = counters.cache_hits,
            cache_misses = counters.cache_misses,
            superseded = counters.superseded,
            gpu_fallbacks = counters.gpu_fallbacks,
            av_drift_ms = ?drift_ms,
            audio_underflows = ?underflows,
            "bush playback"
        );
        self.reset(encoder);
    }
}

fn seek(
    player: &mut Player,
    audio: Option<&mut (dyn AudioOutput + 'static)>,
    position_secs: f64,
    outstanding_since: &mut Option<Instant>,
) {
    match player.seek(position_secs) {
        Ok(_) => {
            *outstanding_since = Some(Instant::now());
            if let Some(audio) = audio {
                audio.seek_to(player.position_secs());
            }
        }
        Err(failure) => tracing::warn!(?failure, "bush seek failed"),
    }
}

fn first_video_fps(timeline: &Timeline, media: &[MediaAsset]) -> f64 {
    timeline
        .tracks
        .iter()
        .filter(|track| track.visible && track.kind == TrackKind::Video)
        .flat_map(|track| track.clips.iter())
        .find_map(|clip| {
            media
                .iter()
                .find(|asset| asset.id == clip.resolved_asset_id())
        })
        .and_then(|asset| asset.fps)
        .map(f64::from)
        .filter(|fps| *fps >= 1.0)
        .unwrap_or(FALLBACK_FPS)
}

#[cfg(test)]
#[path = "backend_test.rs"]
mod tests;
