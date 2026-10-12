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

//! D1 (`spec/architecture/differentiators.md`): detects silent/dead-air stretches from an
//! asset's already-computed waveform peaks ([`crate::waveform::generate_waveform`]) so they can
//! be listed in a review UI (accept/reject per gap, never a silent auto-apply) before
//! [`crate::timeline::Track::ripple_delete_range`] cuts the accepted ones. Deliberately built on
//! [`crate::waveform`], not [`crate::loudness::measure_loudness`] — the latter is a single-pass
//! whole-file aggregate (one LUFS/TruePeak/LRA report per file), not a time series, so it can't
//! say *where* in a clip a silence falls. `generate_waveform`'s per-bucket `(min, max)` peaks
//! already are windowed amplitude data, so silence detection is a pure scan over data most
//! assets already have cached (`MediaAsset::waveform_peaks`) — no new decode pass.

use crate::timeline::ClipInstance;

/// A detected stretch of near-silence, in timeline-relative seconds — what D1's review UI lists
/// for accept/reject before [`crate::timeline::Track::ripple_delete_range`] applies it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SilenceGap {
    pub start_secs: f64,
    pub end_secs: f64,
}

impl SilenceGap {
    pub fn duration_secs(&self) -> f64 {
        self.end_secs - self.start_secs
    }
}

/// Default below-this-amplitude-counts-as-silent threshold, linear `[0.0, 1.0]` — roughly
/// -34 dBFS, low enough to skip normal room tone/keyboard noise picked up by an open mic, high
/// enough to actually catch a genuine pause or breath.
pub const DEFAULT_SILENCE_THRESHOLD_LINEAR: f32 = 0.02;

/// Default minimum run length to count as a cuttable gap, not a natural breath between words —
/// half a second, short enough to still catch dead air but long enough to not chop mid-sentence
/// pauses that are part of normal speech cadence.
pub const DEFAULT_MIN_SILENCE_SECS: f64 = 0.5;

/// Scans `peaks` (see [`crate::waveform::generate_waveform`], evenly spread across
/// `source_duration_secs`) for runs of consecutive buckets whose peak amplitude never exceeds
/// `threshold_linear`, and reports each run at least `min_gap_secs` long. Coordinates are
/// source-file-relative (the same space `peaks` was computed in), not timeline-relative — a
/// caller mapping this onto a specific timeline clip should use [`clip_silence_gaps`] instead,
/// which also handles the clip's trim range and speed factor.
pub fn detect_silence_gaps(
    peaks: &[(f32, f32)],
    source_duration_secs: f64,
    threshold_linear: f32,
    min_gap_secs: f64,
) -> Vec<SilenceGap> {
    if peaks.is_empty() || source_duration_secs <= 0.0 {
        return Vec::new();
    }
    let bucket_secs = source_duration_secs / peaks.len() as f64;
    let mut gaps = Vec::new();
    let mut run_start: Option<usize> = None;

    let flush = |run_start: &mut Option<usize>, end_idx: usize, gaps: &mut Vec<SilenceGap>| {
        if let Some(start_idx) = run_start.take() {
            let start_secs = start_idx as f64 * bucket_secs;
            let end_secs = end_idx as f64 * bucket_secs;
            if end_secs - start_secs >= min_gap_secs {
                gaps.push(SilenceGap {
                    start_secs,
                    end_secs,
                });
            }
        }
    };

    for (i, &(min, max)) in peaks.iter().enumerate() {
        let amplitude = min.abs().max(max.abs());
        if amplitude <= threshold_linear {
            run_start.get_or_insert(i);
        } else {
            flush(&mut run_start, i, &mut gaps);
        }
    }
    flush(&mut run_start, peaks.len(), &mut gaps);

    gaps
}

