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

use super::*;

fn sample(time_fraction: f32, subject_center: Option<(f32, f32)>) -> ReframeSample {
    ReframeSample {
        time_fraction,
        subject_center,
    }
}

#[test]
fn fill_reframe_gaps_holds_a_short_gap_at_the_last_known_center() {
    let samples = vec![
        sample(0.0, Some((0.1, 0.1))),
        sample(0.1, None),
        sample(0.2, None),
        sample(0.3, Some((0.5, 0.5))),
    ];
    let filled = fill_reframe_gaps(&samples);
    assert_eq!(
        filled,
        vec![
            Some((0.1, 0.1)),
            Some((0.1, 0.1)),
            Some((0.1, 0.1)),
            Some((0.5, 0.5)),
        ]
    );
}

#[test]
fn fill_reframe_gaps_falls_back_to_none_on_a_long_gap() {
    let samples = vec![
        sample(0.0, Some((0.1, 0.1))),
        sample(0.1, None),
        sample(0.2, None),
        sample(0.3, None),
        sample(0.4, None),
        sample(0.5, Some((0.9, 0.9))),
    ];
    let filled = fill_reframe_gaps(&samples);
    assert_eq!(
        filled,
        vec![Some((0.1, 0.1)), None, None, None, None, Some((0.9, 0.9)),]
    );
}

#[test]
fn fill_reframe_gaps_leaves_a_leading_gap_as_none() {
    // No subject has ever been seen yet -- nothing to hold.
    let samples = vec![
        sample(0.0, None),
        sample(0.1, None),
        sample(0.2, Some((0.5, 0.5))),
    ];
    let filled = fill_reframe_gaps(&samples);
    assert_eq!(filled, vec![None, None, Some((0.5, 0.5))]);
}

#[test]
fn fill_reframe_gaps_is_a_no_op_with_no_gaps() {
    let samples = vec![
        sample(0.0, Some((0.2, 0.2))),
        sample(0.5, Some((0.4, 0.4))),
        sample(1.0, Some((0.6, 0.6))),
    ];
    let filled = fill_reframe_gaps(&samples);
    assert_eq!(
        filled,
        vec![Some((0.2, 0.2)), Some((0.4, 0.4)), Some((0.6, 0.6))]
    );
}

#[test]
fn smooth_subject_centers_averages_within_the_window() {
    let centers = vec![Some((0.0, 0.0)), Some((0.2, 0.2)), Some((0.4, 0.4))];
    let smoothed = smooth_subject_centers(&centers, 3);
    assert_eq!(smoothed[0], Some((0.1, 0.1)));
    assert_eq!(smoothed[1], Some((0.2, 0.2)));
    assert_eq!(smoothed[2], Some((0.3, 0.3)));
}

#[test]
fn smooth_subject_centers_leaves_none_entries_untouched() {
    let centers = vec![Some((0.0, 0.0)), None, Some((1.0, 1.0))];
    let smoothed = smooth_subject_centers(&centers, 3);
    assert_eq!(smoothed[1], None);
    // Neighbors skip the None slot in their own average rather than treating it as zero.
    assert_eq!(smoothed[0], Some((0.0, 0.0)));
    assert_eq!(smoothed[2], Some((1.0, 1.0)));
}

fn crop(x: f32, y: f32, w: f32, h: f32) -> CropRect {
    CropRect { x, y, w, h }
}

#[test]
fn sparse_crop_keyframes_emits_first_and_last_even_when_perfectly_static() {
    let samples = vec![
        (0.0, crop(0.1, 0.1, 0.5, 0.5)),
        (0.5, crop(0.1, 0.1, 0.5, 0.5)),
        (1.0, crop(0.1, 0.1, 0.5, 0.5)),
    ];
    let axes = sparse_crop_keyframes(&samples, DEFAULT_SPARSIFY_EPSILON);
    for axis in [&axes.x, &axes.y, &axes.width, &axes.height] {
        assert_eq!(axis.len(), 2);
        assert_eq!(axis[0].time_fraction, 0.0);
        assert_eq!(axis[1].time_fraction, 1.0);
    }
}

#[test]
fn sparse_crop_keyframes_is_empty_for_no_samples() {
    let axes = sparse_crop_keyframes(&[], DEFAULT_SPARSIFY_EPSILON);
    assert!(
        axes.x.is_empty() && axes.y.is_empty() && axes.width.is_empty() && axes.height.is_empty()
    );
}

