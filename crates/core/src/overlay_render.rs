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

//! Rasterizes a [`crate::timeline::TextClip`] or [`crate::timeline::ShapeClip`] into a single
//! full-canvas RGBA buffer — text export now writes this exact buffer to a temporary PNG while
//! shape export still uses a `geq` avfilter pass ([`crate::render`], [`crate::shape_render`]),
//! only ever run against an already-encoded file and have no live GStreamer element equivalent
//! ([`crate::preview`]'s doc comment previously listed this as a known gap — see
//! `Preview::open_composited`, which now feeds each buffer this module builds into the
//! compositor as its own static overlay branch).
//!
//! Both clip kinds have static geometry for their whole visible span (no keyframes on either
//! type). Shape preview therefore needs one RGBA buffer only. Text preview also starts with one,
//! then replaces it only when playback enters another timed word; `imagefreeze` repeats the
//! latest buffer between boundaries without per-video-frame rasterization.
//!
//! Text shapes and rasterizes through [`crate::text_layout`]'s `cosmic-text` adapter
//! (TEXT-01A step 3 — see that module's own doc comment), wrapping at word boundaries once a
//! line would run past the canvas's right edge. Word-highlight timing
//! ([`crate::timeline::TextClip::words`]) is rendered through [`render_text_clip_rgba`]'s
//! `local_time_secs` parameter. Preview calls it on word-boundary changes; export emits one
//! precisely timed PNG overlay per word.
//!
//! Shapes mirror [`crate::shape_render::build_shape_filter_desc`]'s per-pixel math term-for-term
//! (rotate into the shape's local frame, then an ellipse quadratic or
//! [`crate::shape_render::point_in_polygon`] ray-cast), just evaluated directly against a pixel
//! buffer in Rust instead of compiled into a `geq` expression string.

use std::ops::Range;

use crate::render::TextSegment;
use crate::shape_render::point_in_polygon;
use crate::text_layout::{self, ShapedText, TextLayoutEngine, TextLayoutError};
use crate::timeline::{MaskShape, ShapeClip, ShapeKind, TextClip};

/// Writes `[r, g, b, a]` at `(x, y)` into a `width`×`height` RGBA buffer, `a` already the final
/// (straight, not premultiplied) alpha to store — glyphs/shape pixels don't overlap in practice
/// (distinct characters, one shape per buffer), so a plain overwrite is enough; no need to blend
/// against whatever's already there.
fn put_pixel(buf: &mut [u8], width: u32, height: u32, x: i64, y: i64, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height {
        return;
    }
    let idx = (y as u32 * width + x as u32) as usize * 4;
    buf[idx..idx + 4].copy_from_slice(&rgba);
}

/// Alpha-composites one straight-alpha RGBA pixel over the existing buffer pixel.
fn blend_pixel(buf: &mut [u8], width: u32, height: u32, x: i64, y: i64, rgba: [u8; 4]) {
    if x < 0 || y < 0 || x as u32 >= width || y as u32 >= height || rgba[3] == 0 {
        return;
    }
    let idx = (y as u32 * width + x as u32) as usize * 4;
    let src_a = rgba[3] as f32 / 255.0;
    let dst_a = buf[idx + 3] as f32 / 255.0;
    let out_a = src_a + dst_a * (1.0 - src_a);
    if out_a <= f32::EPSILON {
        return;
    }
    for channel in 0..3 {
        let src = rgba[channel] as f32 / 255.0;
        let dst = buf[idx + channel] as f32 / 255.0;
        let out = (src * src_a + dst * dst_a * (1.0 - src_a)) / out_a;
        buf[idx + channel] = (out * 255.0).round().clamp(0.0, 255.0) as u8;
    }
    buf[idx + 3] = (out_a * 255.0).round().clamp(0.0, 255.0) as u8;
}

/// `glyph_byte_range` selects every glyph cluster that overlaps the logical timed-word range.
/// This expands a highlight to complete ligatures, combining marks, and conjuncts rather than
/// splitting a shaped cluster at a UTF-8 byte boundary.
fn glyph_excluded(cluster: &Range<usize>, glyph_byte_range: Option<[u32; 2]>) -> bool {
    glyph_byte_range.is_some_and(|[start, end]| {
        let start = start as usize;
        let end = end as usize;
        cluster.end <= start || cluster.start >= end
    })
}

