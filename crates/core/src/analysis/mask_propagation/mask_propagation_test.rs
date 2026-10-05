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

/// Same synthetic-frame convention `motion_tracking_test.rs` uses (a distinctively textured
/// block on a uniform background) — duplicated locally rather than shared across the two test
/// modules, which have no existing plumbing for that.
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
fn match_confidence_is_one_for_a_perfect_match() {
    assert_eq!(match_confidence(0, 10, 10), 1.0);
}

#[test]
fn match_confidence_is_zero_for_the_worst_possible_match() {
    let worst = 10i64 * 10 * 255;
    assert_eq!(match_confidence(worst, 10, 10), 0.0);
}

#[test]
fn match_confidence_clamps_to_zero_for_a_score_beyond_the_theoretical_max() {
    // Shouldn't happen in practice (SAD is bounded by the template size), but the function must
    // still not panic or return a negative confidence.
    assert_eq!(match_confidence(10i64 * 10 * 255 * 2, 10, 10), 0.0);
}

#[test]
fn match_confidence_is_zero_for_a_non_positive_template_area() {
    assert_eq!(match_confidence(0, 0, 10), 0.0);
    assert_eq!(match_confidence(0, 10, 0), 0.0);
}

#[test]
fn match_confidence_is_between_zero_and_one_for_a_partial_mismatch() {
    let half = (10i64 * 10 * 255) / 2;
    let c = match_confidence(half, 10, 10);
    assert!((0.4..=0.6).contains(&c), "expected roughly 0.5, got {c}");
}

