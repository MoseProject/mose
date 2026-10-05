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

//! Auto-reframe — per `request.md`'s Fase 4 "Reenquadramento automático": when the export
//! aspect ratio changes, recenter the main subject in the new frame automatically, with manual
//! adjustment on top. Reuses [`crate::timeline::ClipInstance`]'s existing static
//! `crop_x`/`crop_y`/`crop_w`/`crop_h` rect — this isn't an animated reframe, just an
//! AI-assisted way to *set* that rect instead of dragging it by hand; "manual adjustment on
//! top" is just editing those same fields afterward, no new UI concept needed.
//!
//! Two independent halves:
//! - [`compute_reframe_crop`] is pure geometry (fits the target aspect ratio inside the source
//!   frame, centered on a detected subject point, clamped to stay in-bounds) — fully unit
//!   tested, no ONNX involved.
//! - [`detect_main_subject`] runs a small ONNX face-detection model (`ort`) against one decoded
//!   frame to find that subject point. Detection is best-effort: a video with no face in frame
//!   (e.g. gameplay footage) just falls back to a centered crop, same as dragging the rect
//!   yourself and leaving it centered.
//!
//! **Model:** UltraFace `version-RFB-320` (`Linzaer/Ultra-Light-Fast-Generic-Face-Detector-1MB`,
//! MIT), shipped in the release bundle under `resources/models/`; the application never
//! downloads it at runtime. Input/output tensor shapes and the
//! `(pixel - 127) / 128` normalization come from the model's own published spec, not guessed.
//! **Unverified:** no machine this has been developed on has actually run `ort` against this
//! model file end-to-end (no ONNX Runtime binary confirmed working here) — same caveat class as
//! this codebase's GPU encoder support, see `CLAUDE.md`.

use std::path::Path;

/// Target size (`320x240`) UltraFace's `version-RFB-320` model expects, and the per-channel
/// normalization it was trained with — confirmed against the model's own published preprocessing
/// spec (not guessed): RGB, resized to exactly this, `(pixel - 127) / 128`, NCHW.
const MODEL_INPUT_WIDTH: u32 = 320;
const MODEL_INPUT_HEIGHT: u32 = 240;

/// Detections below this confidence are discarded before picking the "main" subject.
const SCORE_THRESHOLD: f32 = 0.7;
/// IoU above this collapses two boxes into one during non-max suppression.
const NMS_IOU_THRESHOLD: f32 = 0.4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FaceBox {
    /// Top-left corner and size, all as fractions (`0.0..=1.0`) of the source frame.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
}

