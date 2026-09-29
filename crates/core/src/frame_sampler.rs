// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! A shared seek-and-poll frame sampling primitive against an already-open [`Preview`]
//! pipeline — the shape `ui`'s auto-reframe, motion tracking, and background-removal matte
//! generation background-thread functions each independently reimplemented before this existed
//! (open a decode-only `Preview`, seek to a timestamp, poll `current_frame()` until it decodes
//! or a deadline passes, skip the sample on timeout rather than failing the whole run). See
//! `spec/architecture/performance-and-caching.md` §5.
//!
//! Deliberately doesn't know what a sampled frame is *for* — face detection, block matching,
//! and person segmentation all stay exactly where they were, in each `ui` feature's own
//! background-thread function; this only owns the open/seek/poll mechanics and the "N evenly
//! spaced sample times across a duration, clamped" bookkeeping every multi-sample caller needed
//! on top of it.

use std::path::Path;
use std::time::{Duration, Instant};

use crate::preview::{Preview, PreviewError, VideoFrame};

/// A decode-only [`Preview`] (no display, no effects — opened with `clip: None`) plus the
/// polling cadence [`Self::sample`] waits on. Cheap to construct once per background-thread run
/// and reused across every sample, same as every caller already did by hand.
pub struct FrameSampler {
    preview: Preview,
    poll_interval: Duration,
}

impl FrameSampler {
    /// Opens `path` for sampling. `poll_interval` is how often [`Self::sample`] re-checks
    /// `Preview::current_frame` after a seek while waiting for a frame to actually decode.
    pub fn open(path: &Path, poll_interval: Duration) -> Result<Self, PreviewError> {
        Ok(Self {
            preview: Preview::open(path, None)?,
            poll_interval,
        })
    }

    /// Seeks to `at_secs` (clamped to `0.0`) and waits up to `deadline` for a frame to decode.
    /// `None` on timeout — every caller already tolerated a slow/missing sample by just
    /// shrinking its result rather than failing the whole run over one frame, so this makes
    /// that the primitive's own contract instead of three independent copies of it.
    pub fn sample(&self, at_secs: f64, deadline: Duration) -> Option<VideoFrame> {
        let _ = self.preview.seek(at_secs.max(0.0));
        let deadline_at = Instant::now() + deadline;
        loop {
            if let Some(frame) = self.preview.current_frame() {
                return Some(frame);
            }
            if Instant::now() >= deadline_at {
                return None;
            }
            std::thread::sleep(self.poll_interval);
        }
    }

    /// `sample_count` timestamps evenly spaced across `[start_secs, end_secs]`, where
    /// `sample_count = round(duration * samples_per_sec)` clamped to
    /// `[min_samples, max_samples]` — the "dense enough to follow motion, bounded for a long
    /// clip" shape motion-tracking/background-removal each computed inline with their own rate
    /// and bounds. Empty if `end_secs <= start_secs`.
    pub fn even_sample_times(
        start_secs: f64,
        end_secs: f64,
        samples_per_sec: f64,
        min_samples: usize,
        max_samples: usize,
    ) -> Vec<f64> {
        let duration = (end_secs - start_secs).max(0.0);
        if duration <= 0.0 {
            return Vec::new();
        }
        let min_samples = min_samples.min(max_samples);
        let sample_count =
            ((duration * samples_per_sec).round() as usize).clamp(min_samples, max_samples);
        (0..sample_count)
            .map(|i| {
                if sample_count == 1 {
                    start_secs
                } else {
                    start_secs + duration * (i as f64 / (sample_count - 1) as f64)
                }
            })
            .collect()
    }
}

#[cfg(test)]
#[path = "frame_sampler/frame_sampler_test.rs"]
mod tests;