#[test]
fn sparse_crop_keyframes_handles_a_single_sample_without_panicking() {
    let samples = vec![(0.5, crop(0.2, 0.2, 0.6, 0.6))];
    let axes = sparse_crop_keyframes(&samples, DEFAULT_SPARSIFY_EPSILON);
    for axis in [&axes.x, &axes.y, &axes.width, &axes.height] {
        assert_eq!(axis.len(), 1);
        assert_eq!(axis[0].time_fraction, 0.5);
    }
}

#[test]
fn sparse_crop_keyframes_emits_a_point_where_a_value_crosses_the_threshold() {
    let samples = vec![
        (0.0, crop(0.1, 0.5, 0.5, 0.5)),
        (0.3, crop(0.1005, 0.5, 0.5, 0.5)), // within epsilon of the first x -- no new point
        (0.6, crop(0.5, 0.5, 0.5, 0.5)),    // crosses epsilon -- new point
        (1.0, crop(0.5, 0.5, 0.5, 0.5)),    // matches the last emitted value -- no new point
    ];
    let xs = sparse_crop_keyframes(&samples, 0.01).x;
    assert_eq!(xs.len(), 3);
    assert_eq!(xs[0].time_fraction, 0.0);
    assert_eq!(xs[0].value, 0.1);
    assert_eq!(xs[1].time_fraction, 0.6);
    assert_eq!(xs[1].value, 0.5);
    assert_eq!(xs[2].time_fraction, 1.0);
    assert_eq!(xs[2].value, 0.5);
}

#[test]
fn sparse_crop_keyframes_sparsifies_each_axis_independently() {
    // x moves a lot at the middle sample; y never moves at all -- y should stay at just its
    // first/last points while x also picks up the middle one.
    let samples = vec![
        (0.0, crop(0.1, 0.3, 0.5, 0.5)),
        (0.5, crop(0.6, 0.3, 0.5, 0.5)),
        (1.0, crop(0.6, 0.3, 0.5, 0.5)),
    ];
    let axes = sparse_crop_keyframes(&samples, 0.01);
    let xs = axes.x;
    let ys = axes.y;
    assert_eq!(xs.len(), 3);
    assert_eq!(xs[1].time_fraction, 0.5);
    assert_eq!(xs[2].time_fraction, 1.0);
    assert_eq!(ys.len(), 2);
    assert_eq!(ys[0].time_fraction, 0.0);
    assert_eq!(ys[1].time_fraction, 1.0);
}

fn face(x: f32, y: f32, w: f32, h: f32, score: f32) -> FaceBox {
    FaceBox { x, y, w, h, score }
}

#[test]
fn select_subject_center_falls_back_to_highest_score_with_no_previous_center() {
    let faces = vec![face(0.0, 0.0, 0.1, 0.1, 0.6), face(0.5, 0.5, 0.1, 0.1, 0.9)];
    let center = select_subject_center(&faces, None, DEFAULT_CONTINUITY_MAX_DISTANCE);
    assert_eq!(center, Some((0.55, 0.55)));
}

#[test]
fn select_subject_center_prefers_continuity_over_a_higher_score_elsewhere() {
    // The previous subject is near (0.1, 0.1). A new, higher-scoring face appears far away --
    // a real second person, or momentary noise -- while a lower-scoring face still sits right
    // where the tracked subject was. Continuity should win: it's the same person, not a re-target.
    let previous_center = Some((0.1, 0.1));
    let faces = vec![
        face(0.05, 0.05, 0.1, 0.1, 0.72), // center (0.1, 0.1) -- continues the previous subject
        face(0.8, 0.8, 0.1, 0.1, 0.95),   // center (0.85, 0.85) -- higher score, but far away
    ];
    let center = select_subject_center(&faces, previous_center, DEFAULT_CONTINUITY_MAX_DISTANCE);
    assert_eq!(center, Some((0.1, 0.1)));
}

#[test]
fn select_subject_center_falls_back_to_highest_score_when_nothing_continues() {
    // The previously tracked subject left frame (or the shot cut) -- no candidate is within
    // range of the previous center, so this must fall back to the plain highest-score rule
    // rather than returning None or clinging to a stale position.
    let previous_center = Some((0.0, 0.0));
    let faces = vec![face(0.8, 0.8, 0.1, 0.1, 0.95)];
    let center = select_subject_center(&faces, previous_center, DEFAULT_CONTINUITY_MAX_DISTANCE);
    assert_eq!(center, Some((0.85, 0.85)));
}

