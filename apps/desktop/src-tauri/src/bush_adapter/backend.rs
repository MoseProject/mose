// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The Bush playback loop behind `PreviewEngine`: same command channel, latest-frame slot,
//! playhead atomic and `preview-tick` event as the GStreamer loop, so the rest of the app cannot
//! tell which engine is running.
//!
//! One thread owns a [`bush::Player`] (video, wall or audio clock) and, once playback starts, a
//! [`bush::DeviceSink`] with an [`bush::AudioPlayer`]. While audio plays, the device clock is the
//! master and video follows it. `Engine::submit` cancels the render in flight, so the loop asks
//! for the next frame only after the previous one arrived (or a safety timeout), otherwise a
//! render slower than the poll interval would never finish.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avcore::nested_sequence::{
    cache_dir_for_project, materialize_nested_sequences, NestedSequenceCache,
};
use avcore::timeline::{Timeline, TrackKind};
use avcore::{Lut3D as CoreLut, MediaAsset, MediaKind};
use bush::{
    AudioPlayer, AudioRenderer, AudioSink, DecodePolicy, DeviceSink, Engine, EngineConfig, Player,
    PlayerEvent, RendererChoice, SceneSnapshot, SceneVersions, Version,
};
use tauri::{AppHandle, Emitter};

use super::mapper::{map_timeline, Approximation, SourceResolver};
use crate::frames::{encode_jpeg_rgba, PREVIEW_MAX_DIM};
use crate::preview_engine::{
    preview_source_path, Command, LatestFrame, PreviewTick, FALLBACK_FPS, POLL_INTERVAL,
    PREVIEW_TICK, TICK_INTERVAL,
};

const DEFAULT_CANVAS: (u32, u32) = (1280, 720);
const MAX_OPEN_SOURCES: usize = 6;
const FRAME_CACHE_BYTES: usize = 192 * 1024 * 1024;
const AUDIO_QUEUE_FRAMES: usize = 192_000;
const AUDIO_BLOCK_FRAMES: usize = 1024;
/// A render that has not answered after this long is assumed lost and the next frame is asked for.
const OUTSTANDING_TIMEOUT: Duration = Duration::from_millis(250);

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

/// Audio output kept next to the player: the device queue and the position it was anchored at.
struct AudioStack {
    sink: DeviceSink,
    player: AudioPlayer,
    anchor_consumed: u64,
    anchor_secs: f64,
}

impl AudioStack {
    fn open(scene: &SceneSnapshot) -> Option<Self> {
        let sink = DeviceSink::open_default(AUDIO_QUEUE_FRAMES).ok()?;
        let rate = sink.sample_rate();
        let renderer = AudioRenderer::new(rate, sink.channels(), 4096, MAX_OPEN_SOURCES).ok()?;
        let player = AudioPlayer::new(
            renderer,
            scene.clone(),
            AUDIO_BLOCK_FRAMES,
            rate as usize / 4,
        );
        Some(Self {
            sink,
            player,
            anchor_consumed: 0,
            anchor_secs: 0.0,
        })
    }

    fn seek_to(&mut self, secs: f64) {
        self.sink.clear();
        self.player.seek(
            (secs.max(0.0) * f64::from(self.sink.sample_rate())) as u64,
            &mut self.sink,
        );
        self.anchor_consumed = self.sink.consumed_frames();
        self.anchor_secs = secs;
    }

    /// Timeline seconds the device has actually played.
    fn position_secs(&self) -> f64 {
        let played = self
            .sink
            .consumed_frames()
            .saturating_sub(self.anchor_consumed);
        self.anchor_secs + played as f64 / f64::from(self.sink.sample_rate())
    }

    fn pump(&mut self) {
        static NEVER: AtomicBool = AtomicBool::new(false);
        if self.sink.is_failed() {
            if self.sink.recover().is_ok() {
                let secs = self.position_secs();
                self.seek_to(secs);
            }
            return;
        }
        if let Err(error) = self.player.pump(&mut self.sink, &NEVER) {
            tracing::warn!(?error, "bush audio pump failed");
        }
    }
}

/// The canvas the preview composites at: the first video clip's resolution, shrunk to the preview
/// size so Bush never renders pixels the webview would throw away.
fn preview_canvas(timeline: &Timeline, media: &[MediaAsset]) -> (u32, u32) {
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
    let scale = (PREVIEW_MAX_DIM as f32 / width.max(height) as f32).min(1.0);
    let even = |value: f32| ((value as u32).max(2)) & !1;
    (even(width as f32 * scale), even(height as f32 * scale))
}

fn renderer_choice() -> RendererChoice {
    match std::env::var("OCA_BUSH_RENDERER").as_deref() {
        Ok("gpu") => RendererChoice::GpuWithCpuFallback,
        _ => RendererChoice::Cpu,
    }
}

fn new_engine(canvas: (u32, u32), hardware_decode: bool) -> Option<Engine> {
    Engine::new(EngineConfig {
        canvas_width: canvas.0,
        canvas_height: canvas.1,
        max_open_sources: MAX_OPEN_SOURCES,
        frame_cache_bytes: FRAME_CACHE_BYTES,
        decode_policy: if hardware_decode {
            DecodePolicy::HardwareWithFallback
        } else {
            DecodePolicy::Software
        },
        renderer: renderer_choice(),
    })
    .map_err(|error| tracing::error!(?error, "bush engine failed to start"))
    .ok()
}

