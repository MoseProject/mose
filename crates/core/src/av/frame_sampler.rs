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

//! A shared frame sampling primitive: open a media file decode-only, then ask for the frame at a
//! timestamp. Auto-reframe, motion tracking, scene detection, background removal, privacy blur
//! and the thumbnail protocol each need "the picture at T", skipping a sample that cannot be
//! decoded rather than failing the whole run. See `spec/architecture/performance-and-caching.md`
//! §5.
//!
//! Decoding is Bush's [`bush::DecodeSource`] (software decode through `avbridge`, the same seek
//! and "frame at or before T" logic the preview uses), not a playback pipeline.
//!
//! Deliberately doesn't know what a sampled frame is *for* — face detection, block matching,
//! and person segmentation all stay exactly where they were, in each feature's own
//! background-thread function; this only owns the open/seek mechanics and the "N evenly
//! spaced sample times across a duration, clamped" bookkeeping every multi-sample caller needed
//! on top of it.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use bush::DecodeSource;

/// One decoded frame as tightly packed RGBA8 (`width * height * 4` bytes).
#[derive(Clone, Debug, PartialEq)]
pub struct SampledFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Why a file could not be opened for sampling.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SamplerError(pub String);

impl std::fmt::Display for SamplerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "frame sampler: {}", self.0)
    }
}

impl std::error::Error for SamplerError {}

/// A decode-only source for one file. Cheap to construct once per background-thread run and
/// reused across every sample; the decoder keeps its position, so samples in increasing time
/// order are cheaper than random ones.
pub struct FrameSampler {
    source: Mutex<DecodeSource>,
}

impl FrameSampler {
    /// Opens `path` for sampling.
    pub fn open(path: &Path) -> Result<Self, SamplerError> {
        let source =
            DecodeSource::open(path).map_err(|error| SamplerError(format!("{error:?}")))?;
        Ok(Self {
            source: Mutex::new(source),
        })
    }

    /// The frame at (or just before) `at_secs`, clamped to `0.0`. `None` when the sample cannot
    /// be produced: the decoder failed, the stream has no frame, or it took longer than
    /// `deadline` (checked once the decode returns; decoding itself is bounded by the
    /// decoder's seek search limit, so a bad file cannot hang). Every caller already tolerated
    /// a slow or missing sample by shrinking its result rather than failing the whole run over
    /// one frame, so that is this primitive's own contract.
    pub fn sample(&self, at_secs: f64, deadline: Duration) -> Option<SampledFrame> {
        let started = Instant::now();
        let never_cancelled = AtomicBool::new(false);
        let frame = self
            .source
            .lock()
            .ok()?
            .frame_at(at_secs.max(0.0), &never_cancelled)
            .ok()??;
        if started.elapsed() > deadline {
            return None;
        }
        let (width, height) = (frame.width(), frame.height());
        let row_bytes = width as usize * 4;
        let stride = frame.stride_bytes();
        let pixels = frame.pixels();
        let rgba = if stride == row_bytes {
            pixels.get(..row_bytes * height as usize)?.to_vec()
        } else {
            let mut packed = Vec::with_capacity(row_bytes * height as usize);
            for row in 0..height as usize {
                packed.extend_from_slice(pixels.get(row * stride..row * stride + row_bytes)?);
            }
            packed
        };
        Some(SampledFrame {
            width,
            height,
            rgba,
        })
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