/// Rasterizes every glyph in `shaped` matching `glyph_byte_range` into `buf`, blended in `rgba`.
/// `engine`/`swash_cache` come from [`text_layout::with_shared_engine`] so this never builds its
/// own `FontSystem`.
#[allow(clippy::too_many_arguments)]
fn draw_shaped_text(
    buf: &mut [u8],
    shaped: &ShapedText,
    rgba: [u8; 4],
    glyph_byte_range: Option<[u32; 2]>,
    canvas_width: u32,
    canvas_height: u32,
    engine: &mut text_layout::TextLayoutEngine,
    swash_cache: &mut cosmic_text::SwashCache,
) {
    let [r, g, b, a] = rgba;
    let base_color = cosmic_text::Color::rgb(r, g, b);
    for line in &shaped.lines {
        for glyph in &line.glyphs {
            if glyph_excluded(&glyph.cluster, glyph_byte_range) {
                continue;
            }
            let (cache_key, phys_x, phys_y) =
                (glyph.physical.cache_key, glyph.physical.x, glyph.physical.y);
            swash_cache.with_pixels(
                engine.font_system_mut(),
                cache_key,
                base_color,
                |x, y, color| {
                    let coverage = color.a();
                    if coverage == 0 {
                        return;
                    }
                    let pixel_alpha = (coverage as u32 * a as u32 / 255) as u8;
                    if pixel_alpha == 0 {
                        return;
                    }
                    blend_pixel(
                        buf,
                        canvas_width,
                        canvas_height,
                        (phys_x + x) as i64,
                        (phys_y + y) as i64,
                        [color.r(), color.g(), color.b(), pixel_alpha],
                    );
                },
            );
        }
    }
}

/// The tight rasterized-ink bounding box (not the wider advance-box geometry) across every glyph
/// in `shaped` matching `glyph_byte_range` — matches the previous `fontdue` renderer's own
/// `glyph.width`/`glyph.height` semantics (the rendered bitmap's own extent), which is what makes
/// a text background hug the visible letterforms rather than their full advance boxes.
fn shaped_ink_bbox(
    shaped: &ShapedText,
    glyph_byte_range: Option<[u32; 2]>,
    engine: &mut text_layout::TextLayoutEngine,
    swash_cache: &mut cosmic_text::SwashCache,
) -> Option<(f32, f32, f32, f32)> {
    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;
    let mut any = false;
    for line in &shaped.lines {
        for glyph in &line.glyphs {
            if glyph_excluded(&glyph.cluster, glyph_byte_range) {
                continue;
            }
            let Some(image) =
                swash_cache.get_image(engine.font_system_mut(), glyph.physical.cache_key)
            else {
                continue;
            };
            if image.placement.width == 0 || image.placement.height == 0 {
                continue;
            }
            let left = (glyph.physical.x + image.placement.left) as f32;
            let top = (glyph.physical.y - image.placement.top) as f32;
            let right = left + image.placement.width as f32;
            let bottom = top + image.placement.height as f32;
            min_x = min_x.min(left);
            min_y = min_y.min(top);
            max_x = max_x.max(right);
            max_y = max_y.max(bottom);
            any = true;
        }
    }
    any.then_some((min_x, min_y, max_x, max_y))
}

fn draw_rounded_background(
    buf: &mut [u8],
    ink_bbox: (f32, f32, f32, f32),
    rgba: [u8; 4],
    padding: f32,
    corner_radius: f32,
    canvas_width: u32,
    canvas_height: u32,
) {
    if rgba[3] == 0 {
        return;
    }
    let (min_x, min_y, max_x, max_y) = ink_bbox;
    let padding = padding.max(0.0);
    let left = min_x - padding;
    let top = min_y - padding;
    let right = max_x + padding;
    let bottom = max_y + padding;
    let radius = corner_radius
        .max(0.0)
        .min((right - left) / 2.0)
        .min((bottom - top) / 2.0);

    let x_start = left.floor().max(0.0) as i64;
    let x_end = right.ceil().min(canvas_width as f32) as i64;
    let y_start = top.floor().max(0.0) as i64;
    let y_end = bottom.ceil().min(canvas_height as f32) as i64;
    for y in y_start..y_end {
        for x in x_start..x_end {
            let px = x as f32 + 0.5;
            let py = y as f32 + 0.5;
            let nearest_x = px.clamp(left + radius, right - radius);
            let nearest_y = py.clamp(top + radius, bottom - radius);
            let dx = px - nearest_x;
            let dy = py - nearest_y;
            if dx * dx + dy * dy <= radius * radius {
                blend_pixel(buf, canvas_width, canvas_height, x, y, rgba);
            }
        }
    }
}

