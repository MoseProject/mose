// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The audio output the playback loop keeps next to its player. It wraps the operating
//! system's audio device, so it is excluded from coverage together with `bush`'s own
//! `device_sink` (see `spec/architecture/test-coverage-plan.md`); everything the loop decides
//! around it is tested with no device present.

use std::sync::atomic::AtomicBool;

use bush::{AudioPlayer, AudioRenderer, AudioSink, DeviceSink, SceneSnapshot};

use super::backend::MAX_OPEN_SOURCES;

const AUDIO_QUEUE_FRAMES: usize = 192_000;
const AUDIO_BLOCK_FRAMES: usize = 1024;

/// What the playback loop needs from the audio output. The real implementation drives the
/// operating system's device; tests substitute one that keeps time on its own clock.
pub(crate) trait AudioOutput {
    /// Restarts the queue at `secs` of timeline time.
    fn seek_to(&mut self, secs: f64);
    /// Linear peaks of the block being played right now, for the level meter.
    fn peaks(&self) -> (f32, f32);
    /// Momentary loudness (LUFS) around the block being played, `None` when silent.
    fn momentary_lufs(&self) -> Option<f32>;
    /// Timeline seconds the output has actually played.
    fn position_secs(&self) -> f64;
    /// Feeds the queue; recovers a failed device.
    fn pump(&mut self);
    /// Drops everything queued.
    fn clear(&mut self);
    /// Switches to a new scene without restarting playback.
    fn set_scene(&mut self, scene: SceneSnapshot);
    /// How many times the device ran out of audio.
    fn underflow_callbacks(&self) -> u64;
}

/// Opens an output for `scene`, or `None` when there is no usable device.
pub(crate) type OpenAudio = fn(&SceneSnapshot) -> Option<Box<dyn AudioOutput>>;

/// The real output: the default device.
pub(crate) fn open_default_device(scene: &SceneSnapshot) -> Option<Box<dyn AudioOutput>> {
    AudioStack::open(scene).map(|stack| Box::new(stack) as Box<dyn AudioOutput>)
}

/// Audio output kept next to the player: the device queue and the position it was anchored at.
pub(super) struct AudioStack {
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
}

impl AudioOutput for AudioStack {
    fn seek_to(&mut self, secs: f64) {
        self.sink.clear();
        self.player.seek(
            (secs.max(0.0) * f64::from(self.sink.sample_rate())) as u64,
            &mut self.sink,
        );
        self.anchor_consumed = self.sink.consumed_frames();
        self.anchor_secs = secs;
    }

    /// Linear peaks of the block the device is playing right now, for the level meter.
    fn peaks(&self) -> (f32, f32) {
        let frame = (self.position_secs() * f64::from(self.sink.sample_rate())) as u64;
        self.player.peaks_at(frame)
    }

    fn momentary_lufs(&self) -> Option<f32> {
        let frame = (self.position_secs() * f64::from(self.sink.sample_rate())) as u64;
        self.player.momentary_lufs_at(frame)
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

    fn clear(&mut self) {
        self.sink.clear();
    }

    fn set_scene(&mut self, scene: SceneSnapshot) {
        self.player.set_scene(scene, &mut self.sink);
    }

    fn underflow_callbacks(&self) -> u64 {
        self.sink.underflow_callbacks()
    }
}
