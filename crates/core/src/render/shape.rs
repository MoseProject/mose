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

use crate::project::Sequence;
use crate::timeline::{ShapeClip, TrackKind};
use avbridge::Canvas;
use std::path::Path;

/// Applies shape overlays to an already-written export file in place — same "temp path, rename
/// over `output`, log-and-skip on error" shape as [`apply_text_overlay_pass`].
pub(crate) fn apply_shape_overlay_pass(
    output: &Path,
    canvas: Canvas,
    shape_segments: &[avbridge::ShapeSegment],
) {
    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let tmp = parent.join(format!("{stem}.shape_tmp.mp4"));

    match avbridge::apply_shape_overlays(
        output,
        &tmp,
        shape_segments,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
    ) {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, output) {
                tracing::error!(error = %e, output = %output.display(), "shape overlay rename failed");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, output = %output.display(), "shape overlay skipped");
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// Collects all [`ShapeClip`]s from `sequence`'s shape tracks into [`avbridge::ShapeSegment`]s
/// (each already a complete `geq` filter node, via
/// `crate::shape_render::build_shape_filter_desc`), sorted by `start_secs` ascending. Returns
/// an empty vec if the sequence has no shape tracks or none have any clips. Same role as
/// [`resolve_text_segments`], one post-processing pass earlier/later in the chain (see
/// [`render_export_job`]).
pub fn resolve_shape_segments(
    sequence: &Sequence,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<avbridge::ShapeSegment> {
    let mut clips: Vec<&ShapeClip> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Shape)
        .flat_map(|t| &t.shape_clips)
        .collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    clips
        .into_iter()
        .map(|clip| shape_clip_to_segment(clip, canvas_width, canvas_height))
        .collect()
}

/// Converts one [`ShapeClip`] into its [`avbridge::ShapeSegment`] — the actual geometry/color/
/// rotation math lives in [`crate::shape_render::build_shape_filter_desc`], this just adapts
/// field names/types across that boundary.
pub(crate) fn shape_clip_to_segment(
    clip: &ShapeClip,
    canvas_width: u32,
    canvas_height: u32,
) -> avbridge::ShapeSegment {
    let filter_desc = crate::shape_render::build_shape_filter_desc(&crate::ShapeRenderInput {
        shape_kind: &clip.shape_kind,
        center_x: clip.center_x,
        center_y: clip.center_y,
        center_x_keyframes: &clip.center_x_keyframes,
        center_y_keyframes: &clip.center_y_keyframes,
        width: clip.width,
        height: clip.height,
        width_keyframes: &clip.width_keyframes,
        height_keyframes: &clip.height_keyframes,
        rotation_deg: clip.rotation_deg,
        rotation_keyframes: &clip.rotation_keyframes,
        color_rgba: clip.color_rgba,
        stroke_thickness_px: clip.stroke_thickness_px,
        start_secs: clip.start_secs,
        duration_secs: clip.duration_secs,
        canvas_width,
        canvas_height,
    });
    avbridge::ShapeSegment { filter_desc }
}
