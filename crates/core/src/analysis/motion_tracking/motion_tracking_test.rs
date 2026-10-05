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

const FRAME_W: u32 = 80;
const FRAME_H: u32 = 60;
const BLOCK_SIZE: i32 = 12;

/// A frame filled with `background`, with a distinctively textured (not uniform — a uniform
/// block would tie-match against any equally uniform region) `BLOCK_SIZE`-square block whose
/// top-left corner is at `(x0, y0)`.
fn frame_with_block(x0: i32, y0: i32, background: u8) -> GrayFrame {
    let mut data = vec![background; (FRAME_W * FRAME_H) as usize];
    for by in 0..BLOCK_SIZE {
        for bx in 0..BLOCK_SIZE {
            let x = x0 + bx;
            let y = y0 + by;
            if x < 0 || y < 0 || x >= FRAME_W as i32 || y >= FRAME_H as i32 {
                continue;
            }
            let value = ((bx * 7 + by * 13) % 256) as u8;
            data[(y as u32 * FRAME_W + x as u32) as usize] = value;
        }
    }
    GrayFrame {
        width: FRAME_W,
        height: FRAME_H,
        data,
    }
}

#[test]
fn tracks_a_block_moving_in_a_straight_line() {
    // Block center starts at (20, 20) and moves by (+2, +1) px per frame, for 5 frames.
    let start = (20 - BLOCK_SIZE / 2, 20 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = (0..5)
        .map(|i| frame_with_block(start.0 + i * 2, start.1 + i, 0))
        .collect();

    let tracked = track_region(
        &frames,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );

    assert_eq!(tracked.len(), 5);
    for (i, t) in tracked.iter().enumerate() {
        let expected_cx = 20.0 + (i as f32) * 2.0;
        let expected_cy = 20.0 + (i as f32) * 1.0;
        let got_cx = t.center_x_frac * FRAME_W as f32;
        let got_cy = t.center_y_frac * FRAME_H as f32;
        assert!(
            (got_cx - expected_cx).abs() <= 1.0,
            "frame {i}: expected cx {expected_cx}, got {got_cx}"
        );
        assert!(
            (got_cy - expected_cy).abs() <= 1.0,
            "frame {i}: expected cy {expected_cy}, got {got_cy}"
        );
    }
}

#[test]
fn tracks_a_block_starting_from_an_off_center_region() {
    // Regression coverage for the UI region picker: every other tracking test starts near the
    // frame's middle, which would silently pass even if a caller always hard-coded (0.5, 0.5)
    // instead of threading through the user's picked center. Starts near the top-left corner
    // instead and moves away from it, so a center-only bug would produce a very different
    // (0.5, 0.5)-centered track and fail this assertion.
    let start = (8 - BLOCK_SIZE / 2, 8 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = (0..4)
        .map(|i| frame_with_block(start.0 + i * 3, start.1 + i * 2, 200))
        .collect();

    let tracked = track_region(
        &frames,
        8.0 / FRAME_W as f32,
        8.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.15,
    );

    assert_eq!(tracked.len(), 4);
    for (i, t) in tracked.iter().enumerate() {
        let expected_cx = 8.0 + (i as f32) * 3.0;
        let expected_cy = 8.0 + (i as f32) * 2.0;
        let got_cx = t.center_x_frac * FRAME_W as f32;
        let got_cy = t.center_y_frac * FRAME_H as f32;
        assert!(
            (got_cx - expected_cx).abs() <= 1.0,
            "frame {i}: expected cx {expected_cx}, got {got_cx}"
        );
        assert!(
            (got_cy - expected_cy).abs() <= 1.0,
            "frame {i}: expected cy {expected_cy}, got {got_cy}"
        );
    }
}

#[test]
fn tracks_a_non_square_block_with_independent_width_and_height() {
    // width_frac=0.4 (24px) height_frac=0.3 (18px) on a 60px short side — both comfortably
    // larger than BLOCK_SIZE (12px) so the whole textured block stays inside the template
    // regardless of its rectangular (non-square) shape.
    let start = (20 - BLOCK_SIZE / 2, 20 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = (0..5)
        .map(|i| frame_with_block(start.0 + i * 2, start.1 + i, 0))
        .collect();

    let tracked = track_region(
        &frames,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        0.4,
        0.3,
        0.1,
    );

    assert_eq!(tracked.len(), 5);
    for (i, t) in tracked.iter().enumerate() {
        let expected_cx = 20.0 + (i as f32) * 2.0;
        let expected_cy = 20.0 + (i as f32) * 1.0;
        let got_cx = t.center_x_frac * FRAME_W as f32;
        let got_cy = t.center_y_frac * FRAME_H as f32;
        assert!(
            (got_cx - expected_cx).abs() <= 1.0,
            "frame {i}: expected cx {expected_cx}, got {got_cx}"
        );
        assert!(
            (got_cy - expected_cy).abs() <= 1.0,
            "frame {i}: expected cy {expected_cy}, got {got_cy}"
        );
    }
}

#[test]
fn template_width_and_height_clamp_independently_to_each_frame_dimension() {
    // Requests a template wider than the frame's short side (1.5 * 60 = 90px, clamped to the
    // frame's own 80px width) but a modest height (0.2 * 60 = 12px, well under the 60px
    // height) — regression coverage for clamping width to the frame's own width and height to
    // the frame's own height independently, not both to min(width, height) the way the old
    // single-scalar template_size_frac did.
    let frame = frame_with_block(30, 20, 0);

    let tracked = track_region(&[frame], 0.0, 0.5, 1.5, 0.2, 0.1);

    assert_eq!(tracked.len(), 1);
    // Template width clamped to the full 80px frame width leaves no horizontal room to move —
    // the center lands dead in the middle (0.5), not wherever a (buggy) 60px clamp would have
    // allowed.
    assert!((tracked[0].center_x_frac - 0.5).abs() < 1e-4);
    // Height wasn't anywhere near its own clamp limit, so the requested center is untouched.
    assert!((tracked[0].center_y_frac - 0.5).abs() < 1e-4);
}

#[test]
fn stays_put_when_the_block_does_not_move() {
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();

    let tracked = track_region(
        &frames,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );

    for t in &tracked {
        assert!((t.center_x_frac - tracked[0].center_x_frac).abs() < 1e-6);
        assert!((t.center_y_frac - tracked[0].center_y_frac).abs() < 1e-6);
    }
}

#[test]
fn empty_frames_produce_no_tracked_positions() {
    assert!(track_region(&[], 0.5, 0.5, 0.2, 0.2, 0.1).is_empty());
}

#[test]
fn rgba_to_gray_rejects_incomplete_packed_frames() {
    let frame = rgba_to_gray(&[255; 15], 2, 2);
    assert_eq!(frame.width, 2);
    assert_eq!(frame.height, 2);
    assert!(frame.data.is_empty());
    assert!(!frame.is_complete());
}

#[test]
fn tracking_rejects_undersized_or_incomplete_frames() {
    let undersized = GrayFrame {
        width: 3,
        height: 3,
        data: vec![0; 9],
    };
    assert!(track_region(&[undersized], 0.5, 0.5, 0.2, 0.2, 0.1).is_empty());

    let incomplete = GrayFrame {
        width: FRAME_W,
        height: FRAME_H,
        data: vec![0; (FRAME_W * FRAME_H - 1) as usize],
    };
    let result = track_region_with_scores(&[incomplete], 0.5, 0.5, 0.2, 0.2, 0.1);
    assert!(result.positions.is_empty());
    assert!(result.scores.is_empty());
    assert_eq!((result.template_width, result.template_height), (0, 0));
}

#[test]
fn tracking_rejects_a_sequence_with_mismatched_frame_dimensions() {
    let frames = vec![
        frame_with_block(30, 20, 0),
        GrayFrame {
            width: FRAME_W - 1,
            height: FRAME_H,
            data: vec![0; ((FRAME_W - 1) * FRAME_H) as usize],
        },
    ];
    assert!(track_region(&frames, 0.5, 0.5, 0.2, 0.2, 0.1).is_empty());
}

#[test]
fn a_single_frame_produces_one_clamped_position() {
    let frame = frame_with_block(30, 20, 0);
    let tracked = track_region(&[frame], 0.5, 0.5, 0.2, 0.2, 0.1);
    assert_eq!(tracked.len(), 1);
}

#[test]
fn initial_center_near_the_edge_is_clamped_so_the_template_fits() {
    let frames = vec![frame_with_block(0, 0, 0)];
    // Requesting a center right at the corner (0,0) — the template must still fit inside the
    // frame, so the actual tracked center should be pulled inward, not sit at the edge.
    let tracked = track_region(&frames, 0.0, 0.0, 0.2, 0.2, 0.1);
    assert_eq!(tracked.len(), 1);
    assert!(tracked[0].center_x_frac > 0.0);
    assert!(tracked[0].center_y_frac > 0.0);
}

#[test]
fn keyframes_ride_the_delta_from_the_first_tracked_frame() {
    let tracked = vec![
        TrackedPosition {
            center_x_frac: 0.3,
            center_y_frac: 0.3,
        },
        TrackedPosition {
            center_x_frac: 0.4,
            center_y_frac: 0.25,
        },
    ];
    let times = [0.0, 1.0];
    let base = Position { x: 0.1, y: 0.2 };

    let keyframes = tracked_positions_to_keyframes(&tracked, &times, base);

    assert_eq!(keyframes.len(), 2);
    assert_eq!(keyframes[0].time_fraction, 0.0);
    assert!((keyframes[0].value.x - 0.1).abs() < 1e-6);
    assert!((keyframes[0].value.y - 0.2).abs() < 1e-6);
    assert_eq!(keyframes[1].time_fraction, 1.0);
    // Delta from frame 0 to frame 1 was (+0.1, -0.05); applied on top of base.
    assert!((keyframes[1].value.x - 0.2).abs() < 1e-5);
    assert!((keyframes[1].value.y - 0.15).abs() < 1e-5);
}

#[test]
fn keyframes_are_empty_when_no_positions_were_tracked() {
    assert!(tracked_positions_to_keyframes(&[], &[], Position { x: 0.0, y: 0.0 }).is_empty());
}

#[test]
fn track_region_with_scores_positions_match_plain_track_region() {
    let start = (20 - BLOCK_SIZE / 2, 20 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = (0..4)
        .map(|i| frame_with_block(start.0 + i * 2, start.1 + i, 0))
        .collect();

    let plain = track_region(
        &frames,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );
    let with_scores = track_region_with_scores(
        &frames,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );

    assert_eq!(plain.len(), with_scores.positions.len());
    for (a, b) in plain.iter().zip(&with_scores.positions) {
        assert_eq!(a.center_x_frac, b.center_x_frac);
        assert_eq!(a.center_y_frac, b.center_y_frac);
    }
}

#[test]
fn track_region_with_scores_first_frame_score_is_zero() {
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();
    let result = track_region_with_scores(
        &frames,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );
    assert_eq!(result.scores[0], 0);
}

#[test]
fn track_region_with_scores_reports_a_low_score_for_a_perfect_match() {
    // The block never moves, so every subsequent frame should re-find a near-perfect match
    // against the frame-0 template (score 0 or very close to it).
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();
    let result = track_region_with_scores(
        &frames,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        0.2,
        0.2,
        0.1,
    );
    for &score in &result.scores {
        assert_eq!(score, 0);
    }
}

#[test]
fn track_region_with_scores_reports_template_dimensions() {
    let frames = vec![frame_with_block(30, 20, 0)];
    let result = track_region_with_scores(&frames, 0.5, 0.5, 0.2, 0.2, 0.1);
    // 0.2 * short_side(60) = 12px on each axis.
    assert_eq!(result.template_width, 12);
    assert_eq!(result.template_height, 12);
}

#[test]
fn track_region_with_scores_on_empty_frames_is_empty_with_zeroed_dimensions() {
    let result = track_region_with_scores(&[], 0.5, 0.5, 0.2, 0.2, 0.1);
    assert!(result.positions.is_empty());
    assert!(result.scores.is_empty());
    assert_eq!(result.template_width, 0);
    assert_eq!(result.template_height, 0);
}