/// Computes the `(origin_x, max_width_px)` box [`TextLayoutEngine::shape`]'s wrapping/alignment
/// should use, given [`TextClip::pos_x`]'s anchor meaning under `align` — see
/// [`crate::timeline::TextAlign`]'s own doc comment for why each variant anchors a different edge
/// of the box instead of always the left edge:
/// - `Auto`/`Left`: the box's left edge sits at `anchor_x`, extending to the canvas's right edge
///   (this clip's exact pre-existing behavior).
/// - `Center`: the box is centered on `anchor_x`, symmetric out to whichever canvas edge is
///   closer — `cosmic-text`'s own per-line centering within that symmetric box then always lands
///   each line's own center exactly on `anchor_x`, regardless of that line's width.
/// - `Right`: the box's right edge sits at `anchor_x`, extending back to the canvas's left edge.
///
/// [`TextClip::pos_x`]: crate::timeline::TextClip::pos_x
/// `align` must already be resolved (never `Start`/`End`) — see [`text_layout::
/// resolve_text_align`], which the one caller ([`draw_text_segment_onto`]) applies before this
/// box computation and before shaping, so both use the same physical edge.
fn text_horizontal_box(
    align: crate::timeline::TextAlign,
    anchor_x: f32,
    canvas_width: f32,
) -> (f32, f32) {
    use crate::timeline::TextAlign;
    match align {
        TextAlign::Auto | TextAlign::Left => (anchor_x, (canvas_width - anchor_x).max(1.0)),
        TextAlign::Center => {
            let half = anchor_x.min(canvas_width - anchor_x).max(0.5);
            (anchor_x - half, half * 2.0)
        }
        TextAlign::Right => (0.0, anchor_x.max(1.0)),
        TextAlign::Start | TextAlign::End => {
            unreachable!("caller resolves Start/End before calling text_horizontal_box")
        }
    }
}

fn shape_text_segment(
    engine: &mut TextLayoutEngine,
    segment: &TextSegment,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<ShapedText, TextLayoutError> {
    let anchor_x = segment.pos_x * canvas_width as f32;
    let y = segment.pos_y * canvas_height as f32;
    let resolved_align =
        text_layout::resolve_text_align(&segment.text, segment.direction, segment.text_align);
    let (x, max_width) = text_horizontal_box(resolved_align, anchor_x, canvas_width as f32);
    engine.try_shape(
        &segment.text,
        segment.font_family.clone(),
        segment.font_style,
        segment.font_weight,
        segment.font_size,
        Some(max_width),
        (x, y),
        segment.direction,
        resolved_align,
    )
}

/// Validates a text segment with the exact shaping configuration used for preview and export.
pub fn validate_text_segment(
    segment: &TextSegment,
    canvas_width: u32,
    canvas_height: u32,
) -> Result<(), TextLayoutError> {
    text_layout::with_shared_engine(|engine, _| {
        shape_text_segment(engine, segment, canvas_width, canvas_height).map(|_| ())
    })
}

fn draw_text_segment_onto(
    buf: &mut [u8],
    segment: &TextSegment,
    canvas_width: u32,
    canvas_height: u32,
) {
    text_layout::with_shared_engine(|engine, swash_cache| {
        let Ok(shaped) = shape_text_segment(engine, segment, canvas_width, canvas_height) else {
            return;
        };
        if let Some(ink_bbox) =
            shaped_ink_bbox(&shaped, segment.glyph_byte_range, engine, swash_cache)
        {
            draw_rounded_background(
                buf,
                ink_bbox,
                segment.background_rgba,
                segment.background_padding,
                segment.background_corner_radius,
                canvas_width,
                canvas_height,
            );
        }
        draw_shaped_text(
            buf,
            &shaped,
            segment.color_rgba,
            segment.glyph_byte_range,
            canvas_width,
            canvas_height,
            engine,
            swash_cache,
        );
    });
}

/// Rasterizes one semantic export segment into a full-canvas transparent RGBA image. Export
/// writes this buffer to a temporary PNG and the native post-pass overlays it, ensuring the
/// exact same font/background implementation as preview.
pub fn render_text_segment_rgba(
    segment: &TextSegment,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; canvas_width as usize * canvas_height as usize * 4];
    draw_text_segment_onto(&mut buf, segment, canvas_width, canvas_height);
    buf
}

