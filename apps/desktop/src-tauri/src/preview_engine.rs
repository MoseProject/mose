// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Playback coordinator for the Tauri shell.
//!
//! The egui shell drives preview from its per-frame UI loop (`ui::App::ensure_preview_loaded`/
//! `pump_preview_frame`). A webview has no such loop on the Rust side, so this module owns one:
//! a dedicated thread that receives timeline snapshots and transport commands, decides which
//! clips cover the playhead, and drives [`avcore::preview_worker::PreviewWorker`] (the same
//! GStreamer-owning worker the egui shell uses). Decoded frames are JPEG-encoded into a
//! one-slot [`LatestFrame`] served by the `oca://` protocol; the webview learns a new one exists
//! from the throttled [`PREVIEW_TICK`] event.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use avcore::nested_sequence::{
    cache_dir_for_project, materialize_nested_sequences, NestedSequenceCache,
};
use avcore::preview_worker::{
    clip_seek_offset, LiveUpdateKey, PreviewOpenRequest, PreviewSeek, PreviewStatus, PreviewWorker,
};
use avcore::timeline::{ClipInstance, ShapeClip, TextClip, Timeline, TrackKind};
use avcore::{
    live_preview_updates, DeflickerHistory, Lut3D, MediaAsset, PreviewCpuEffects, Project,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::frames::{encode_jpeg, PREVIEW_MAX_DIM};

/// Event carrying a [`PreviewTick`].
pub const PREVIEW_TICK: &str = "preview-tick";

/// Upper bound on tick events per second while playing -- enough for a smooth playhead
/// without flooding the IPC bridge.
const TICK_INTERVAL: Duration = Duration::from_millis(16);
/// How long the engine parks waiting for a command before polling the worker again. Matches
/// the worker's own poll cadence.
const POLL_INTERVAL: Duration = Duration::from_millis(8);
/// Fallback fps for frame stepping when the clip's asset has none (same guess as `avcore::render`).
const FALLBACK_FPS: f64 = 30.0;

/// The most recent encoded preview frame, shared with the `oca://` protocol handler.
pub type LatestFrame = Arc<Mutex<Option<Arc<Vec<u8>>>>>;

enum Command {
    Content { project: Box<Project> },
    Seek(f64),
    Step(i64),
    Playing(bool),
    HardwareDecode(bool),
}

/// Snapshot of transport state for the webview.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTick {
    /// Increments every time [`LatestFrame`] changes.
    pub frame_seq: u64,
    pub playhead_secs: f64,
    pub playing: bool,
    /// A clip covers the playhead (whether or not its pipeline opened).
    pub has_clip: bool,
    /// The pipeline for the clip at the playhead has produced a frame.
    pub available: bool,
    pub error: Option<String>,
    pub peak_l: f32,
    pub peak_r: f32,
}

pub struct PreviewEngine {
    tx: Sender<Command>,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
}

impl PreviewEngine {
    pub fn spawn(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let latest: LatestFrame = Arc::new(Mutex::new(None));
        let playhead_bits = Arc::new(AtomicU64::new(0f64.to_bits()));
        let engine_latest = Arc::clone(&latest);
        let engine_playhead = Arc::clone(&playhead_bits);
        std::thread::Builder::new()
            .name("oca-preview-engine".into())
            .spawn(move || run(rx, app, engine_latest, engine_playhead))
            .expect("spawning the preview engine thread");
        Self {
            tx,
            latest,
            playhead_bits,
        }
    }

    pub fn set_content(&self, project: Project) {
        let _ = self.tx.send(Command::Content {
            project: Box::new(project),
        });
    }

    pub fn seek(&self, position_secs: f64) {
        let _ = self.tx.send(Command::Seek(position_secs));
    }

    pub fn step(&self, delta_frames: i64) {
        let _ = self.tx.send(Command::Step(delta_frames));
    }

    pub fn set_playing(&self, playing: bool) {
        let _ = self.tx.send(Command::Playing(playing));
    }

    /// Changes the decoder policy for subsequent opens. The current pipeline is reopened.
    pub fn set_hardware_decode(&self, enabled: bool) {
        let _ = self.tx.send(Command::HardwareDecode(enabled));
    }