#[test]
fn propagate_mask_by_translation_moves_every_vertex_by_the_tracked_delta() {
    let start = (20 - BLOCK_SIZE / 2, 20 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = (0..4)
        .map(|i| frame_with_block(start.0 + i * 4, start.1, 0))
        .collect();
    let seed_vertices = vec![(0.2, 0.2), (0.3, 0.2), (0.3, 0.3), (0.2, 0.3)];

    let propagated = propagate_mask_by_translation(
        &seed_vertices,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.15,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );

    assert_eq!(propagated.len(), 4);
    // Frame 0 is the seed itself -- vertices unchanged, perfect confidence.
    assert_eq!(propagated[0].vertices, seed_vertices);
    assert_eq!(propagated[0].confidence, 1.0);
    assert!(!propagated[0].needs_correction);

    // The block moved +4px/frame in x, 0 in y -- every later frame's vertices should be shifted
    // by the same delta (as a fraction of frame width) in x, unchanged in y.
    let expected_dx = 4.0 / FRAME_W as f32;
    for (i, mask) in propagated.iter().enumerate().skip(1) {
        for (j, &(vx, vy)) in mask.vertices.iter().enumerate() {
            let expected_x = seed_vertices[j].0 + expected_dx * i as f32;
            assert!(
                (vx - expected_x).abs() < 0.01,
                "frame {i} vertex {j}: expected x {expected_x}, got {vx}"
            );
            assert!((vy - seed_vertices[j].1).abs() < 0.01);
        }
    }
}

#[test]
fn propagate_mask_by_translation_is_empty_for_empty_frames() {
    let seed_vertices = vec![(0.2, 0.2)];
    let propagated = propagate_mask_by_translation(
        &seed_vertices,
        0.5,
        0.5,
        &[],
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );
    assert!(propagated.is_empty());
}

#[test]
fn propagate_mask_by_translation_flags_low_confidence_when_the_block_disappears() {
    // Frame 0 has the textured block; frame 1 is uniform background with no matching content
    // anywhere -- the tracker's best match should be poor, flagging needs_correction.
    let frames = vec![
        frame_with_block(30, 20, 0),
        GrayFrame {
            width: FRAME_W,
            height: FRAME_H,
            // 255 maximizes this template's own L1 distance (computed for real, not guessed —
            // this exact texture's SAD against a uniform background is worst at the extremes)
            // to a uniform field, guaranteeing a low-confidence match below the default 0.5
            // threshold regardless of where the search lands (a uniform frame's score doesn't
            // depend on position at all).
            data: vec![255u8; (FRAME_W * FRAME_H) as usize],
        },
    ];
    let seed_vertices = vec![(0.4, 0.4)];

    let propagated = propagate_mask_by_translation(
        &seed_vertices,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );

    assert_eq!(propagated.len(), 2);
    assert!(!propagated[0].needs_correction);
    assert!(
        propagated[1].needs_correction,
        "expected a low-confidence flag once the tracked content vanished, got confidence {}",
        propagated[1].confidence
    );
}

#[test]
fn propagate_mask_with_corrections_restarts_tracking_from_the_correction() {
    // First segment (frames 0-1): block moves +4px/frame in x.
    // At frame 2, a correction re-seeds the mask at a manually-picked position/vertices.
    // Second segment (frames 2-3): block moves +4px/frame in y instead, matching the
    // correction's own new center so tracking picks it up cleanly.
    let seg1_start = (20 - BLOCK_SIZE / 2, 20 - BLOCK_SIZE / 2);
    let seg2_start = (40 - BLOCK_SIZE / 2, 10 - BLOCK_SIZE / 2);
    let frames: Vec<GrayFrame> = vec![
        frame_with_block(seg1_start.0, seg1_start.1, 0),
        frame_with_block(seg1_start.0 + 4, seg1_start.1, 0),
        frame_with_block(seg2_start.0, seg2_start.1, 0),
        frame_with_block(seg2_start.0, seg2_start.1 + 4, 0),
    ];
    let seed_vertices = vec![(0.1, 0.1)];
    let corrected_vertices = vec![(0.5, 0.15)];

    let corrections = vec![MaskCorrection {
        frame_index: 2,
        vertices: corrected_vertices.clone(),
        center_x_frac: 40.0 / FRAME_W as f32,
        center_y_frac: 10.0 / FRAME_H as f32,
    }];

    let propagated = propagate_mask_with_corrections(
        &seed_vertices,
        20.0 / FRAME_W as f32,
        20.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.15,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
        &corrections,
    );

    assert_eq!(propagated.len(), 4);
    // First segment unaffected by the correction.
    assert_eq!(propagated[0].vertices, seed_vertices);
    // Second segment starts fresh from the correction's own vertices, not a delta applied on
    // top of the first segment's drift.
    assert_eq!(propagated[2].vertices, corrected_vertices);
    // Third segment frame moved +4px in y from the correction's own center.
    let expected_dy = 4.0 / FRAME_H as f32;
    assert!((propagated[3].vertices[0].0 - corrected_vertices[0].0).abs() < 0.01);
    assert!((propagated[3].vertices[0].1 - (corrected_vertices[0].1 + expected_dy)).abs() < 0.01);
}

#[test]
fn propagate_mask_with_corrections_ignores_an_out_of_order_correction() {
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();
    let seed_vertices = vec![(0.1, 0.1)];
    // frame_index 0 is at-or-before the segment start (0) -- must be ignored, not panic.
    let corrections = vec![MaskCorrection {
        frame_index: 0,
        vertices: vec![(0.9, 0.9)],
        center_x_frac: 0.9,
        center_y_frac: 0.9,
    }];

    let propagated = propagate_mask_with_corrections(
        &seed_vertices,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
        &corrections,
    );

    assert_eq!(propagated.len(), 3);
    assert_eq!(propagated[0].vertices, seed_vertices);
}

#[test]
fn propagate_mask_with_corrections_ignores_an_out_of_range_correction() {
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();
    let seed_vertices = vec![(0.1, 0.1)];
    let corrections = vec![MaskCorrection {
        frame_index: 99,
        vertices: vec![(0.9, 0.9)],
        center_x_frac: 0.9,
        center_y_frac: 0.9,
    }];

    let propagated = propagate_mask_with_corrections(
        &seed_vertices,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
        &corrections,
    );

    assert_eq!(propagated.len(), 3);
}

#[test]
fn propagate_mask_with_corrections_with_no_corrections_matches_plain_translation() {
    let frames: Vec<GrayFrame> = (0..3).map(|_| frame_with_block(30, 20, 0)).collect();
    let seed_vertices = vec![(0.1, 0.1), (0.2, 0.1)];

    let plain = propagate_mask_by_translation(
        &seed_vertices,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );
    let with_corrections = propagate_mask_with_corrections(
        &seed_vertices,
        36.0 / FRAME_W as f32,
        26.0 / FRAME_H as f32,
        &frames,
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
        &[],
    );

    assert_eq!(plain, with_corrections);
}

#[test]
fn propagate_mask_with_corrections_is_empty_for_empty_frames() {
    let propagated = propagate_mask_with_corrections(
        &[(0.1, 0.1)],
        0.5,
        0.5,
        &[],
        0.2,
        0.2,
        0.1,
        DEFAULT_LOW_CONFIDENCE_THRESHOLD,
        &[],
    );
    assert!(propagated.is_empty());
}

fn mask(vertices: Vec<(f32, f32)>) -> PropagatedMask {
    PropagatedMask {
        vertices,
        confidence: 1.0,
        needs_correction: false,
    }
}

#[test]
fn rasterize_mask_to_matte_marks_pixels_inside_the_polygon() {
    // A square spanning the middle half of a 10x10 canvas: pixels (2..8, 2..8) inside, corners
    // outside.
    let m = mask(vec![(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)]);
    let buf = rasterize_mask_to_matte(&m, 10, 10);

    assert_eq!(buf.len(), 100);
    assert_eq!(buf[5 * 10 + 5], 255, "center pixel should be inside");
    assert_eq!(buf[0], 0, "top-left corner should be outside");
    assert_eq!(buf[9 * 10 + 9], 0, "bottom-right corner should be outside");
}

#[test]
fn rasterize_mask_to_matte_is_all_zero_for_a_degenerate_polygon() {
    let m = mask(vec![(0.5, 0.5), (0.6, 0.6)]); // only 2 vertices
    let buf = rasterize_mask_to_matte(&m, 10, 10);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn rasterize_mask_to_matte_is_empty_for_zero_sized_canvas() {
    let m = mask(vec![(0.2, 0.2), (0.8, 0.2), (0.8, 0.8)]);
    assert!(rasterize_mask_to_matte(&m, 0, 10).is_empty());
    assert!(rasterize_mask_to_matte(&m, 10, 0).is_empty());
}

#[test]
fn rasterize_to_matte_frames_produces_one_buffer_per_mask() {
    let masks = vec![
        mask(vec![(0.2, 0.2), (0.8, 0.2), (0.8, 0.8), (0.2, 0.8)]),
        mask(vec![(0.0, 0.0), (0.1, 0.0), (0.1, 0.1)]),
    ];
    let frames = rasterize_to_matte_frames(&masks, 10, 10);
    assert_eq!(frames.len(), 2);
    for f in &frames {
        assert_eq!(f.len(), 100);
    }
}

#[test]
fn rasterize_to_matte_frames_is_empty_for_no_masks() {
    assert!(rasterize_to_matte_frames(&[], 10, 10).is_empty());
}
