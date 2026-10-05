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

//! CF-09 (`spec/architecture/competitive-feature-plan.md`)'s slice 2 — "Propagate masks between
//! sampled frames and offer manual correction at failure points." Builds directly on
//! [`crate::motion_tracking::track_region_with_scores`]'s existing block-matching tracker
//! (reused, not reimplemented) rather than a new segmentation model: an arbitrary user-seeded
//! mask (any polygon, drawn once on the seed frame) is propagated by *rigid translation* — every
//! vertex moves by the same per-frame delta the tracked center moves by. This is deliberately
//! not real per-pixel segmentation propagation (no rotation, scale, or deformation), but it needs
//! no model weights, no network access, and no `libonnxruntime` — CF-09's slice 1 (an actual
//! local segmentation model, and general-shape mask propagation beyond rigid translation) remains
//! a genuine, separate follow-up once a model can actually be verified against, the same category
//! of environment-limited stopping point as CF-08's real-embedding gap.
//!
//! Match quality (the tracker's own sum-of-absolute-differences score, normalized via
//! [`match_confidence`]) doubles as this slice's own "failure point" signal: a frame whose best
//! match is a poor fit is flagged [`PropagatedMask::needs_correction`], and
//! [`propagate_mask_with_corrections`] lets a caller supply an observed correction at any frame
//! index, at which point tracking re-seeds from the corrected vertices/center and continues —
//! the doc's own "propagate... and offer manual correction at failure points" shape, without
//! inventing a second tracking algorithm.
//!
//! [`rasterize_to_matte_frames`] bridges into slice 3 ("Store generated matte data outside the
//! main project JSON with versioned references and cache invalidation") for free: it turns a
//! sequence of [`PropagatedMask`]s into exactly the `Vec<Vec<u8>>` grayscale-per-frame shape
//! [`crate::background_removal::encode_matte_video`] already accepts, so an arbitrary-object mask
//! reuses that same on-disk matte-video storage/caching convention background-removal already
//! established — no new storage format needed for this slice either.

use crate::motion_tracking::{track_region_with_scores, GrayFrame};
use crate::shape_render::point_in_polygon;

/// Below this normalized confidence (`0.0..=1.0`, see [`match_confidence`]), a propagated frame
/// is flagged [`PropagatedMask::needs_correction`] — the doc's own "failure points" needing a
/// manual correction affordance.
pub const DEFAULT_LOW_CONFIDENCE_THRESHOLD: f32 = 0.5;

/// One frame's propagated mask: the seed polygon's vertices translated to follow the tracked
/// point, plus how well the tracker matched this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct PropagatedMask {
    /// The seed mask's own vertices (`(x_frac, y_frac)`, fraction of frame size — the same
    /// convention [`crate::timeline::ShapeKind::Polygon`] already uses), each translated by this
    /// frame's own tracked delta from the seed frame (or the most recent correction — see
    /// [`propagate_mask_with_corrections`]).
    pub vertices: Vec<(f32, f32)>,
    /// `0.0` (worst) to `1.0` (best) — see [`match_confidence`].
    pub confidence: f32,
    /// `confidence < threshold`, where `threshold` is whatever the caller passed to
    /// [`propagate_mask_by_translation`]/[`propagate_mask_with_corrections`] (typically
    /// [`DEFAULT_LOW_CONFIDENCE_THRESHOLD`]) — the frame this slice's "manual correction"
    /// affordance should surface first.
    pub needs_correction: bool,
}

/// Normalizes a tracker match score (sum of absolute per-pixel luma differences, `0` = perfect
/// match) against the theoretical worst case for a `template_width`x`template_height` template
/// (every pixel off by the maximum possible `255`), producing a `0.0..=1.0` confidence where
/// higher is better. A non-positive template area (shouldn't happen in practice — the tracker
/// clamps template dimensions to at least `4`) is defined as `0.0` confidence rather than
/// dividing by zero.
pub fn match_confidence(score: i64, template_width: i32, template_height: i32) -> f32 {
    let max_score = (template_width as i64) * (template_height as i64) * 255;
    if max_score <= 0 {
        return 0.0;
    }
    (1.0 - (score as f64 / max_score as f64)).clamp(0.0, 1.0) as f32
}

/// Propagates `seed_vertices` across `frames` by rigid translation: tracks
/// `(initial_center_x_frac, initial_center_y_frac)` via [`track_region_with_scores`], then adds
/// each frame's own delta from the seed frame's tracked center to every seed vertex. Empty input
/// (`frames` empty) returns an empty result. `frames[0]`'s own confidence is always `1.0` — it's
/// the template itself, a perfect self-match by construction, matching
/// [`crate::motion_tracking::TrackResult::scores`]'s own convention.
#[allow(clippy::too_many_arguments)]
pub fn propagate_mask_by_translation(
    seed_vertices: &[(f32, f32)],
    initial_center_x_frac: f32,
    initial_center_y_frac: f32,
    frames: &[GrayFrame],
    template_width_frac: f32,
    template_height_frac: f32,
    search_radius_frac: f32,
    low_confidence_threshold: f32,
) -> Vec<PropagatedMask> {
    let result = track_region_with_scores(
        frames,
        initial_center_x_frac,
        initial_center_y_frac,
        template_width_frac,
        template_height_frac,
        search_radius_frac,
    );
    let Some(first) = result.positions.first() else {
        return Vec::new();
    };
    let (first_x, first_y) = (first.center_x_frac, first.center_y_frac);
    result
        .positions
        .iter()
        .zip(&result.scores)
        .map(|(pos, &score)| {
            let dx = pos.center_x_frac - first_x;
            let dy = pos.center_y_frac - first_y;
            let vertices = seed_vertices
                .iter()
                .map(|&(x, y)| (x + dx, y + dy))
                .collect();
            let confidence = match_confidence(score, result.template_width, result.template_height);
            PropagatedMask {
                vertices,
                confidence,
                needs_correction: confidence < low_confidence_threshold,
            }
        })
        .collect()
}

