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

//! CF-03 slice 3 (`spec/architecture/competitive-feature-plan.md`): renders two short samples
//! of a `Mic`-role clip's own source audio — voice cleanup bypassed vs. applied with its
//! currently staged parameters — plus each sample's measured loudness/peak/range, so a user can
//! judge the effect before committing to it. Deliberately *not* a live GStreamer preview effect
//! (that gap is separately documented on `ClipInstance::voice_cleanup_enabled`'s own doc comment
//! — most effects' elements only conditionally exist in the running pipeline at all, a
//! materially bigger lift): this renders two real, disposable files through the exact same
//! mixing path a real export uses ([`avbridge::mix_audio_timeline`]), then measures each with
//! the same [`crate::loudness::measure_loudness`] primitive already used elsewhere. A caller
//! (`ui`) plays each file back through its own preview pipeline.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::loudness::{self, LoudnessError};
use crate::media::LoudnessMetrics;

/// Upper bound on how much of the clip's own source audio gets rendered for the A/B sample —
/// long enough to actually judge the compressor/limiter's effect on real speech, short enough
/// to render close to instantly and not turn this into a miniature export.
pub const PREVIEW_SAMPLE_MAX_SECS: f64 = 6.0;

/// Voice-cleanup filter parameters, mirroring `avcore::timeline::ClipInstance`'s own four
/// fields — this module doesn't depend on `timeline::ClipInstance` directly so a caller can pass
/// whatever values the properties panel currently has staged, even mid-edit before they're
/// committed to the clip itself.
#[derive(Debug, Clone, Copy)]
pub struct VoiceCleanupParams {
    pub noise_floor_db: f32,
    pub compressor_threshold_db: f32,
    pub compressor_ratio: f32,
    pub ceiling_linear: f32,
}

#[derive(Debug)]
pub enum VoiceCleanupPreviewError {
    /// Couldn't create `out_dir`.
    Io(std::io::Error),
    Mix(avbridge::AudioMixError),
    Measure(LoudnessError),
}

impl std::fmt::Display for VoiceCleanupPreviewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VoiceCleanupPreviewError::Io(e) => {
                write!(f, "failed to prepare the preview output directory: {e}")
            }
            VoiceCleanupPreviewError::Mix(e) => write!(f, "failed to render the preview: {e}"),
            VoiceCleanupPreviewError::Measure(e) => {
                write!(f, "failed to measure the rendered preview: {e}")
            }
        }
    }
}

impl std::error::Error for VoiceCleanupPreviewError {}

/// One rendered A/B sample plus its own measured [`LoudnessMetrics`].
#[derive(Debug, Clone)]
pub struct VoiceCleanupPreviewSample {
    pub path: PathBuf,
    pub metrics: LoudnessMetrics,
}

#[derive(Debug, Clone)]
pub struct VoiceCleanupPreviewResult {
    /// The clip's source audio, unprocessed (`voice_cleanup_enabled: false`).
    pub bypassed: VoiceCleanupPreviewSample,
    /// The same range with the cleanup chain applied at `params`.
    pub processed: VoiceCleanupPreviewSample,
}

/// One shared segment shape for both renders below, differing only in
/// `voice_cleanup_enabled` — everything else (trim range, gain, speed) is a plain unprocessed
/// preview, not the clip's own full effect stack, so the A/B comparison isolates just the
/// cleanup chain's own contribution.
fn preview_segment(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    params: VoiceCleanupParams,
    voice_cleanup_enabled: bool,
) -> avbridge::AudioSegment {
    avbridge::AudioSegment {
        source_path: source_path.to_path_buf(),
        source_in_secs,
        source_out_secs,
        timeline_start_secs: 0.0,
        gain_db: 0.0,
        speed_factor: 1.0,
        gain_keyframe_expr: String::new(),
        duck_role: 0,
        voice_cleanup_enabled,
        voice_cleanup_noise_floor_db: params.noise_floor_db,
        voice_cleanup_compressor_threshold_db: params.compressor_threshold_db,
        voice_cleanup_compressor_ratio: params.compressor_ratio,
        voice_cleanup_ceiling_linear: params.ceiling_linear,
    }
}

/// Renders both A/B samples for `source_path`'s `[source_in_secs, source_out_secs)` range
/// (clamped to [`PREVIEW_SAMPLE_MAX_SECS`], measured from `source_in_secs`) into `out_dir`,
/// overwriting any previous preview render there — this is disposable scratch output, never
/// something a project keeps track of. `target_lufs` should be the sequence's own configured
/// export target (`avcore::project::ExportSettings::target_lufs`), so the preview's overall
/// loudness matches what a real export would actually produce; the cleanup chain's own effect
/// still shows up in the measured true peak/loudness range even once both samples land on
/// roughly the same integrated LUFS via the shared mastering pass every mix already applies.
pub fn render_voice_cleanup_preview(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    params: VoiceCleanupParams,
    target_lufs: f32,
    out_dir: &Path,
    cancel: &AtomicBool,
) -> Result<VoiceCleanupPreviewResult, VoiceCleanupPreviewError> {
    std::fs::create_dir_all(out_dir).map_err(VoiceCleanupPreviewError::Io)?;

    let clamped_out_secs =
        source_in_secs + (source_out_secs - source_in_secs).clamp(0.0, PREVIEW_SAMPLE_MAX_SECS);
    let render_duration_secs = clamped_out_secs - source_in_secs;

    let bypassed_path = out_dir.join("voice_cleanup_preview_bypassed.m4a");
    let processed_path = out_dir.join("voice_cleanup_preview_processed.m4a");

    for (voice_cleanup_enabled, path) in [(false, &bypassed_path), (true, &processed_path)] {
        let segment = preview_segment(
            source_path,
            source_in_secs,
            clamped_out_secs,
            params,
            voice_cleanup_enabled,
        );
        avbridge::mix_audio_timeline(&[segment], render_duration_secs, path, target_lufs, cancel)
            .map_err(VoiceCleanupPreviewError::Mix)?;
    }

    let bypassed_metrics =
        loudness::measure_loudness(&bypassed_path).map_err(VoiceCleanupPreviewError::Measure)?;
    let processed_metrics =
        loudness::measure_loudness(&processed_path).map_err(VoiceCleanupPreviewError::Measure)?;

    Ok(VoiceCleanupPreviewResult {
        bypassed: VoiceCleanupPreviewSample {
            path: bypassed_path,
            metrics: bypassed_metrics,
        },
        processed: VoiceCleanupPreviewSample {
            path: processed_path,
            metrics: processed_metrics,
        },
    })
}