/// Renders `clip`'s text into a fully transparent `canvas_width`×`canvas_height` RGBA buffer,
/// positioned at `x=w*pos_x:y=h*pos_y` (top-left of the text block). Export writes the same
/// rasterized buffer to PNG before its native overlay pass. Falls back to an all-transparent
/// buffer if a bundled font is unexpectedly corrupt.
///
/// Wraps at word boundaries once a line would run past the canvas's right edge. `max_width` is
/// the space between the text's own left anchor and the canvas's right edge — `TextLayoutEngine::
/// shape`'s `origin`/`max_width_px` are independent, `max_width_px` alone doesn't already account
/// for a nonzero `x`), floored at `1.0` so a clip anchored at or past the right edge still lays
/// out instead of getting a degenerate zero/negative wrap width.
///
/// `local_time_secs` is elapsed time since `clip.start_secs` (not a timeline position) — when
/// [`TextClip::highlight_enabled`] and it falls within some [`crate::timeline::WordTiming`]'s
/// `[start_secs, end_secs)`, that word's glyph range is redrawn on top in
/// `highlight_color_rgba`. The range is filtered from the complete caption's layout, so explicit
/// newlines and automatic wrapping match the base text and export exactly.
pub fn render_text_clip_rgba(
    clip: &TextClip,
    canvas_width: u32,
    canvas_height: u32,
    local_time_secs: f64,
) -> Vec<u8> {
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
        pos_x: clip.pos_x,
        pos_y: clip.pos_y,
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
        direction: clip.direction,
        text_align: clip.text_align,
    };
    let mut buf = render_text_segment_rgba(&base, canvas_width, canvas_height);

    if let Some(index) = active_highlight_word_index(clip, local_time_secs) {
        let words: Vec<&str> = clip.words.iter().map(|w| w.text.as_str()).collect();
        let byte_range = crate::text_metrics::word_byte_ranges(&clip.text, &words)
            .get(index)
            .copied()
            .flatten()
            .and_then(|[start, end]| Some([u32::try_from(start).ok()?, u32::try_from(end).ok()?]));
        let Some(glyph_byte_range) = byte_range else {
            return buf;
        };
        let highlight = TextSegment {
            color_rgba: clip.highlight_color_rgba,
            background_rgba: [0, 0, 0, 0],
            background_padding: 0.0,
            background_corner_radius: 0.0,
            glyph_byte_range: Some(glyph_byte_range),
            ..base
        };
        draw_text_segment_onto(&mut buf, &highlight, canvas_width, canvas_height);
    }

    buf
}

/// Index of the word whose half-open timing window covers `local_time_secs`, or `None` when
/// highlighting is disabled/between words. Shared by the rasterizer and live preview branch so
/// an unchanged word does not trigger another full-canvas RGBA upload every UI frame.
pub fn active_highlight_word_index(clip: &TextClip, local_time_secs: f64) -> Option<usize> {
    clip.highlight_enabled.then(|| {
        clip.words
            .iter()
            .position(|word| local_time_secs >= word.start_secs && local_time_secs < word.end_secs)
    })?
}

/// `(rx/hw)^2 + (ry/hh)^2 <= 1` — the pure-pixel twin of
/// [`crate::shape_render::ellipse_inside_expr`]'s `geq` expression, `rx`/`ry` already in the
/// shape's own unrotated local frame (pixels relative to center).
fn ellipse_inside(rx: f64, ry: f64, w_px: f64, h_px: f64) -> bool {
    let hw = w_px / 2.0;
    let hh = h_px / 2.0;
    if hw <= 0.0 || hh <= 0.0 {
        return false;
    }
    (rx / hw).powi(2) + (ry / hh).powi(2) <= 1.0
}

fn shape_inside(kind: &ShapeKind, rx: f64, ry: f64, w_px: f64, h_px: f64) -> bool {
    match kind {
        ShapeKind::Ellipse => ellipse_inside(rx, ry, w_px, h_px),
        ShapeKind::Polygon(vertices) => {
            let scaled: Vec<(f64, f64)> = vertices
                .iter()
                .map(|&(x, y)| (x as f64 * w_px, y as f64 * h_px))
                .collect();
            point_in_polygon((rx, ry), &scaled)
        }
    }
}

