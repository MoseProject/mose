// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

//! Watches a folder for finished gameplay recordings and cleans up their audio automatically —
//! the in-app replacement for the standalone `scripts/Watch-Gameplay.ps1` utility (its own
//! `ui.html` documents the pipeline this module mirrors: detect → wait for the recorder to
//! finish writing → measure loudness → normalize → measure again).
//!
//! Reuses [`crate::render::render_export`] (video passthrough-copied, audio decoded through
//! `afftdn` noise reduction + `loudnorm` + a true-peak `alimiter`, re-encoded to AAC) rather
//! than a new filter chain — that's already this app's own normalize-export primitive, unused
//! by any UI caller before this feature, and already covers noise reduction, loudness
//! normalization, and peak safety. **Documented gap, not silently dropped**: the standalone
//! script's `highpass=f=80` (sub-80Hz rumble cut) and `acompressor` (voice-leveling compressor)
//! steps aren't in `render_export`'s chain — adding them would change the filter chain every
//! export caller shares, not just this feature, so it's left for a follow-up that weighs that
//! wider blast radius deliberately rather than as a side effect of this feature.
//!
//! Also deliberately not ported: the script's exclusive-file-lock check (`Test-FileReady`,
//! `FileShare::None`) as a second signal alongside "size unchanged for N seconds" that a
//! recorder is done writing. That needs a platform-specific open-with-no-sharing call this
//! codebase has no existing dependency for (Windows: `CreateFileW` with `dwShareMode=0`); the
//! size-stable-for-N-seconds heuristic alone is what most watched-folder tools rely on in
//! practice, and is what's implemented here — a real, not fake, gap.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use crate::media::LoudnessMetrics;

/// Extensions the watcher recognizes as gameplay recordings — matches
/// `Watch-Gameplay.ps1`'s own `$videoExtensions` list exactly.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mkv", "mov", "avi", "flv", "ts", "m4v"];

/// Default source-loudness target passed to [`crate::render::render_export`] — matches
/// `Watch-Gameplay.ps1`'s own `loudnorm=I=-16` and this crate's [`crate::loudness`] module's own
/// stated default.
pub const DEFAULT_TARGET_LUFS: f32 = -16.0;

/// How long a file's size must stay unchanged before it's considered done being written —
/// matches the script's own `$StableSeconds` default.
pub const DEFAULT_STABLE_SECS: u64 = 6;

/// The subfolder (created under the watched folder) that cleaned-up copies are written into —
/// matches the script's own `$OutputFolderName` default.
pub const DEFAULT_OUTPUT_SUBFOLDER: &str = "processed";

/// True if `path`'s extension (case-insensitively) is one this watcher processes.
pub fn is_video_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let lower = e.to_ascii_lowercase();
            VIDEO_EXTENSIONS.contains(&lower.as_str())
        })
        .unwrap_or(false)
}

/// Where a cleaned-up copy of `source` (a file directly inside `watch_dir`) is written —
/// `watch_dir/output_subfolder/<source's file name>`, unchanged extension (matches the script's
/// own `Join-Path $outputPath $file.Name`, no re-muxing to a different container).
pub fn output_path_for(watch_dir: &Path, output_subfolder: &str, source: &Path) -> PathBuf {
    watch_dir
        .join(output_subfolder)
        .join(source.file_name().unwrap_or_default())
}

/// Tracks each watched file's size across polls to decide when it's stopped changing —
/// `now`/instant-based rather than reading the system clock itself, so this stays a plain,
/// deterministically-testable function. One instance persists across the whole watch session
/// (the caller owns it, matching `Watch-Gameplay.ps1`'s own `$tracked` hashtable).
#[derive(Default)]
pub struct StabilityTracker {
    entries: HashMap<PathBuf, TrackedEntry>,
}

struct TrackedEntry {
    size: u64,
    stable_since: Instant,
}

impl StabilityTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records `path`'s current `size` as of `now` and returns whether it's been unchanged for
    /// at least `stable_for`. A size change (including the very first observation) resets the
    /// stability clock — mirrors the script's own "if the size changed, `StableSince = Get-Date`"
    /// logic exactly.
    pub fn poll(&mut self, path: &Path, size: u64, now: Instant, stable_for: Duration) -> bool {
        let entry = self
            .entries
            .entry(path.to_path_buf())
            .or_insert_with(|| TrackedEntry {
                size,
                stable_since: now,
            });
        if entry.size != size {
            entry.size = size;
            entry.stable_since = now;
            return false;
        }
        now.duration_since(entry.stable_since) >= stable_for
    }

    /// Drops a file's tracked state — call once it's been fully processed (or permanently
    /// errored) so a later re-poll of the same path starts fresh, matching the script's own
    /// `$tracked.Remove(...)` on success.
    pub fn forget(&mut self, path: &Path) {
        self.entries.remove(path);
    }
}

/// What [`process_watched_file`] failed on.
#[derive(Debug)]
pub enum ProcessError {
    /// Couldn't measure the source file's loudness before processing.
    MeasureBefore(crate::loudness::LoudnessError),
    /// Couldn't probe the source file's duration (needed for progress reporting).
    Probe(crate::probe::ProbeError),
    /// The actual render (video-copy + normalize) step failed.
    Render(crate::render::RenderError),
    /// Couldn't measure the output file's loudness after processing.
    MeasureAfter(crate::loudness::LoudnessError),
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessError::MeasureBefore(e) => write!(f, "failed to measure source loudness: {e}"),
            ProcessError::Probe(e) => write!(f, "failed to probe source: {e}"),
            ProcessError::Render(e) => write!(f, "failed to render: {e}"),
            ProcessError::MeasureAfter(e) => write!(f, "failed to measure output loudness: {e}"),
        }
    }
}

impl std::error::Error for ProcessError {}

/// The full one-file pipeline: measure `source`'s loudness, normalize it into `output` (video
/// passthrough-copied, per [`crate::render::render_export`]), then measure `output`'s loudness
/// — the before/after pair the review UI shows, mirroring `Watch-Gameplay.ps1`'s own
/// `analyzing-in` → `processing` → `analyzing-out` stages. `output`'s parent directory is
/// created if missing. Returns `Ok(None)` if `cancel` was set mid-render (the caller's own
/// cancellation, not an error) rather than a partial before/after pair.
pub fn process_watched_file(
    source: &Path,
    output: &Path,
    target_lufs: f32,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u8),
) -> Result<Option<(LoudnessMetrics, LoudnessMetrics)>, ProcessError> {
    let before = crate::loudness::measure_loudness(source).map_err(ProcessError::MeasureBefore)?;

    let probed = crate::probe::probe_media(source).map_err(ProcessError::Probe)?;

    if let Some(parent) = output.parent() {
        // Best-effort — a failure here surfaces naturally as a render error immediately after
        // (can't open the output path), no need for a distinct error variant.
        let _ = std::fs::create_dir_all(parent);
    }

    let outcome = crate::render::render_export(
        source,
        output,
        target_lufs,
        probed.duration_secs,
        cancel,
        &mut on_progress,
    )
    .map_err(ProcessError::Render)?;

    if outcome == crate::render::RenderOutcome::Cancelled {
        return Ok(None);
    }

    let after = crate::loudness::measure_loudness(output).map_err(ProcessError::MeasureAfter)?;
    Ok(Some((before, after)))
}

#[cfg(test)]
#[path = "watched_folder/watched_folder_test.rs"]
mod tests;