    /// The live playhead. Authoritative over `Timeline::playhead_secs` in the session, which
    /// only catches up when an edit or save needs it.
    pub fn playhead(&self) -> f64 {
        f64::from_bits(self.playhead_bits.load(Ordering::Acquire))
    }

    pub fn latest_frame(&self) -> Option<Arc<Vec<u8>>> {
        self.latest
            .lock()
            .ok()
            .and_then(|frame| frame.as_ref().map(Arc::clone))
    }
}

/// Everything that must be baked into a pipeline at open time. Two playhead positions with
/// equal `Layers` can share one pipeline via a seek; any difference requires a reopen.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Layers {
    pub background: Option<(PathBuf, ClipInstance)>,
    pub overlays: Vec<(PathBuf, ClipInstance)>,
    pub audio: Vec<(PathBuf, ClipInstance)>,
    pub text: Vec<TextClip>,
    pub shapes: Vec<ShapeClip>,
}

impl Layers {
    fn is_composited(&self) -> bool {
        !(self.overlays.is_empty()
            && self.audio.is_empty()
            && self.text.is_empty()
            && self.shapes.is_empty())
    }
}

/// Returns all live updates when layer identity and every pipeline-shaping field are unchanged.
/// Text, shapes, audio, paths, and missing layers still require a reopen.
fn live_updates_between(
    previous: &Layers,
    next: &Layers,
) -> Option<Vec<avcore::LivePreviewUpdate>> {
    let mut updates = Vec::new();
    let mut compare_video = |before: &(PathBuf, ClipInstance), after: &(PathBuf, ClipInstance)| {
        if before.0 != after.0 {
            return None;
        }
        let next_updates = live_preview_updates(&before.1, &after.1)?;
        updates.extend(next_updates);
        Some(())
    };
    match (&previous.background, &next.background) {
        (Some(before), Some(after)) => compare_video(before, after)?,
        (None, None) => {}
        _ => return None,
    }
    if previous.overlays.len() != next.overlays.len()
        || previous.audio != next.audio
        || previous.text != next.text
        || previous.shapes != next.shapes
    {
        return None;
    }
    for (before, after) in previous.overlays.iter().zip(&next.overlays) {
        compare_video(before, after)?;
    }
    Some(updates)
}

/// The source a preview pipeline should decode for `asset`: its editing proxy when one
/// exists, else the original -- `None` if neither is on disk (moved/deleted media).
fn preview_source_path(asset: &MediaAsset) -> Option<PathBuf> {
    let path = asset
        .proxy_path
        .clone()
        .unwrap_or_else(|| asset.source_path.clone());
    path.exists().then_some(path)
}

/// Resolves which clips cover `at_secs`. The background is the first visible video track with
/// a clip there; later visible video tracks composite over it. A compound clip resolves via
/// `resolved_asset_id()` against the synthetic asset `Command::Content`'s handler merges into
/// `media` (see `materialize_nested_sequences`); clips whose media still can't be found are
/// left out rather than failing the whole preview.
pub(crate) fn resolve_layers(timeline: &Timeline, media: &[MediaAsset], at_secs: f64) -> Layers {
    let with_source = |clip: &ClipInstance| {
        media
            .iter()
            .find(|asset| asset.id == clip.resolved_asset_id())
            .and_then(preview_source_path)
            .map(|path| (path, clip.clone()))
    };
    let active = |start: f64, duration: f64| at_secs >= start && at_secs < start + duration;

    let mut layers = Layers::default();
    for track in timeline.tracks.iter().filter(|t| t.visible) {
        match track.kind {
            TrackKind::Video => {
                let Some(layer) = track.clip_at(at_secs).and_then(with_source) else {
                    continue;
                };
                if layers.background.is_none() {
                    layers.background = Some(layer);
                } else {
                    layers.overlays.push(layer);
                }
            }
            TrackKind::Audio => {
                if let Some(layer) = track.clip_at(at_secs).and_then(with_source) {
                    layers.audio.push(layer);
                }
            }
            TrackKind::Text => layers.text.extend(
                track
                    .text_clips
                    .iter()
                    .find(|c| active(c.start_secs, c.duration_secs))
                    .cloned(),
            ),
            TrackKind::Shape => layers.shapes.extend(
                track
                    .shape_clips
                    .iter()
                    .find(|c| active(c.start_secs, c.duration_secs))
                    .cloned(),
            ),
        }
    }
    layers
}

