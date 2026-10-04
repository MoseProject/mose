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

//! Computes per-bucket amplitude peaks of an asset's audio track, for drawing a waveform in
//! the timeline's audio clips — via `oca-avbridge`'s FFI, no subprocess. Like [`crate::proxy`],
//! this is a full decode pass, so callers run it on a background thread and cache the result
//! rather than call it from the UI thread (see `ui`'s import enrichment pipeline).

use std::path::Path;

#[derive(Debug)]
pub enum WaveformError {
    /// `oca-avbridge` failed before or during the decode/filter pipeline.
    Bridge(avbridge::WaveformError),
}

impl std::fmt::Display for WaveformError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WaveformError::Bridge(e) => write!(f, "failed to compute waveform: {e}"),
        }
    }
}

impl std::error::Error for WaveformError {}

/// How many (min, max) buckets [`generate_waveform`] computes across an asset's full duration.
/// Fixed rather than tied to any particular timeline zoom level — the timeline panel
/// subsamples/interpolates from this array to whatever pixel width a clip is drawn at, the
/// same way the fixed-resolution source data behind any other zoomable UI is resampled for
/// display.
pub const WAVEFORM_BUCKET_COUNT: usize = 2000;

/// Computes [`WAVEFORM_BUCKET_COUNT`] (min, max) amplitude peaks — each in `[-1.0, 1.0]` — of
/// `path`'s audio track, downmixed to mono and spread evenly across its full duration.
pub fn generate_waveform(path: &Path) -> Result<Vec<(f32, f32)>, WaveformError> {
    avbridge::generate_waveform(path, WAVEFORM_BUCKET_COUNT).map_err(WaveformError::Bridge)
}

/// Detail waveform resolution: buckets per second of audio. High enough that a clip zoomed to
/// the timeline's maximum scale still has several buckets per pixel's worth of time.
pub const WAVEFORM_DETAIL_BUCKETS_PER_SEC: f64 = 50.0;
/// Upper bound on [`generate_detailed_waveform`]'s bucket count, so a very long recording cannot
/// ask for an unbounded array (about 12 MB of `f32` pairs).
pub const WAVEFORM_DETAIL_MAX_BUCKETS: usize = 1_500_000;

/// Like [`generate_waveform`] but with [`WAVEFORM_DETAIL_BUCKETS_PER_SEC`] buckets per second of
/// `duration_secs` (never fewer than [`WAVEFORM_BUCKET_COUNT`]), for drawing zoomed-in clips.
pub fn generate_detailed_waveform(
    path: &Path,
    duration_secs: f64,
) -> Result<Vec<(f32, f32)>, WaveformError> {
    avbridge::generate_waveform(path, detailed_bucket_count(duration_secs))
        .map_err(WaveformError::Bridge)
}

/// The bucket count [`generate_detailed_waveform`] asks for.
pub fn detailed_bucket_count(duration_secs: f64) -> usize {
    let wanted = (duration_secs.max(0.0) * WAVEFORM_DETAIL_BUCKETS_PER_SEC).ceil() as usize;
    wanted.clamp(WAVEFORM_BUCKET_COUNT, WAVEFORM_DETAIL_MAX_BUCKETS)
}