#[test]
fn select_subject_center_is_none_for_no_faces_regardless_of_previous_center() {
    assert_eq!(
        select_subject_center(&[], None, DEFAULT_CONTINUITY_MAX_DISTANCE),
        None
    );
    assert_eq!(
        select_subject_center(&[], Some((0.5, 0.5)), DEFAULT_CONTINUITY_MAX_DISTANCE),
        None
    );
}

#[test]
fn select_subject_center_breaks_a_continuity_tie_by_score_too() {
    // Two candidates both within range of the previous center -- the higher-scoring one still
    // wins among them, continuity narrows the field but doesn't override confidence within it.
    let previous_center = Some((0.5, 0.5));
    let faces = vec![
        face(0.45, 0.45, 0.1, 0.1, 0.7), // center (0.5, 0.5)
        face(0.5, 0.45, 0.1, 0.1, 0.9),  // center (0.55, 0.5) -- also close, higher score
    ];
    let center = select_subject_center(&faces, previous_center, DEFAULT_CONTINUITY_MAX_DISTANCE);
    assert_eq!(center, Some((0.55, 0.5)));
}

fn cut(at_secs: f64) -> SceneCut {
    SceneCut {
        at_secs,
        score: 1.0,
    }
}

#[test]
fn augment_sample_times_for_cuts_is_a_no_op_with_no_cuts() {
    let times = vec![0.0, 1.0, 2.0, 3.0];
    assert_eq!(augment_sample_times_for_cuts(&times, &[], 2), times);
}

#[test]
fn augment_sample_times_for_cuts_is_a_no_op_with_zero_extra_per_cut() {
    let times = vec![0.0, 1.0, 2.0];
    assert_eq!(augment_sample_times_for_cuts(&times, &[cut(1.0)], 0), times);
}

#[test]
fn augment_sample_times_for_cuts_is_a_no_op_with_fewer_than_two_samples() {
    let times = vec![0.0];
    assert_eq!(augment_sample_times_for_cuts(&times, &[cut(0.0)], 2), times);
}

#[test]
fn augment_sample_times_for_cuts_inserts_evenly_spaced_times_in_the_preceding_gap() {
    let times = vec![0.0, 1.0, 2.0, 3.0];
    // Cut lands at 2.0 -- the gap immediately preceding it is [1.0, 2.0].
    let augmented = augment_sample_times_for_cuts(&times, &[cut(2.0)], 2);
    assert_eq!(
        augmented,
        vec![0.0, 1.0, 1.0 + 1.0 / 3.0, 1.0 + 2.0 / 3.0, 2.0, 3.0]
    );
}

#[test]
fn augment_sample_times_for_cuts_ignores_a_cut_at_the_first_sample() {
    // No preceding gap exists for the very first sample -- nothing to thicken.
    let times = vec![0.0, 1.0, 2.0];
    assert_eq!(augment_sample_times_for_cuts(&times, &[cut(0.0)], 2), times);
}

#[test]
fn augment_sample_times_for_cuts_ignores_a_cut_that_matches_no_sample() {
    let times = vec![0.0, 1.0, 2.0];
    assert_eq!(augment_sample_times_for_cuts(&times, &[cut(1.5)], 2), times);
}

#[test]
fn augment_sample_times_for_cuts_handles_multiple_cuts_without_duplicating_times() {
    let times = vec![0.0, 1.0, 2.0, 3.0, 4.0];
    let augmented = augment_sample_times_for_cuts(&times, &[cut(2.0), cut(4.0)], 1);
    assert_eq!(augmented, vec![0.0, 1.0, 1.5, 2.0, 3.0, 3.5, 4.0]);
}

#[test]
fn augment_sample_times_for_cuts_never_produces_out_of_order_or_duplicate_times() {
    let times = vec![0.0, 1.0, 2.0, 3.0];
    let augmented = augment_sample_times_for_cuts(&times, &[cut(1.0), cut(2.0), cut(3.0)], 3);
    for w in augmented.windows(2) {
        assert!(
            w[1] > w[0],
            "times must be strictly ascending: {augmented:?}"
        );
    }
}