fn open_request(
    layers: &Layers,
    playhead_secs: f64,
    hardware_decode: bool,
) -> Option<PreviewOpenRequest> {
    let (path, clip) = layers.background.clone()?;
    Some(if layers.is_composited() {
        PreviewOpenRequest::Composited {
            path,
            clip,
            overlays: layers.overlays.clone(),
            audio: layers.audio.clone(),
            text: layers.text.clone(),
            shapes: layers.shapes.clone(),
            playhead_secs,
            hardware_decode,
        }
    } else {
        PreviewOpenRequest::Single {
            path,
            clip,
            playhead_secs,
            hardware_decode,
        }
    })
}

fn refresh_text_highlights(worker: &PreviewWorker, layers: &Layers, playhead_secs: f64) {
    if layers.text.is_empty() {
        return;
    }
    let timed_clips: Vec<_> = layers
        .text
        .iter()
        .cloned()
        .map(|clip| {
            let local_time = playhead_secs - clip.start_secs;
            (clip, local_time)
        })
        .collect();
    worker.live(LiveUpdateKey::global("text"), move |preview| {
        let refs: Vec<_> = timed_clips
            .iter()
            .map(|(clip, time)| (clip, *time))
            .collect();
        if let Err(error) = preview.update_text_overlays(&refs) {
            tracing::warn!(%error, "failed to refresh preview text overlays");
        }
    });
}

/// Seek parameters in the branch order [`open_request`] built the pipeline with.
pub(crate) fn seek_for(layers: &Layers, playhead_secs: f64) -> Option<PreviewSeek> {
    let (_, background) = layers.background.as_ref()?;
    let rate = |clip: &ClipInstance| clip.speed_factor.max(0.01) as f64;
    Some(if layers.is_composited() {
        let (offsets, rates) = std::iter::once(background)
            .chain(layers.overlays.iter().map(|(_, clip)| clip))
            .chain(layers.audio.iter().map(|(_, clip)| clip))
            .map(|clip| (clip_seek_offset(clip, playhead_secs), rate(clip)))
            .unzip();
        PreviewSeek::Composited { offsets, rates }
    } else {
        PreviewSeek::Single {
            offset_secs: clip_seek_offset(background, playhead_secs),
            rate: rate(background),
        }
    })
}

/// Maps the background pipeline's source position back to timeline time. Past the clip's
/// trimmed out point it lands exactly on the clip's end, so the next resolve picks whatever
/// follows the cut.
pub(crate) fn timeline_position(clip: &ClipInstance, source_position_secs: f64) -> f64 {
    if source_position_secs >= clip.source_out_secs {
        clip.start_secs + clip.duration_secs()
    } else {
        let speed = clip.speed_factor.max(0.01) as f64;
        clip.start_secs + (source_position_secs - clip.source_in_secs) / speed
    }
}