/// One caller-observed manual correction: at `frame_index` (into `frames`, `1..frames.len()` —
/// correcting frame `0` makes no sense, it's the seed), the mask is replaced with `vertices`,
/// and tracking re-seeds from `(center_x_frac, center_y_frac)` for every frame from
/// `frame_index` onward, up to the next correction (or the end of `frames`).
pub struct MaskCorrection {
    pub frame_index: usize,
    pub vertices: Vec<(f32, f32)>,
    pub center_x_frac: f32,
    pub center_y_frac: f32,
}

/// Same as [`propagate_mask_by_translation`], but restarts tracking from each entry in
/// `corrections` in turn — so the tracker never keeps drifting from a source of error a caller
/// already observed and fixed. `corrections` must be sorted by [`MaskCorrection::frame_index`]
/// ascending; an out-of-order entry (`frame_index` at or before the current segment's own start)
/// or an out-of-range one (`frame_index > frames.len()`) is skipped rather than applied, so a
/// caller building this list incrementally can't corrupt an already-propagated prefix.
#[allow(clippy::too_many_arguments)]
pub fn propagate_mask_with_corrections(
    seed_vertices: &[(f32, f32)],
    initial_center_x_frac: f32,
    initial_center_y_frac: f32,
    frames: &[GrayFrame],
    template_width_frac: f32,
    template_height_frac: f32,
    search_radius_frac: f32,
    low_confidence_threshold: f32,
    corrections: &[MaskCorrection],
) -> Vec<PropagatedMask> {
    if frames.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(frames.len());
    let mut seg_start = 0usize;
    let mut seg_vertices: Vec<(f32, f32)> = seed_vertices.to_vec();
    let mut seg_center_x = initial_center_x_frac;
    let mut seg_center_y = initial_center_y_frac;

    for correction in corrections {
        if correction.frame_index <= seg_start || correction.frame_index > frames.len() {
            continue;
        }
        out.extend(propagate_mask_by_translation(
            &seg_vertices,
            seg_center_x,
            seg_center_y,
            &frames[seg_start..correction.frame_index],
            template_width_frac,
            template_height_frac,
            search_radius_frac,
            low_confidence_threshold,
        ));
        seg_start = correction.frame_index;
        seg_vertices = correction.vertices.clone();
        seg_center_x = correction.center_x_frac;
        seg_center_y = correction.center_y_frac;
    }
    if seg_start < frames.len() {
        out.extend(propagate_mask_by_translation(
            &seg_vertices,
            seg_center_x,
            seg_center_y,
            &frames[seg_start..],
            template_width_frac,
            template_height_frac,
            search_radius_frac,
            low_confidence_threshold,
        ));
    }
    out
}

/// Rasterizes one [`PropagatedMask`]'s vertices (fraction-of-frame coordinates, as every function
/// in this module produces/consumes) into a `width * height` grayscale-as-luma byte buffer — `255`
/// inside the polygon, `0` outside — via [`point_in_polygon`]'s existing ray-casting test (reused
/// exactly as [`crate::overlay_render`]'s own shape rasterizer already uses it, just without that
/// module's per-shape center/rotation transform, since a mask polygon here is already in absolute
/// frame-fraction coordinates). A polygon with fewer than 3 vertices can enclose no area, so every
/// pixel is `0` rather than calling into a ray-cast that can't meaningfully answer "inside."
pub fn rasterize_mask_to_matte(mask: &PropagatedMask, width: u32, height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; (width as usize) * (height as usize)];
    if mask.vertices.len() < 3 || width == 0 || height == 0 {
        return buf;
    }
    let vertices_px: Vec<(f64, f64)> = mask
        .vertices
        .iter()
        .map(|&(x, y)| (x as f64 * width as f64, y as f64 * height as f64))
        .collect();
    for y in 0..height {
        for x in 0..width {
            let px = x as f64 + 0.5;
            let py = y as f64 + 0.5;
            if point_in_polygon((px, py), &vertices_px) {
                buf[(y * width + x) as usize] = 255;
            }
        }
    }
    buf
}

/// [`rasterize_mask_to_matte`] applied to a whole propagated sequence — exactly the `Vec<Vec<u8>>`
/// shape [`crate::background_removal::encode_matte_video`] expects, so a caller can pipe
/// [`propagate_mask_by_translation`]/[`propagate_mask_with_corrections`]'s output straight into
/// that existing encoder with no reshaping in between.
pub fn rasterize_to_matte_frames(
    masks: &[PropagatedMask],
    width: u32,
    height: u32,
) -> Vec<Vec<u8>> {
    masks
        .iter()
        .map(|mask| rasterize_mask_to_matte(mask, width, height))
        .collect()
}

#[cfg(test)]
#[path = "mask_propagation/mask_propagation_test.rs"]
mod tests;
