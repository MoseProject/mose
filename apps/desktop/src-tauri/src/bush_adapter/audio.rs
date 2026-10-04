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

/// Audio output kept next to the player: the device queue and the position it was anchored at.
pub(super) struct AudioStack {
    pub(super) sink: DeviceSink,
    pub(super) player: AudioPlayer,
    anchor_consumed: u64,
    anchor_secs: f64,
}

impl AudioStack {
    pub(super) fn open(scene: &SceneSnapshot) -> Option<Self> {
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

    pub(super) fn seek_to(&mut self, secs: f64) {
        self.sink.clear();
        self.player.seek(
            (secs.max(0.0) * f64::from(self.sink.sample_rate())) as u64,
            &mut self.sink,
        );
        self.anchor_consumed = self.sink.consumed_frames();
        self.anchor_secs = secs;
    }

    /// Linear peaks of the block the device is playing right now, for the level meter.
    pub(super) fn peaks(&self) -> (f32, f32) {
        let frame = (self.position_secs() * f64::from(self.sink.sample_rate())) as u64;
        self.player.peaks_at(frame)
    }

    /// Timeline seconds the device has actually played.
    pub(super) fn position_secs(&self) -> f64 {
        let played = self
            .sink
            .consumed_frames()
            .saturating_sub(self.anchor_consumed);
        self.anchor_secs + played as f64 / f64::from(self.sink.sample_rate())
    }

    pub(super) fn pump(&mut self) {
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