/// [`detect_silence_gaps`] over `asset_peaks`/`asset_duration_secs` (a whole source file),
/// intersected with `clip`'s visible `source_in_secs..source_out_secs` range and mapped into
/// timeline-relative seconds for that clip instance — what D1's review UI lists per clip. Empty
/// for a clip with a non-positive `speed_factor`.
pub fn clip_silence_gaps(
    asset_peaks: &[(f32, f32)],
    asset_duration_secs: f64,
    clip: &ClipInstance,
    threshold_linear: f32,
    min_gap_secs: f64,
) -> Vec<SilenceGap> {
    if clip.speed_factor <= 0.0 {
        return Vec::new();
    }

    detect_silence_gaps(
        asset_peaks,
        asset_duration_secs,
        threshold_linear,
        min_gap_secs,
    )
    .into_iter()
    .filter_map(|gap| {
        let clipped_start = gap.start_secs.max(clip.source_in_secs);
        let clipped_end = gap.end_secs.min(clip.source_out_secs);
        if clipped_end - clipped_start < min_gap_secs {
            return None;
        }
        Some(SilenceGap {
            start_secs: clip.source_to_timeline_secs(clipped_start),
            end_secs: clip.source_to_timeline_secs(clipped_end),
        })
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn silent(n: usize) -> Vec<(f32, f32)> {
        vec![(0.0, 0.0); n]
    }

    fn loud(n: usize) -> Vec<(f32, f32)> {
        vec![(-0.8, 0.8); n]
    }

    #[test]
    fn detects_a_single_silent_run_in_the_middle() {
        let mut peaks = loud(10);
        for p in &mut peaks[3..8] {
            *p = (0.0, 0.0);
        }
        // 10 buckets over 10 seconds -> 1 second per bucket; run [3,8) is 5 seconds.
        let gaps = detect_silence_gaps(&peaks, 10.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 3.0,
                end_secs: 8.0
            }]
        );
    }

    #[test]
    fn ignores_runs_shorter_than_the_minimum_duration() {
        let mut peaks = loud(10);
        peaks[4] = (0.0, 0.0);
        // 10 buckets over 1 second -> the single silent bucket is only 0.1 seconds.
        let gaps = detect_silence_gaps(&peaks, 1.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert!(gaps.is_empty());
    }

    #[test]
    fn detects_silence_running_to_the_very_end() {
        let mut peaks = loud(10);
        for p in &mut peaks[6..] {
            *p = (0.0, 0.0);
        }
        let gaps = detect_silence_gaps(&peaks, 10.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 6.0,
                end_secs: 10.0
            }]
        );
    }

    #[test]
    fn empty_peaks_or_zero_duration_yields_no_gaps() {
        assert!(detect_silence_gaps(&[], 10.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5).is_empty());
        assert!(
            detect_silence_gaps(&silent(10), 0.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5).is_empty()
        );
    }

    #[test]
    fn amplitude_exactly_at_threshold_counts_as_silent() {
        let peaks = vec![
            (
                -DEFAULT_SILENCE_THRESHOLD_LINEAR,
                DEFAULT_SILENCE_THRESHOLD_LINEAR
            );
            10
        ];
        let gaps = detect_silence_gaps(&peaks, 10.0, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 0.0,
                end_secs: 10.0
            }]
        );
    }

    fn test_clip(
        start_secs: f64,
        source_in_secs: f64,
        source_out_secs: f64,
        speed_factor: f32,
    ) -> ClipInstance {
        use crate::timeline::{BlendMode, ColorFilter, MaskShape, TransitionType};
        ClipInstance {
            id: 1,
            asset_id: 1,
            start_secs,
            source_in_secs,
            source_out_secs,
            composite_id: None,
            color_label: None,
            gain_db: 0.0,
            frozen: false,
            speed_factor,
            speed_ramp_end_factor: None,
            nested_sequence_id: None,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_w: 1.0,
            crop_h: 1.0,
            mask_shape: MaskShape::None,
            mask_corner_radius: 0.0,
            flipped_h: false,
            color_filter: ColorFilter::None,
            vignette_intensity: 0.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            sharpen: 0.0,
            chroma_key_enabled: false,
            chroma_key_color: [0, 255, 0],
            chroma_key_tolerance: 0.4,
            blur_intensity: 0.0,
            shake_intensity: 0.0,
            glitch_intensity: 0.0,
            pixelize_intensity: 0.0,
            transition_in: TransitionType::None,
            transition_duration_secs: 0.5,
            fade_in_secs: 0.0,
            fade_out_secs: 0.0,
            position_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            opacity_keyframes: vec![],
            gain_keyframes: vec![],
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            brightness_keyframes: vec![],
            contrast_keyframes: vec![],
            saturation_keyframes: vec![],
            crop_x_keyframes: vec![],
            crop_y_keyframes: vec![],
            crop_w_keyframes: vec![],
            crop_h_keyframes: vec![],
            deflicker_enabled: false,
            lut_path: String::new(),
            layer_scale_x: 1.0,
            layer_scale_y: 1.0,
            stabilization_intensity: 0.0,
            background_removal_enabled: false,
            background_removal_mask_path: String::new(),
            blend_mode: BlendMode::Normal,
            anchor_x: 0.5,
            anchor_y: 0.5,
            reframe_seed_point: None,
            privacy_blur_enabled: false,
            privacy_blur_mask_path: String::new(),
            privacy_blur_sigma: 15.0,
            privacy_blur_seed_vertices: Vec::new(),
            privacy_blur_seed_center_x_frac: 0.0,
            privacy_blur_seed_center_y_frac: 0.0,
            grade: Default::default(),
        }
    }

    #[test]
    fn maps_a_gap_fully_inside_the_clips_trim_range_to_timeline_coordinates() {
        // 20 one-second buckets, silent [8, 12) -> source seconds 8..12.
        let mut peaks = loud(20);
        for p in &mut peaks[8..12] {
            *p = (0.0, 0.0);
        }
        // Clip shows source 5..15 starting at timeline 100, unity speed.
        let clip = test_clip(100.0, 5.0, 15.0, 1.0);
        let gaps = clip_silence_gaps(&peaks, 20.0, &clip, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 103.0,
                end_secs: 107.0
            }]
        );
    }

    #[test]
    fn clips_a_gap_that_straddles_the_clips_trim_boundary() {
        let mut peaks = loud(20);
        for p in &mut peaks[8..14] {
            *p = (0.0, 0.0);
        }
        // Clip only shows source up to 12, so the silent run [8,14) is clipped to [8,12).
        let clip = test_clip(100.0, 5.0, 12.0, 1.0);
        let gaps = clip_silence_gaps(&peaks, 20.0, &clip, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 103.0,
                end_secs: 107.0
            }]
        );
    }

    #[test]
    fn scales_timeline_position_by_speed_factor() {
        let mut peaks = loud(20);
        for p in &mut peaks[10..14] {
            *p = (0.0, 0.0);
        }
        // 2x speed: 4 source-seconds of silence maps to 2 timeline-seconds.
        let clip = test_clip(0.0, 0.0, 20.0, 2.0);
        let gaps = clip_silence_gaps(&peaks, 20.0, &clip, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5);
        assert_eq!(
            gaps,
            vec![SilenceGap {
                start_secs: 5.0,
                end_secs: 7.0
            }]
        );
    }

    #[test]
    fn non_positive_speed_factor_yields_no_gaps() {
        let peaks = silent(20);
        let clip = test_clip(0.0, 0.0, 20.0, 0.0);
        assert!(
            clip_silence_gaps(&peaks, 20.0, &clip, DEFAULT_SILENCE_THRESHOLD_LINEAR, 0.5)
                .is_empty()
        );
    }
}