pub fn run(
    rx: mpsc::Receiver<Command>,
    app: AppHandle,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
) {
    let started = Instant::now();
    let mut resolver = AppResolver::default();
    let mut timeline = Timeline {
        tracks: Vec::new(),
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    let mut media: Vec<MediaAsset> = Vec::new();
    let mut nested_cache: HashMap<u64, NestedSequenceCache> = HashMap::new();
    let mut player: Option<Player> = None;
    let mut canvas = DEFAULT_CANVAS;
    let mut audio: Option<AudioStack> = None;
    let mut scene: Option<SceneSnapshot> = None;
    let mut approximations: BTreeSet<Approximation> = BTreeSet::new();
    let mut hardware_decode = false;
    let mut revision = 0u64;
    let mut playing = false;
    let mut available = false;
    let mut error: Option<String> = None;
    let mut frame_seq = 0u64;
    let mut dirty = true;
    let mut last_tick = Instant::now();
    let mut outstanding_since: Option<Instant> = None;

    loop {
        let mut commands = Vec::new();
        match rx.recv_timeout(POLL_INTERVAL) {
            Ok(command) => commands.push(command),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        commands.extend(rx.try_iter());

        let mut rebuild = false;
        for command in commands {
            match command {
                Command::Content { project: next } => {
                    timeline = next.timeline().clone();
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
                Command::Seek(position) => {
                    if let Some(player) = player.as_mut() {
                        seek(player, audio.as_mut(), position, &mut outstanding_since);
                    } else {
                        playhead_bits.store(position.max(0.0).to_bits(), Ordering::Release);
                    }
                    dirty = true;
                }
                Command::Step(delta_frames) => {
                    let fps = first_video_fps(&timeline, &media);
                    if let Some(player) = player.as_mut() {
                        let target = player.position_secs() + delta_frames as f64 / fps;
                        seek(player, audio.as_mut(), target, &mut outstanding_since);
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
                                    audio = scene.as_ref().and_then(AudioStack::open);
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
                                audio.sink.clear();
                            }
                        }
                    }
                    dirty = true;
                }
                Command::HardwareDecode(enabled) => {
                    if hardware_decode != enabled {
                        hardware_decode = enabled;
                        player = None;
                        rebuild = true;
                    }
                }
            }
        }

        if rebuild {
            revision += 1;
            let next_canvas = preview_canvas(&timeline, &media);
            let duration = timeline.duration_secs();
            let versions = SceneVersions {
                source: Version(revision),
                scene: Version(revision),
                presentation: Version(revision),
            };
            match map_timeline(&timeline, &media, next_canvas, &mut resolver, versions) {
                Ok(mapped) => {
                    if mapped.approximations != approximations {
                        tracing::info!(approximations = ?mapped.approximations, "bush scene approximations");
                        approximations = mapped.approximations;
                    }
                    let next_scene = mapped.scene;
                    if duration > 0.0 && !next_scene.contributors().is_empty() {
                        if next_canvas != canvas {
                            player = None;
                            canvas = next_canvas;
                        }
                        let result = match player.as_mut() {
                            Some(player) => player
                                .set_scene(next_scene.clone(), Vec::new(), duration)
                                .map(|_| ()),
                            None => new_engine(canvas, hardware_decode)
                                .ok_or(bush::PlayerError::InvalidDuration(duration))
                                .and_then(|engine| {
                                    Player::new(engine, next_scene.clone(), duration)
                                })
                                .and_then(|mut created| {
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
                                    audio.player.set_scene(next_scene.clone(), &mut audio.sink);
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
                        if let Ok(mut frame) = latest.lock() {
                            *frame = None;
                        }
                        frame_seq = frame_seq.wrapping_add(1);
                    }
                }
                Err(failure) => error = Some(format!("scene rejected: {failure:?}")),
            }
            dirty = true;
        }

        if let Some(player) = player.as_mut() {
            let now = started.elapsed();
            if playing {
                if let Some(audio) = audio.as_mut() {
                    audio.pump();
                }
                let ready =
                    outstanding_since.is_none_or(|since| since.elapsed() > OUTSTANDING_TIMEOUT);
                if ready {
                    let submitted = match audio.as_ref() {
                        Some(audio) => player.tick_synced(now, audio.position_secs()),
                        None => player.tick(now),
                    };
                    match submitted {
                        Ok(Some(_)) => outstanding_since = Some(Instant::now()),
                        Ok(None) => {}
                        Err(failure) => error = Some(format!("{failure:?}")),
                    }
                }
            }

            // Keep only the newest finished frame; older ones were superseded.
            let mut newest = None;
            while let Some(event) = player.poll() {
                match event {
                    PlayerEvent::Frame { frame, .. } => newest = Some(frame),
                    PlayerEvent::Ended { .. } => {
                        playing = false;
                        if let Some(audio) = audio.as_mut() {
                            audio.sink.clear();
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
                if let Some(jpeg) = encode_jpeg_rgba(
                    frame.width(),
                    frame.height(),
                    frame.pixels(),
                    PREVIEW_MAX_DIM,
                ) {
                    if let Ok(mut slot) = latest.lock() {
                        *slot = Some(Arc::new(jpeg));
                    }
                    frame_seq = frame_seq.wrapping_add(1);
                    available = true;
                    error = None;
                }
                dirty = true;
            }
            playhead_bits.store(player.position_secs().to_bits(), Ordering::Release);
        }

        if dirty && (!playing || last_tick.elapsed() >= TICK_INTERVAL) {
            let tick = PreviewTick {
                frame_seq,
                playhead_secs: f64::from_bits(playhead_bits.load(Ordering::Acquire)),
                playing,
                has_clip: scene.is_some(),
                available,
                error: error.clone(),
                peak_l: 0.0,
                peak_r: 0.0,
            };
            if let Err(error) = app.emit(PREVIEW_TICK, tick) {
                tracing::warn!(%error, "failed to emit preview tick");
            }
            last_tick = Instant::now();
            dirty = false;
        }
    }
}

fn seek(
    player: &mut Player,
    audio: Option<&mut AudioStack>,
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
