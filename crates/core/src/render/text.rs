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

use crate::keyframe;
use crate::project::Sequence;
use crate::timeline::{TextClip, TextFontFamily, TextFontStyle, Timeline, TrackKind};
use avbridge::Canvas;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

/// One resolved text overlay saved in an export job. Unlike avbridge's raw image-overlay FFI
/// input, this remains semantic text plus styling so a persisted queue does not depend on
/// temporary PNG files. [`apply_text_overlay_pass`] rasterizes it only when the job runs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextSegment {
    pub start_secs: f64,
    pub duration_secs: f64,
    pub text: String,
    pub font_size: f32,
    #[serde(default)]
    pub font_family: TextFontFamily,
    #[serde(default)]
    pub font_style: TextFontStyle,
    /// See [`crate::timeline::TextClip::font_weight`]'s own doc comment.
    #[serde(default)]
    pub font_weight: Option<u16>,
    pub color_rgba: [u8; 4],
    #[serde(default)]
    pub background_rgba: [u8; 4],
    #[serde(default)]
    pub background_padding: f32,
    #[serde(default)]
    pub background_corner_radius: f32,
    /// Optional UTF-8 byte range within `text` to rasterize. Word-highlight segments keep the
    /// full caption here and select just one word, so they inherit the base caption's exact
    /// fontdue wrapping and line position. `None` renders the whole string.
    #[serde(default)]
    pub glyph_byte_range: Option<[u32; 2]>,
    pub pos_x: f32,
    pub pos_y: f32,
    /// Pre-built `geq` alpha-multiplier expression for this clip's `opacity_keyframes`
    /// (`keyframe::text_opacity_alpha_expr`), built once from the *base clip's* own
    /// `start_secs`/`duration_secs` in [`text_clip_to_segments`] and copied onto every segment
    /// derived from that clip (base plus any per-word highlight segments) — the expression is
    /// self-contained/timeline-absolute, so it stays correct even on a word-highlight segment
    /// whose own `start_secs`/`duration_secs` (used only for its `enable=between(...)`
    /// visibility window) differ from the base clip's. Empty means "no fade".
    #[serde(default)]
    pub opacity_keyframe_expr: String,
    /// Pre-built `overlay` `x`/`y` pixel-offset expressions for this clip's
    /// `pos_x_keyframes`/`pos_y_keyframes` (`keyframe::text_position_offset_expr`), built once
    /// from the *base clip's* own `start_secs`/`duration_secs` in [`text_clip_to_segments`] and
    /// copied onto every segment derived from that clip, same "self-contained/timeline-absolute,
    /// stays correct on a word-highlight segment too" reasoning as `opacity_keyframe_expr`.
    /// Empty means "no offset" (`overlay=x=0:y=0`, same as before this field existed).
    #[serde(default)]
    pub position_keyframe_expr_x: String,
    #[serde(default)]
    pub position_keyframe_expr_y: String,
    /// Pre-built `geq` inverse-sample coordinate expressions for this clip's `scale_keyframes`
    /// (`keyframe::text_scale_sample_exprs`), same "built once from the base clip, copied onto
    /// every derived segment" treatment as `position_keyframe_expr_x`/`_y`. Empty (either one)
    /// means "no remap" (the raster's own unscaled pixels, same as before this field existed).
    #[serde(default)]
    pub scale_keyframe_expr_x: String,
    #[serde(default)]
    pub scale_keyframe_expr_y: String,
    /// Pre-built `geq` inverse-sample coordinate expressions for this clip's `rotation_keyframes`
    /// (`keyframe::text_rotation_sample_exprs`), same "built once from the base clip, copied onto
    /// every derived segment" treatment as `scale_keyframe_expr_x`/`_y`. Empty (either one) means
    /// "no remap" (the raster's own unrotated pixels, same as before this field existed).
    #[serde(default)]
    pub rotation_keyframe_expr_x: String,
    #[serde(default)]
    pub rotation_keyframe_expr_y: String,
    /// Paragraph base-direction override — see [`crate::timeline::TextDirection`].
    /// `#[serde(default)]` so an already-queued export job (persisted `.ocqueue` bytes) loads as
    /// `Auto`, the exact behavior every segment already had before this field existed.
    #[serde(default)]
    pub direction: crate::timeline::TextDirection,
    /// Horizontal text alignment — see [`crate::timeline::TextAlign`]. `#[serde(default)]` so an
    /// already-queued export job loads as `Auto`, the exact behavior every segment already had
    /// before this field existed.
    #[serde(default)]
    pub text_align: crate::timeline::TextAlign,
}

