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

fn approx(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

#[test]
fn no_subject_falls_back_to_center_crop() {
    // 1920x1080 -> 9:16: narrower target, full height kept.
    let rect = compute_reframe_crop(1920, 1080, 9, 16, None);
    approx(rect.h, 1.0);
    approx(rect.w, (9.0 / 16.0) / (1920.0 / 1080.0));
    approx(rect.x, (1.0 - rect.w) / 2.0);
    approx(rect.y, 0.0);
}

#[test]
fn wider_target_keeps_full_width() {
    // 1080x1920 source -> 16:9 target: wider than source, full width kept.
    let rect = compute_reframe_crop(1080, 1920, 16, 9, None);
    approx(rect.w, 1.0);
    approx(rect.h, (1080.0 / 1920.0) / (16.0 / 9.0));
}

#[test]
fn subject_off_center_shifts_crop_toward_it() {
    let rect = compute_reframe_crop(1920, 1080, 9, 16, Some((0.2, 0.5)));
    // Crop is centered on x=0.2 but clamped so it doesn't run past the left edge.
    assert!(rect.x < (1.0 - rect.w) / 2.0);
    assert!(rect.x >= 0.0);
}

#[test]
fn subject_near_edge_clamps_without_overflow() {
    let rect = compute_reframe_crop(1920, 1080, 9, 16, Some((0.98, 0.98)));
    assert!(rect.x + rect.w <= 1.0 + 1e-5);
    assert!(rect.y + rect.h <= 1.0 + 1e-5);
    assert!(rect.x >= 0.0);
    assert!(rect.y >= 0.0);
}

#[test]
fn same_aspect_ratio_keeps_full_frame() {
    let rect = compute_reframe_crop(1920, 1080, 16, 9, Some((0.3, 0.7)));
    approx(rect.w, 1.0);
    approx(rect.h, 1.0);
    approx(rect.x, 0.0);
    approx(rect.y, 0.0);
}

#[test]
fn main_subject_picks_highest_score() {
    let faces = vec![
        FaceBox {
            x: 0.0,
            y: 0.0,
            w: 0.1,
            h: 0.1,
            score: 0.75,
        },
        FaceBox {
            x: 0.4,
            y: 0.4,
            w: 0.2,
            h: 0.2,
            score: 0.95,
        },
    ];
    let center = main_subject_center(&faces).unwrap();
    approx(center.0, 0.5);
    approx(center.1, 0.5);
}

#[test]
fn main_subject_none_when_no_faces() {
    assert!(main_subject_center(&[]).is_none());
}

#[test]
fn nms_collapses_overlapping_boxes() {
    let candidates = vec![
        FaceBox {
            x: 0.1,
            y: 0.1,
            w: 0.3,
            h: 0.3,
            score: 0.9,
        },
        FaceBox {
            x: 0.12,
            y: 0.11,
            w: 0.3,
            h: 0.3,
            score: 0.8,
        },
        FaceBox {
            x: 0.7,
            y: 0.7,
            w: 0.2,
            h: 0.2,
            score: 0.85,
        },
    ];
    let kept = non_max_suppress(candidates);
    assert_eq!(kept.len(), 2);
    approx(kept[0].score, 0.9);
    approx(kept[1].score, 0.85);
}

#[test]
fn zero_area_boxes_never_overlap() {
    let empty = FaceBox {
        x: 0.5,
        y: 0.5,
        w: 0.0,
        h: 0.0,
        score: 0.9,
    };
    assert_eq!(empty.iou(empty), 0.0);
}

#[test]
fn preprocessing_normalises_and_resamples_to_the_model_grid() {
    // A 2x2 frame: the model grid samples it nearest-neighbour into 320x240 channel planes.
    let mut rgba = Vec::new();
    for value in [0u8, 127, 255, 64] {
        rgba.extend_from_slice(&[value, value, value, 255]);
    }
    let chw = preprocess(&rgba, 2, 2);
    let plane = (MODEL_INPUT_WIDTH * MODEL_INPUT_HEIGHT) as usize;
    assert_eq!(chw.len(), 3 * plane);
    // Top-left of the grid maps to source pixel (0, 0) = 0, so (0 - 127) / 128.
    assert!((chw[0] - (-127.0 / 128.0)).abs() < 1e-6);
    // The last column of the first row maps to source pixel (1, 0) = 127 -> 0.
    assert_eq!(chw[(MODEL_INPUT_WIDTH - 1) as usize], 0.0);
    // The bottom-right of every channel plane is source pixel (1, 1) = 64.
    let last = plane - 1;
    for channel in 0..3 {
        assert!((chw[channel * plane + last] - (64.0 - 127.0) / 128.0).abs() < 1e-6);
    }
}

#[test]
fn the_two_model_outputs_are_told_apart_by_their_last_dimension() {
    let scores = (vec![1, 3, 2], vec![0.0; 6]);
    let boxes = (vec![1, 3, 4], vec![0.0; 12]);
    let (anchors, s, b) = select_outputs(vec![boxes.clone(), scores.clone()]).unwrap();
    assert_eq!((anchors, s.len(), b.len()), (3, 6, 12));
    // Tensors of any other shape are ignored, and both outputs must be present.
    let other = (vec![1, 5], vec![0.0; 5]);
    assert!(select_outputs(vec![other.clone(), boxes]).is_err());
    assert!(select_outputs(vec![other, scores]).is_err());
    let error = select_outputs(Vec::new()).unwrap_err();
    assert!(error.to_string().contains("no scores output"));
}

#[test]
fn only_confident_non_degenerate_non_overlapping_faces_are_decoded() {
    // Four anchors: a face, a weak face, a degenerate box and a heavy overlap of the first.
    let scores = [
        0.0, 0.95, //
        0.0, 0.2, //
        0.0, 0.9, //
        0.0, 0.8,
    ];
    let boxes = [
        0.1, 0.1, 0.5, 0.5, //
        0.1, 0.1, 0.5, 0.5, //
        0.6, 0.6, 0.6, 0.9, //
        0.12, 0.12, 0.5, 0.5,
    ];
    let faces = decode_faces(4, &scores, &boxes);
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].score, 0.95);
    assert!((faces[0].w - 0.4).abs() < 1e-6);

    // Corners outside the frame are clamped to it.
    let clamped = decode_faces(1, &[0.0, 0.9], &[-0.5, -0.5, 1.5, 1.5]);
    assert_eq!(
        (clamped[0].x, clamped[0].y, clamped[0].w, clamped[0].h),
        (0.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn errors_are_readable_and_a_missing_model_fails_to_load() {
    assert!(ReframeError::Onnx("bad".into()).to_string().contains("bad"));
    let io = ReframeError::Io(std::io::Error::other("disk"));
    assert!(io.to_string().contains("disk"));
    let result = detect_faces(std::path::Path::new("missing-model.onnx"), &[0; 16], 2, 2);
    assert!(matches!(result, Err(ReframeError::Onnx(_))));
}

/// Output tensors shaped like the real model's: `anchors` rows of (background, face) scores and
/// of (xmin, ymin, xmax, ymax) boxes.
fn model_outputs(faces: &[(f32, [f32; 4])], anchors: usize) -> Vec<(Vec<i64>, Vec<f32>)> {
    let mut scores = vec![0.0; anchors * 2];
    let mut boxes = vec![0.0; anchors * 4];
    for (i, (score, bbox)) in faces.iter().enumerate() {
        scores[i * 2 + 1] = *score;
        boxes[i * 4..i * 4 + 4].copy_from_slice(bbox);
    }
    vec![
        (vec![1, anchors as i64, 4], boxes),
        (vec![1, anchors as i64, 2], scores),
    ]
}

#[test]
fn faces_are_decoded_from_whatever_order_the_model_lists_its_outputs() {
    let rgba = vec![128u8; 8 * 8 * 4];
    let faces = detect_faces_with(&rgba, 8, 8, |input| {
        assert_eq!(
            input.len(),
            3 * (MODEL_INPUT_WIDTH * MODEL_INPUT_HEIGHT) as usize
        );
        Ok(model_outputs(
            &[(0.9, [0.1, 0.2, 0.3, 0.6]), (0.2, [0.5, 0.5, 0.9, 0.9])],
            4,
        ))
    })
    .unwrap();
    assert_eq!(faces.len(), 1, "the low-confidence box is dropped");
    approx(faces[0].x, 0.1);
    approx(faces[0].w, 0.2);
}

#[test]
fn overlapping_detections_are_merged_and_degenerate_boxes_ignored() {
    let rgba = vec![0u8; 4 * 4 * 4];
    let faces = detect_faces_with(&rgba, 4, 4, |_| {
        Ok(model_outputs(
            &[
                (0.95, [0.1, 0.1, 0.5, 0.5]),
                (0.90, [0.12, 0.12, 0.5, 0.5]),
                (0.80, [0.7, 0.7, 0.7, 0.9]),
            ],
            4,
        ))
    })
    .unwrap();
    assert_eq!(faces.len(), 1);
}

#[test]
fn a_model_without_the_expected_outputs_is_an_error() {
    let rgba = vec![0u8; 4 * 4 * 4];
    let only_boxes = detect_faces_with(&rgba, 4, 4, |_| Ok(vec![(vec![1, 2, 4], vec![0.0; 8])]));
    assert!(matches!(only_boxes, Err(ReframeError::Onnx(m)) if m.contains("scores")));
    let only_scores = detect_faces_with(&rgba, 4, 4, |_| Ok(vec![(vec![1, 2, 2], vec![0.0; 4])]));
    assert!(matches!(only_scores, Err(ReframeError::Onnx(m)) if m.contains("boxes")));
    let failing = detect_faces_with(&rgba, 4, 4, |_| Err(ReframeError::Onnx("boom".into())));
    assert!(matches!(failing, Err(ReframeError::Onnx(m)) if m == "boom"));
}

#[test]
fn a_missing_model_file_is_an_onnx_error() {
    let result = detect_faces(
        Path::new("definitely-not-a-model.onnx"),
        &[0u8; 4 * 4 * 4],
        4,
        4,
    );
    assert!(matches!(result, Err(ReframeError::Onnx(_))));
}