/// Renders `clip`'s shape into a fully transparent `canvas_width`×`canvas_height` RGBA buffer —
/// the pixel-buffer twin of [`crate::shape_render::build_shape_filter_desc`], evaluated directly
/// instead of compiled into a `geq` expression, and iterated only over the shape's own bounding
/// box (padded for rotation) rather than the whole canvas.
pub fn render_shape_clip_rgba(clip: &ShapeClip, canvas_width: u32, canvas_height: u32) -> Vec<u8> {
    let mut buf = vec![0u8; canvas_width as usize * canvas_height as usize * 4];

    let cx = clip.center_x as f64 * canvas_width as f64;
    let cy = clip.center_y as f64 * canvas_height as f64;
    let w_px = clip.width as f64 * canvas_width as f64;
    let h_px = clip.height as f64 * canvas_height as f64;
    let angle = (clip.rotation_deg as f64).to_radians();
    let (sin_a, cos_a) = angle.sin_cos();

    let inner_w = (w_px - 2.0 * clip.stroke_thickness_px as f64).max(0.0);
    let inner_h = (h_px - 2.0 * clip.stroke_thickness_px as f64).max(0.0);
    let has_stroke = clip.stroke_thickness_px > 0.0;

    // Bounding box padded to the diagonal, since a rotated shape's axis-aligned extent can
    // exceed its own unrotated width/height.
    let half_diag = ((w_px / 2.0).powi(2) + (h_px / 2.0).powi(2)).sqrt().ceil();
    let x0 = (cx - half_diag).floor().max(0.0) as u32;
    let x1 = (cx + half_diag).ceil().min(canvas_width as f64) as u32;
    let y0 = (cy - half_diag).floor().max(0.0) as u32;
    let y1 = (cy + half_diag).ceil().min(canvas_height as f64) as u32;

    let [r, g, b, a] = clip.color_rgba;
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let rx = dx * cos_a + dy * sin_a;
            let ry = -dx * sin_a + dy * cos_a;

            let outer = shape_inside(&clip.shape_kind, rx, ry, w_px, h_px);
            let inside = if has_stroke {
                outer && !shape_inside(&clip.shape_kind, rx, ry, inner_w, inner_h)
            } else {
                outer
            };
            if inside {
                put_pixel(
                    &mut buf,
                    canvas_width,
                    canvas_height,
                    x as i64,
                    y as i64,
                    [r, g, b, a],
                );
            }
        }
    }
    buf
}

/// Renders a [`crate::timeline::ClipInstance::mask_shape`] into a `canvas_width`×`canvas_height`
/// single-channel `GRAY8` buffer — `255` inside the shape (visible), `0` outside (masked out) —
/// for [`crate::preview::build_composite_branch`]'s `alphacombine` mask stage, the preview-side
/// counterpart to export's `geq`-based alpha-clipping stage
/// (`ClipInstance::video_filter_chain`'s `mask_shape` block). Mirrors that block's math
/// term-for-term: [`MaskShape::Circle`] is a plain circle inscribed in `min(width, height)`;
/// [`MaskShape::RoundedRect`] is a rounded-rect signed-distance test, `corner_radius` a
/// `0.0..=1.0` fraction of `min(width, height)`. Static — no keyframes on `mask_shape`, so one
/// buffer rendered once (like [`render_text_clip_rgba`]/[`render_shape_clip_rgba`]) covers the
/// clip's whole visible span. Returns an all-zero (fully masked) buffer for
/// [`MaskShape::None`] — callers gate on [`crate::timeline::ClipInstance::is_masked`]
/// before calling this at all, so that case shouldn't be reached in practice.
pub fn render_mask_shape_gray8(
    mask_shape: MaskShape,
    corner_radius: f32,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<u8> {
    let mut buf = vec![0u8; canvas_width as usize * canvas_height as usize];
    if mask_shape == MaskShape::None {
        return buf;
    }

    let w = canvas_width as f64;
    let h = canvas_height as f64;
    let cx = w / 2.0;
    let cy = h / 2.0;
    let minwh = w.min(h);

    for y in 0..canvas_height {
        for x in 0..canvas_width {
            let dx = x as f64 + 0.5 - cx;
            let dy = y as f64 + 0.5 - cy;
            let inside = match mask_shape {
                MaskShape::None => false,
                MaskShape::Circle => dx * dx + dy * dy <= (minwh / 2.0).powi(2),
                MaskShape::RoundedRect => {
                    let radius = (corner_radius.clamp(0.0, 1.0) as f64 * minwh).min(minwh / 2.0);
                    let outer_x = (dx.abs() - (w / 2.0 - radius)).max(0.0);
                    let outer_y = (dy.abs() - (h / 2.0 - radius)).max(0.0);
                    (outer_x * outer_x + outer_y * outer_y).sqrt() - radius <= 0.0
                }
            };
            if inside {
                let idx = (y * canvas_width + x) as usize;
                buf[idx] = 255;
            }
        }
    }
    buf
}

#[cfg(test)]
#[path = "overlay_render/overlay_render_test.rs"]
mod tests;