/// Applies text overlays to an already-written export file in place. Writes to a temp path
/// beside `output`, then renames over `output`. Logs and silently skips on any error so an
/// overlay-raster or filter failure doesn't destroy the already-completed video file.
pub(crate) fn apply_text_overlay_pass(
    output: &Path,
    canvas: Canvas,
    text_segments: &[TextSegment],
) {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let temp_id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}-{temp_id}", std::process::id());
    let tmp = parent.join(format!("{stem}.oca-text-{suffix}.mp4"));
    let overlay_dir = parent.join(format!(".{stem}.oca-text-{suffix}"));
    if let Err(error) = std::fs::create_dir(&overlay_dir) {
        tracing::error!(error = %error, output = %output.display(), "text overlay temp directory failed");
        return;
    }

    let mut overlays = Vec::with_capacity(text_segments.len());
    for (index, segment) in text_segments.iter().enumerate() {
        let rgba =
            crate::overlay_render::render_text_segment_rgba(segment, canvas.width, canvas.height);
        let overlay_path = overlay_dir.join(format!("overlay-{index}.png"));
        if let Err(error) = image::save_buffer_with_format(
            &overlay_path,
            &rgba,
            canvas.width,
            canvas.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ) {
            tracing::error!(error = %error, output = %output.display(), "text overlay raster write failed");
            let _ = std::fs::remove_dir_all(&overlay_dir);
            return;
        }
        overlays.push(avbridge::TextOverlaySegment {
            start_secs: segment.start_secs,
            duration_secs: segment.duration_secs,
            overlay_path,
            opacity_keyframe_expr: segment.opacity_keyframe_expr.clone(),
            position_keyframe_expr_x: segment.position_keyframe_expr_x.clone(),
            position_keyframe_expr_y: segment.position_keyframe_expr_y.clone(),
            scale_keyframe_expr_x: segment.scale_keyframe_expr_x.clone(),
            scale_keyframe_expr_y: segment.scale_keyframe_expr_y.clone(),
            rotation_keyframe_expr_x: segment.rotation_keyframe_expr_x.clone(),
            rotation_keyframe_expr_y: segment.rotation_keyframe_expr_y.clone(),
        });
    }

    match avbridge::apply_text_overlays(
        output,
        &tmp,
        &overlays,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
    ) {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, output) {
                tracing::error!(error = %e, output = %output.display(), "text overlay rename failed");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, output = %output.display(), "text overlay skipped");
            let _ = std::fs::remove_file(&tmp);
        }
    }
    let _ = std::fs::remove_dir_all(&overlay_dir);
}

/// Collects all [`TextClip`]s from `sequence`'s text tracks into [`TextSegment`]s,
/// sorted by `start_secs` ascending. Returns an empty vec if the sequence has no text tracks
/// or none have any clips. Used to pass text overlays to the post-processing pass after the
/// main video encode ([`render_export_job`]).
///
/// `canvas_width` is retained as a zero-width guard for malformed export settings; word
/// highlights otherwise use UTF-8 byte ranges into the complete caption, letting the shared
/// rasterizer place them with the exact same wrapping as the base text. `canvas_height` is
/// needed only to scale `pos_y_keyframes`' offset expression into pixels.
pub fn resolve_text_segments(
    sequence: &Sequence,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<TextSegment> {
    resolve_timeline_text_segments(&sequence.timeline, canvas_width, canvas_height)
}

/// [`resolve_text_segments`] for a bare [`Timeline`], for callers (the preview) that have no
/// [`Sequence`] around the timeline they are showing.
pub fn resolve_timeline_text_segments(
    timeline: &Timeline,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Text)
        .flat_map(|t| &t.text_clips)
        .flat_map(|clip| text_clip_to_segments(clip, canvas_width, canvas_height))
        .collect();
    segments.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    segments
}