fn run(
    rx: mpsc::Receiver<Command>,
    app: AppHandle,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
) {
    let worker = PreviewWorker::spawn();
    let mut timeline = Timeline {
        tracks: Vec::new(),
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    let mut media: Vec<MediaAsset> = Vec::new();
    let mut playhead = 0.0f64;
    let mut playing = false;
    let mut hardware_decode = true;
    // `None` until the first resolve, so an empty timeline still emits one initial tick.
    let mut loaded: Option<Layers> = None;
    let mut generation = 0u64;
    let mut open_generation = 0u64;
    let mut available = false;
    let mut error: Option<String> = None;
    let mut seek_pending = false;
    // Wall-clock playback for stretches no pipeline clock drives (gaps, frozen clips).
    let mut wall_clock: Option<(Instant, f64)> = None;
    let mut frame_seq = 0u64;
    let mut last_frame_key: Option<(u64, Option<u64>)> = None;
    let mut audio = avcore::AudioLevel::default();
    let mut dirty = true;
    let mut last_tick = Instant::now();
    let mut lut_cache: Option<(String, Option<Lut3D>)> = None;
    let mut deflicker_history: (Option<u64>, DeflickerHistory) = (None, DeflickerHistory::new());
    let mut effect_seed = 0u64;
    // Persists across `Command::Content`s so an edit unrelated to any nested sequence doesn't
    // force a re-encode -- mirrors `ui::App`'s own `nested_sequence_render_state` cache, keyed
    // the same way (`NestedSequenceCache`'s `Timeline`-equality check).
    let mut nested_cache: HashMap<u64, NestedSequenceCache> = HashMap::new();

    loop {
        let mut commands = Vec::new();
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => commands.push(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        commands.extend(rx.try_iter());

        for command in commands {
            match command {
                Command::Content { project: next } => {
                    timeline = next.timeline().clone();
                    media = next.media_library.clone();
                    // A real FFmpeg re-encode per nested sequence -- blocks this dedicated
                    // preview thread only (never the UI thread or a session-mutating command),
                    // same "known cost, deliberately not hidden" tradeoff `nested_sequence`'s own
                    // doc comment records for `ui`'s synchronous export path. Cached by `Timeline`
                    // equality, so an edit that doesn't touch a nested sequence's content is free.
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
                    playhead = playhead.clamp(0.0, timeline.duration_secs());
                    dirty = true;
                }
                Command::Seek(position) => {
                    playhead = position.clamp(0.0, timeline.duration_secs());
                    seek_pending = true;
                    wall_clock = None;
                    dirty = true;
                }
                Command::Step(delta_frames) => {
                    let fps = loaded
                        .as_ref()
                        .and_then(|l| l.background.as_ref())
                        .and_then(|(_, clip)| media.iter().find(|a| a.id == clip.asset_id))
                        .and_then(|asset| asset.fps)
                        .map(f64::from)
                        .filter(|fps| *fps >= 1.0)
                        .unwrap_or(FALLBACK_FPS);
                    playhead =
                        (playhead + delta_frames as f64 / fps).clamp(0.0, timeline.duration_secs());
                    seek_pending = true;
                    dirty = true;
                }
                Command::Playing(next) => {
                    let end = timeline.duration_secs();
                    if next && playhead >= end - 1e-3 {
                        playhead = 0.0;
                        seek_pending = true;
                    }
                    playing = next && end > 0.0;
                    wall_clock = None;
                    let pipeline_clock = loaded
                        .as_ref()
                        .and_then(|l| l.background.as_ref())
                        .is_some_and(|(_, clip)| !clip.frozen);
                    worker.set_playing(playing && pipeline_clock);
                    dirty = true;
                }
                Command::HardwareDecode(enabled) => {
                    if hardware_decode != enabled {
                        hardware_decode = enabled;
                        loaded = None;
                        dirty = true;
                    }
                }
            }
        }

        let layers = resolve_layers(&timeline, &media, playhead);
        let live_updates = loaded
            .as_ref()
            .and_then(|previous| live_updates_between(previous, &layers));
        if !seek_pending && loaded.as_ref().is_some() && live_updates.is_some() {
            for update in live_updates.unwrap_or_default() {
                worker.live_update(update);
            }
            loaded = Some(layers.clone());
        } else if loaded.as_ref() != Some(&layers) {
            match open_request(&layers, playhead, hardware_decode) {
                Some(request) => {
                    generation = generation.wrapping_add(1);
                    open_generation = generation;
                    worker.open(generation, request);
                    let frozen = layers
                        .background
                        .as_ref()
                        .is_some_and(|(_, clip)| clip.frozen);
                    worker.set_playing(playing && !frozen);
                }
                None => {
                    worker.set_playing(false);
                    if let Ok(mut frame) = latest.lock() {
                        *frame = None;
                    }
                    frame_seq = frame_seq.wrapping_add(1);
                }
            }
            available = false;
            error = None;
            loaded = Some(layers);
            seek_pending = false;
            dirty = true;
        } else if seek_pending {
            if let Some(seek) = seek_for(&layers, playhead) {
                generation = generation.wrapping_add(1);
                worker.seek(generation, seek);
            }
            seek_pending = false;
        }

        while let Some(status) = worker.take_status() {
            match status {
                PreviewStatus::Ready { generation: g } if g == open_generation => {
                    dirty = true;
                }
                PreviewStatus::Failed {
                    generation: g,
                    message,
                } if g == open_generation => {
                    available = false;
                    error = Some(message);
                    dirty = true;
                }
                _ => {}
            }
        }

        let background = loaded
            .as_ref()
            .and_then(|l| l.background.as_ref())
            .map(|(_, clip)| clip);
        if let Some(ready) = worker.take_frame() {
            if let Some(clip) = background.filter(|_| ready.generation >= open_generation) {
                available = true;
                audio = ready.audio_level;
                if playing && !clip.frozen {
                    if let Some(position) = ready.position_secs {
                        playhead = timeline_position(clip, position);
                        wall_clock = None;
                    }
                }
                // Paused pipelines hand back the same preroll sample on every poll.
                let key = (ready.generation, ready.position_secs.map(f64::to_bits));
                if last_frame_key != Some(key) {
                    last_frame_key = Some(key);
                    let mut frame = ready.frame;
                    let effects = PreviewCpuEffects::for_clip(Some(clip));
                    if deflicker_history.0 != effects.clip_id {
                        deflicker_history = (effects.clip_id, DeflickerHistory::new());
                    }
                    if !effects.lut_path.is_empty() {
                        let reparse = lut_cache
                            .as_ref()
                            .is_none_or(|(path, _)| path != &effects.lut_path);
                        if reparse {
                            lut_cache = Some((
                                effects.lut_path.clone(),
                                Lut3D::load(std::path::Path::new(&effects.lut_path)).ok(),
                            ));
                        }
                        if let Some((_, Some(lut))) = &lut_cache {
                            avcore::apply_lut_to_rgba(&mut frame.rgba, lut);
                        }
                    }
                    if effects.vignette_intensity > 0.0 {
                        avcore::apply_vignette_to_rgba(
                            &mut frame.rgba,
                            frame.width,
                            frame.height,
                            effects.vignette_intensity,
                        );
                    }
                    if effects.glitch_intensity > 0.0 {
                        effect_seed = effect_seed.wrapping_add(1);
                        avcore::apply_glitch_to_rgba(
                            &mut frame.rgba,
                            effects.glitch_intensity,
                            effect_seed,
                        );
                    }
                    if effects.deflicker_enabled {
                        avcore::apply_deflicker_to_rgba(&mut frame.rgba, &mut deflicker_history.1);
                    }
                    if let Some(jpeg) = encode_jpeg(&frame, PREVIEW_MAX_DIM) {
                        if let Ok(mut frame) = latest.lock() {
                            *frame = Some(Arc::new(jpeg));
                        }
                        frame_seq = frame_seq.wrapping_add(1);
                    }
                }
                dirty = true;
            }
        }

        if playing && background.is_none_or(|clip| clip.frozen) {
            let (started, from) = *wall_clock.get_or_insert((Instant::now(), playhead));
            playhead = from + started.elapsed().as_secs_f64();
            dirty = true;
        }

        if let Some(layers) = &loaded {
            refresh_text_highlights(&worker, layers, playhead);
        }
        let end = timeline.duration_secs();
        if playing && playhead >= end {
            playhead = end;
            playing = false;
            wall_clock = None;
            worker.set_playing(false);
            dirty = true;
        }

        playhead_bits.store(playhead.to_bits(), Ordering::Release);
        if dirty && (!playing || last_tick.elapsed() >= TICK_INTERVAL) {
            let tick = PreviewTick {
                frame_seq,
                playhead_secs: playhead,
                playing,
                has_clip: background.is_some(),
                available,
                error: error.clone(),
                peak_l: audio.peak_l,
                peak_r: audio.peak_r,
            };
            if let Err(error) = app.emit(PREVIEW_TICK, tick) {
                tracing::warn!(%error, "failed to emit preview tick");
            }
            last_tick = Instant::now();
            dirty = false;
        }
    }
    worker.shutdown();
}

#[cfg(test)]
#[path = "preview_engine_test.rs"]
mod tests;