impl FaceBox {
    fn center(self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    fn area(self) -> f32 {
        self.w * self.h
    }

    fn iou(self, other: FaceBox) -> f32 {
        let ix0 = self.x.max(other.x);
        let iy0 = self.y.max(other.y);
        let ix1 = (self.x + self.w).min(other.x + other.w);
        let iy1 = (self.y + self.h).min(other.y + other.h);
        let iw = (ix1 - ix0).max(0.0);
        let ih = (iy1 - iy0).max(0.0);
        let inter = iw * ih;
        let union = self.area() + other.area() - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }
}

#[derive(Debug)]
pub enum ReframeError {
    Onnx(String),
    Io(std::io::Error),
}

impl std::fmt::Display for ReframeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReframeError::Onnx(e) => write!(f, "ONNX inference failed: {e}"),
            ReframeError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

impl std::error::Error for ReframeError {}

/// Nearest-neighbor resize of an RGBA buffer to exactly `MODEL_INPUT_WIDTH x
/// MODEL_INPUT_HEIGHT`, dropping alpha and writing straight into UltraFace's expected NCHW,
/// `(pixel - 127) / 128`-normalized float layout. Distortion from ignoring aspect ratio is
/// accepted — the reference implementation resizes the same way, not letterboxed.
fn preprocess(rgba: &[u8], width: u32, height: u32) -> Vec<f32> {
    let mut chw = vec![0f32; 3 * (MODEL_INPUT_HEIGHT * MODEL_INPUT_WIDTH) as usize];
    let plane = (MODEL_INPUT_WIDTH * MODEL_INPUT_HEIGHT) as usize;
    for y in 0..MODEL_INPUT_HEIGHT {
        let src_y = (y * height / MODEL_INPUT_HEIGHT).min(height - 1);
        for x in 0..MODEL_INPUT_WIDTH {
            let src_x = (x * width / MODEL_INPUT_WIDTH).min(width - 1);
            let src = ((src_y * width + src_x) * 4) as usize;
            let dst = (y * MODEL_INPUT_WIDTH + x) as usize;
            chw[dst] = (rgba[src] as f32 - 127.0) / 128.0;
            chw[plane + dst] = (rgba[src + 1] as f32 - 127.0) / 128.0;
            chw[2 * plane + dst] = (rgba[src + 2] as f32 - 127.0) / 128.0;
        }
    }
    chw
}

/// Runs UltraFace against one decoded RGBA frame and returns every face detected above
/// [`SCORE_THRESHOLD`], sorted by descending score, boxes as fractions of `(width, height)`.
/// `model_path` is a `version-RFB-320` ONNX file (see module docs for where to get one).
pub fn detect_faces(
    model_path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<FaceBox>, ReframeError> {
    detect_faces_with(rgba, width, height, |input| {
        crate::onnx_models::run_face_model(model_path, input, MODEL_INPUT_WIDTH, MODEL_INPUT_HEIGHT)
    })
}

/// [`detect_faces`] with the model run passed in: it receives the preprocessed input tensor and
/// returns every output tensor as `(dims, data)`, so the decoding is testable without a model.
pub(crate) fn detect_faces_with(
    rgba: &[u8],
    width: u32,
    height: u32,
    run: impl FnOnce(Vec<f32>) -> Result<Vec<super::onnx_models::FaceTensorOutput>, ReframeError>,
) -> Result<Vec<FaceBox>, ReframeError> {
    let tensors = run(preprocess(rgba, width, height))?;
    // The model publishes two outputs, "scores" (`1x4420x2`, background/face) and "boxes"
    // (`1x4420x4`, xmin/ymin/xmax/ymax in 0..1 fractions) — told apart here by each tensor's
    // last dimension rather than trusted output order, since ONNX doesn't guarantee that order
    // survives export/re-export.
    let (num_anchors, scores, boxes) = select_outputs(tensors)?;
    Ok(decode_faces(num_anchors, &scores, &boxes))
}

fn select_outputs(
    tensors: Vec<super::onnx_models::FaceTensorOutput>,
) -> Result<(usize, Vec<f32>, Vec<f32>), ReframeError> {
    let mut scores_flat = None;
    let mut boxes_flat = None;
    for tensor in tensors {
        match tensor.dims.last() {
            Some(2) => scores_flat = Some(tensor),
            Some(4) => boxes_flat = Some(tensor),
            _ => {}
        }
    }
    let score_dims = scores_flat
        .ok_or_else(|| ReframeError::Onnx("no scores output (last dim 2) found".into()))?;
    let boxes = boxes_flat
        .ok_or_else(|| ReframeError::Onnx("no boxes output (last dim 4) found".into()))?;
    let num_anchors = *score_dims.dims.get(1).unwrap_or(&0) as usize;
    Ok((num_anchors, score_dims.data, boxes.data))
}

/// Turns the raw model outputs into face boxes: keeps anchors scoring above the threshold,
/// clamps their corners to the frame, drops empty boxes and suppresses overlaps.
fn decode_faces(num_anchors: usize, scores: &[f32], boxes: &[f32]) -> Vec<FaceBox> {
    let mut candidates = Vec::new();
    for i in 0..num_anchors {
        let face_score = scores[i * 2 + 1];
        if face_score < SCORE_THRESHOLD {
            continue;
        }
        let xmin = boxes[i * 4].clamp(0.0, 1.0);
        let ymin = boxes[i * 4 + 1].clamp(0.0, 1.0);
        let xmax = boxes[i * 4 + 2].clamp(0.0, 1.0);
        let ymax = boxes[i * 4 + 3].clamp(0.0, 1.0);
        if xmax <= xmin || ymax <= ymin {
            continue;
        }
        candidates.push(FaceBox {
            x: xmin,
            y: ymin,
            w: xmax - xmin,
            h: ymax - ymin,
            score: face_score,
        });
    }
    non_max_suppress(candidates)
}

fn non_max_suppress(mut candidates: Vec<FaceBox>) -> Vec<FaceBox> {
    candidates.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<FaceBox> = Vec::new();
    for candidate in candidates {
        if kept.iter().all(|k| k.iou(candidate) < NMS_IOU_THRESHOLD) {
            kept.push(candidate);
        }
    }
    kept
}

/// Picks the "main subject" from a set of detected faces — highest score wins, which in
/// practice also tends to be the largest/most centered face UltraFace found. `None` if `faces`
/// is empty (nothing detected, e.g. gameplay footage with no on-camera face).
pub fn main_subject_center(faces: &[FaceBox]) -> Option<(f32, f32)> {
    faces
        .iter()
        .max_by(|a, b| a.score.total_cmp(&b.score))
        .map(|f| f.center())
}

/// A normalized crop rectangle, in the same `(crop_x, crop_y, crop_w, crop_h)` fraction-of-frame
/// convention as [`crate::timeline::ClipInstance`]'s own crop fields.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// Fits `target_aspect_w:target_aspect_h` inside the `source_w x source_h` frame at maximal
/// size (full height with a narrower width, or full width with a shorter height — whichever
/// keeps the crop inside the frame), then centers that rect on `subject_center` (a fraction of
/// the source frame) if given, clamping so the rect never extends past the frame edge. Falls
/// back to a plain center crop when `subject_center` is `None` (no face detected, or detection
/// unavailable/skipped).
pub fn compute_reframe_crop(
    source_w: u32,
    source_h: u32,
    target_aspect_w: u32,
    target_aspect_h: u32,
    subject_center: Option<(f32, f32)>,
) -> CropRect {
    let source_aspect = source_w as f32 / source_h as f32;
    let target_aspect = target_aspect_w as f32 / target_aspect_h as f32;

    let (crop_w, crop_h) = if target_aspect <= source_aspect {
        // Target is narrower (or equal) than source — full height, narrower width.
        (target_aspect / source_aspect, 1.0)
    } else {
        // Target is wider than source — full width, shorter height.
        (1.0, source_aspect / target_aspect)
    };

    let (center_x, center_y) = subject_center.unwrap_or((0.5, 0.5));
    let x = (center_x - crop_w / 2.0).clamp(0.0, 1.0 - crop_w);
    let y = (center_y - crop_h / 2.0).clamp(0.0, 1.0 - crop_h);

    CropRect {
        x,
        y,
        w: crop_w,
        h: crop_h,
    }
}

#[cfg(test)]
#[path = "auto_reframe/auto_reframe_test.rs"]
mod tests;