/// Expands one [`TextClip`] into one or more [`TextSegment`]s. A plain clip (no words,
/// or `highlight_enabled` off) is exactly the one segment it's always been. A word-highlight
/// clip becomes a base segment (the full text, in `color_rgba`, for the clip's whole duration —
/// so something is always on screen even between words / before the first word starts) plus one
/// short segment per matched word (the complete caption plus that word's UTF-8 byte range, in
/// `highlight_color_rgba`, only for its own `[start_secs, end_secs)`). Rendering the complete
/// caption layout and filtering its glyphs by byte range makes highlights follow both explicit
/// newlines and automatic word wrapping exactly.
pub(crate) fn text_clip_to_segments(
    clip: &TextClip,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<TextSegment> {
    // Built once from the base clip's own timing (not any individual segment's) and copied onto
    // every segment below -- see TextSegment::opacity_keyframe_expr's doc comment for why.
    let opacity_keyframe_expr = keyframe::text_opacity_alpha_expr(
        &clip.opacity_keyframes,
        clip.start_secs,
        clip.duration_secs,
    )
    .unwrap_or_default();

    // The raster anchor: the first keyframe's value when position keyframes are present (same
    // "keyframes win when present" convention as ShapeClip's own fields), the plain constant
    // otherwise -- see TextClip::pos_x_keyframes' doc comment. position_keyframe_expr_x/y is
    // then the *delta* from this anchor, in canvas pixels, as a function of timeline-absolute
    // `t` -- zero for every unkeyframed axis, so this is a no-op when neither axis is animated.
    let base_pos_x = clip.pos_x_keyframes.first().map_or(clip.pos_x, |k| k.value);
    let base_pos_y = clip.pos_y_keyframes.first().map_or(clip.pos_y, |k| k.value);
    let position_keyframe_expr_x = keyframe::text_position_offset_expr(
        &clip.pos_x_keyframes,
        base_pos_x,
        clip.start_secs,
        clip.duration_secs,
        canvas_width as f32,
    )
    .unwrap_or_default();
    let position_keyframe_expr_y = keyframe::text_position_offset_expr(
        &clip.pos_y_keyframes,
        base_pos_y,
        clip.start_secs,
        clip.duration_secs,
        canvas_height as f32,
    )
    .unwrap_or_default();

    // The scale remap's own anchor is the same raster-baked position anchor above, in canvas
    // pixels -- see TextClip::scale_keyframes' doc comment.
    let (scale_keyframe_expr_x, scale_keyframe_expr_y) = keyframe::text_scale_sample_exprs(
        &clip.scale_keyframes,
        base_pos_x * canvas_width as f32,
        base_pos_y * canvas_height as f32,
        clip.start_secs,
        clip.duration_secs,
    )
    .unwrap_or_default();

    // Same anchor as the scale remap above -- see TextClip::rotation_keyframes' doc comment.
    let (rotation_keyframe_expr_x, rotation_keyframe_expr_y) =
        keyframe::text_rotation_sample_exprs(
            &clip.rotation_keyframes,
            base_pos_x * canvas_width as f32,
            base_pos_y * canvas_height as f32,
            clip.start_secs,
            clip.duration_secs,
        )
        .unwrap_or_default();

    let base = TextSegment {
        start_secs: clip.start_secs,
        duration_secs: clip.duration_secs,
        text: clip.text.clone(),
        font_size: clip.font_size,
        font_family: clip.font_family.clone(),
        font_style: clip.font_style,
        font_weight: clip.font_weight,
        color_rgba: clip.color_rgba,
        background_rgba: clip.background_rgba,
        background_padding: clip.background_padding,
        background_corner_radius: clip.background_corner_radius,
        glyph_byte_range: None,
        pos_x: base_pos_x,
        pos_y: base_pos_y,
        opacity_keyframe_expr: opacity_keyframe_expr.clone(),
        position_keyframe_expr_x: position_keyframe_expr_x.clone(),
        position_keyframe_expr_y: position_keyframe_expr_y.clone(),
        scale_keyframe_expr_x: scale_keyframe_expr_x.clone(),
        scale_keyframe_expr_y: scale_keyframe_expr_y.clone(),
        rotation_keyframe_expr_x: rotation_keyframe_expr_x.clone(),
        rotation_keyframe_expr_y: rotation_keyframe_expr_y.clone(),
        direction: clip.direction,
        text_align: clip.text_align,
    };
    if !clip.highlight_enabled || clip.words.is_empty() || canvas_width == 0 {
        return vec![base];
    }

    let words: Vec<&str> = clip.words.iter().map(|w| w.text.as_str()).collect();
    let byte_ranges = crate::text_metrics::word_byte_ranges(&clip.text, &words);

    let mut segments = Vec::with_capacity(1 + clip.words.len());
    segments.push(base);
    for (word, byte_range) in clip.words.iter().zip(byte_ranges) {
        let Some([start, end]) = byte_range else {
            continue;
        };
        let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
            continue;
        };
        segments.push(TextSegment {
            start_secs: clip.start_secs + word.start_secs,
            duration_secs: (word.end_secs - word.start_secs).max(0.05),
            text: clip.text.clone(),
            font_size: clip.font_size,
            font_family: clip.font_family.clone(),
            font_style: clip.font_style,
            font_weight: clip.font_weight,
            color_rgba: clip.highlight_color_rgba,
            background_rgba: [0, 0, 0, 0],
            background_padding: 0.0,
            background_corner_radius: 0.0,
            glyph_byte_range: Some([start, end]),
            pos_x: base_pos_x,
            pos_y: base_pos_y,
            opacity_keyframe_expr: opacity_keyframe_expr.clone(),
            position_keyframe_expr_x: position_keyframe_expr_x.clone(),
            position_keyframe_expr_y: position_keyframe_expr_y.clone(),
            scale_keyframe_expr_x: scale_keyframe_expr_x.clone(),
            scale_keyframe_expr_y: scale_keyframe_expr_y.clone(),
            rotation_keyframe_expr_x: rotation_keyframe_expr_x.clone(),
            rotation_keyframe_expr_y: rotation_keyframe_expr_y.clone(),
            direction: clip.direction,
            text_align: clip.text_align,
        });
    }
    segments
}
