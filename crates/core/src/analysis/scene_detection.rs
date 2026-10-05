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

//! D4 (`spec/architecture/differentiators.md`): detects hard scene cuts (loading screen, death
//! screen, menu transition) from a mean-absolute-luma-difference score between consecutive
//! sampled frames — a hard cut shows up as a large jump, unlike a gradual pan/zoom or in-game
//! motion. Deliberately pure and frame-source-agnostic: the caller (`ui`'s background-thread
//! scene-cut detection, mirroring `motion_tracking`'s own split) owns sampling frames via
//! [`crate::FrameSampler`] and converting them via [`crate::motion_tracking::rgba_to_gray`] —
//! reused rather than reimplemented, same grayscale conversion motion tracking already needed.

use crate::motion_tracking::GrayFrame;

/// One detected hard cut — `at_secs` is the *later* frame's sample time (the cut lands between
/// the previous sample and this one), `score` is the normalized `[0.0, 1.0]` mean absolute luma
/// difference that triggered it, useful for a caller that wants to show cut "strength" or let a
/// user retune the threshold without re-sampling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SceneCut {
    pub at_secs: f64,
    pub score: f32,
}

/// Default minimum normalized mean-absolute-luma-difference to count as a hard cut rather than
/// ordinary in-game motion or a camera pan — picked well above the frame-to-frame noise floor of
/// typical gameplay footage, but low enough to catch a same-brightness scene change (e.g. one
/// dark cave room to another).
pub const DEFAULT_SCENE_CUT_THRESHOLD: f32 = 0.35;

/// Scans `samples` (time-ordered `(timestamp_secs, frame)` pairs — gaps from skipped/undecoded
/// frames are fine, this only ever compares adjacent list entries) for consecutive pairs whose
/// mean absolute per-pixel luma difference, normalized to `[0.0, 1.0]`, is at least `threshold`.
/// A pair with mismatched frame dimensions is skipped (comparing them pixel-by-pixel wouldn't
/// mean anything) rather than counted as a cut.
pub fn detect_scene_cuts(samples: &[(f64, GrayFrame)], threshold: f32) -> Vec<SceneCut> {
    let mut cuts = Vec::new();
    for pair in samples.windows(2) {
        let (_, frame_a) = &pair[0];
        let (at_secs, frame_b) = &pair[1];
        if frame_a.width != frame_b.width
            || frame_a.height != frame_b.height
            || frame_a.data.is_empty()
            || !frame_a.is_complete()
            || !frame_b.is_complete()
        {
            continue;
        }
        let score = mean_abs_luma_diff(frame_a, frame_b);
        if score >= threshold {
            cuts.push(SceneCut {
                at_secs: *at_secs,
                score,
            });
        }
    }
    cuts
}

/// Mean absolute per-pixel difference between two same-sized grayscale frames, normalized by
/// `255` (the max possible per-pixel difference) to a `[0.0, 1.0]` score. Panics if `a`/`b`
/// have different pixel counts — callers must check dimensions first (see
/// [`detect_scene_cuts`]'s own guard).
fn mean_abs_luma_diff(a: &GrayFrame, b: &GrayFrame) -> f32 {
    let sum: u64 = a
        .data
        .iter()
        .zip(&b.data)
        .map(|(&x, &y)| (x as i32 - y as i32).unsigned_abs() as u64)
        .sum();
    (sum as f32 / a.data.len() as f32) / 255.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid_frame(value: u8, width: u32, height: u32) -> GrayFrame {
        GrayFrame {
            width,
            height,
            data: vec![value; (width * height) as usize],
        }
    }

    #[test]
    fn flags_a_hard_cut_between_very_different_frames() {
        let samples = vec![(0.0, solid_frame(0, 4, 4)), (1.0, solid_frame(255, 4, 4))];
        let cuts = detect_scene_cuts(&samples, DEFAULT_SCENE_CUT_THRESHOLD);
        assert_eq!(cuts.len(), 1);
        assert_eq!(cuts[0].at_secs, 1.0);
        assert!((cuts[0].score - 1.0).abs() < 1e-6);
    }

    #[test]
    fn does_not_flag_a_small_difference_below_threshold() {
        let samples = vec![(0.0, solid_frame(100, 4, 4)), (1.0, solid_frame(105, 4, 4))];
        let cuts = detect_scene_cuts(&samples, DEFAULT_SCENE_CUT_THRESHOLD);
        assert!(cuts.is_empty());
    }

    #[test]
    fn flags_every_qualifying_pair_across_a_longer_run() {
        let samples = vec![
            (0.0, solid_frame(0, 2, 2)),
            (1.0, solid_frame(0, 2, 2)),
            (2.0, solid_frame(255, 2, 2)), // cut here
            (3.0, solid_frame(255, 2, 2)),
            (4.0, solid_frame(0, 2, 2)), // and here
        ];
        let cuts = detect_scene_cuts(&samples, DEFAULT_SCENE_CUT_THRESHOLD);
        assert_eq!(
            cuts.iter().map(|c| c.at_secs).collect::<Vec<_>>(),
            vec![2.0, 4.0]
        );
    }

    #[test]
    fn skips_a_pair_with_mismatched_dimensions_instead_of_flagging_it() {
        let samples = vec![(0.0, solid_frame(0, 4, 4)), (1.0, solid_frame(255, 2, 2))];
        let cuts = detect_scene_cuts(&samples, DEFAULT_SCENE_CUT_THRESHOLD);
        assert!(cuts.is_empty());
    }

    #[test]
    fn skips_incomplete_frames_instead_of_comparing_partial_data() {
        let samples = vec![
            (0.0, solid_frame(0, 4, 4)),
            (
                1.0,
                GrayFrame {
                    width: 4,
                    height: 4,
                    data: vec![255; 15],
                },
            ),
        ];
        assert!(detect_scene_cuts(&samples, DEFAULT_SCENE_CUT_THRESHOLD).is_empty());
    }

    #[test]
    fn empty_and_single_sample_inputs_yield_no_cuts() {
        assert!(detect_scene_cuts(&[], DEFAULT_SCENE_CUT_THRESHOLD).is_empty());
        assert!(
            detect_scene_cuts(&[(0.0, solid_frame(0, 4, 4))], DEFAULT_SCENE_CUT_THRESHOLD)
                .is_empty()
        );
    }
}
