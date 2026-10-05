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

#[test]
fn timed_highlight_selects_every_overlapping_shaped_cluster() {
    let ligature = 2..4;
    assert!(!glyph_excluded(&ligature, Some([3, 4])));
    assert!(!glyph_excluded(&ligature, Some([1, 3])));
    assert!(glyph_excluded(&ligature, Some([4, 5])));
    assert!(glyph_excluded(&ligature, Some([0, 2])));
}
use crate::timeline::{MaskShape, ShapeClip, ShapeKind, TextClip, TextDirection, WordTiming};

fn sample_shape(kind: ShapeKind) -> ShapeClip {
    ShapeClip {
        id: 1,
        start_secs: 0.0,
        duration_secs: 1.0,
        shape_kind: kind,
        center_x: 0.5,
        center_y: 0.5,
        center_x_keyframes: vec![],
        center_y_keyframes: vec![],
        width_keyframes: vec![],
        height_keyframes: vec![],
        rotation_keyframes: vec![],
        width: 0.4,
        height: 0.4,
        rotation_deg: 0.0,
        color_rgba: [255, 0, 0, 255],
        stroke_thickness_px: 0.0,
    }
}

fn pixel(buf: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let idx = (y * width + x) as usize * 4;
    [buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]]
}

#[test]
fn ellipse_center_pixel_is_opaque_and_corners_are_transparent() {
    let clip = sample_shape(ShapeKind::Ellipse);
    let buf = render_shape_clip_rgba(&clip, 200, 100);
    assert_eq!(pixel(&buf, 200, 100, 50), [255, 0, 0, 255]);
    assert_eq!(pixel(&buf, 200, 0, 0), [0, 0, 0, 0]);
    assert_eq!(pixel(&buf, 200, 199, 99), [0, 0, 0, 0]);
}

#[test]
fn rectangle_fills_its_whole_local_square_not_just_the_center() {
    let clip = sample_shape(ShapeKind::rectangle());
    let buf = render_shape_clip_rgba(&clip, 200, 100);
    // Center (100, 50) is inside; a corner just outside the 80x40px rect (width/height 0.4 of
    // 200x100) at (100+45, 50) should be outside.
    assert_eq!(pixel(&buf, 200, 100, 50), [255, 0, 0, 255]);
    assert_eq!(pixel(&buf, 200, 100 + 45, 50), [0, 0, 0, 0]);
}

#[test]
fn stroke_only_shape_leaves_its_own_center_transparent() {
    let mut clip = sample_shape(ShapeKind::Ellipse);
    clip.stroke_thickness_px = 5.0;
    let buf = render_shape_clip_rgba(&clip, 200, 100);
    assert_eq!(pixel(&buf, 200, 100, 50), [0, 0, 0, 0]);
}

fn sample_text(text: &str) -> TextClip {
    TextClip {
        id: 1,
        start_secs: 0.0,
        duration_secs: 1.0,
        text: text.to_string(),
        font_size: 32.0,
        font_family: Default::default(),
        font_style: Default::default(),
        font_weight: None,
        color_rgba: [255, 255, 255, 255],
        background_rgba: [0, 0, 0, 0],
        background_padding: 8.0,
        background_corner_radius: 8.0,
        pos_x: 0.1,
        pos_y: 0.1,
        words: Vec::<WordTiming>::new(),
        highlight_enabled: false,
        highlight_color_rgba: [255, 220, 0, 255],
        opacity_keyframes: vec![],
        pos_x_keyframes: vec![],
        pos_y_keyframes: vec![],
        scale_keyframes: vec![],
        rotation_keyframes: vec![],
        direction: Default::default(),
        language: None,
        text_align: Default::default(),
    }
}

#[test]
fn empty_text_produces_a_fully_transparent_buffer() {
    let clip = sample_text("");
    let buf = render_text_clip_rgba(&clip, 200, 100, 0.0);
    assert!(buf.as_chunks::<4>().0.iter().all(|p| p[3] == 0));
}

#[test]
fn non_empty_text_draws_at_least_one_opaque_pixel() {
    let clip = sample_text("A");
    let buf = render_text_clip_rgba(&clip, 200, 100, 0.0);
    assert!(buf.as_chunks::<4>().0.iter().any(|p| p[3] > 0));
}

fn leftmost_opaque_column(buf: &[u8], width: u32, height: u32) -> Option<u32> {
    for x in 0..width {
        for y in 0..height {
            if pixel(buf, width, x, y)[3] > 0 {
                return Some(x);
            }
        }
    }
    None
}

/// TEXT-01B step 3's own "mixed-direction golden test" gap — a real pixel-level render, not
/// just the shaping-level cluster-position checks `text_layout`'s own tests already covered.
/// Latin + Hebrew mixed content forced `Ltr` should paint its first (Latin) run starting near
/// the left edge; forced `Rtl` should visibly push the block's own ink further right (the
/// paragraph's anchor flips to the block's right edge — see `text_horizontal_box`), a real,
/// observable pixel difference from the actual `overlay_render` pipeline, not an inferred one.
#[test]
fn mixed_direction_text_paints_visibly_different_pixels_under_forced_ltr_vs_rtl() {
    let mut ltr = sample_text("abc \u{5d0}\u{5d1}\u{5d2}");
    ltr.direction = TextDirection::Ltr;
    let mut rtl = ltr.clone();
    rtl.direction = TextDirection::Rtl;

    let width = 400;
    let height = 120;
    let ltr_buf = render_text_clip_rgba(&ltr, width, height, 0.0);
    let rtl_buf = render_text_clip_rgba(&rtl, width, height, 0.0);

    assert_ne!(
        ltr_buf, rtl_buf,
        "forcing direction must change the rendered pixels"
    );
    let ltr_left =
        leftmost_opaque_column(&ltr_buf, width, height).expect("ltr render draws something");
    let rtl_left =
        leftmost_opaque_column(&rtl_buf, width, height).expect("rtl render draws something");
    assert!(
        rtl_left > ltr_left,
        "forcing Rtl should anchor the block further right than Ltr: ltr_left={ltr_left}, rtl_left={rtl_left}"
    );
}

#[test]
fn selected_font_family_changes_the_rasterized_pixels() {
    let lato = sample_text("Paco Paçoca");
    let mut display = lato.clone();
    display.font_family = crate::timeline::TextFontFamily::BebasNeue;

    assert_ne!(
        render_text_clip_rgba(&lato, 400, 120, 0.0),
        render_text_clip_rgba(&display, 400, 120, 0.0)
    );
}

#[test]
fn rounded_background_has_transparent_corners_and_opaque_edges() {
    let mut clip = sample_text("WIDE");
    clip.color_rgba = [255, 255, 255, 0];
    clip.background_rgba = [10, 20, 30, 255];
    clip.background_padding = 12.0;
    clip.background_corner_radius = 10.0;
    let width = 300;
    let height = 120;
    let buf = render_text_clip_rgba(&clip, width, height, 0.0);

    let opaque: Vec<(u32, u32)> = (0..height)
        .flat_map(|y| (0..width).map(move |x| (x, y)))
        .filter(|&(x, y)| pixel(&buf, width, x, y)[3] > 0)
        .collect();
    let min_x = opaque.iter().map(|&(x, _)| x).min().unwrap();
    let max_x = opaque.iter().map(|&(x, _)| x).max().unwrap();
    let min_y = opaque.iter().map(|&(_, y)| y).min().unwrap();
    let max_y = opaque.iter().map(|&(_, y)| y).max().unwrap();

    assert_eq!(pixel(&buf, width, min_x, min_y)[3], 0);
    assert!(pixel(&buf, width, (min_x + max_x) / 2, min_y)[3] > 0);
    assert!(pixel(&buf, width, min_x, (min_y + max_y) / 2)[3] > 0);
}

fn contains_pixel(buf: &[u8], rgba: [u8; 4]) -> bool {
    buf.as_chunks::<4>().0.iter()
        .any(|p| p[0] == rgba[0] && p[1] == rgba[1] && p[2] == rgba[2] && p[3] == rgba[3])
}

fn sample_text_with_words(text: &str, words: Vec<WordTiming>) -> TextClip {
    let mut clip = sample_text(text);
    clip.words = words;
    clip.highlight_enabled = true;
    clip
}

#[test]
fn the_word_covering_local_time_is_drawn_in_the_highlight_color() {
    let clip = sample_text_with_words(
        "aa bb",
        vec![
            WordTiming {
                text: "aa".to_string(),
                start_secs: 0.0,
                end_secs: 0.5,
            },
            WordTiming {
                text: "bb".to_string(),
                start_secs: 0.5,
                end_secs: 1.0,
            },
        ],
    );
    // 0.7s falls within "bb"'s [0.5, 1.0) window.
    let buf = render_text_clip_rgba(&clip, 300, 100, 0.7);
    assert!(contains_pixel(&buf, clip.highlight_color_rgba));
}

#[test]
fn a_local_time_covered_by_no_word_leaves_only_the_base_color() {
    let clip = sample_text_with_words(
        "aa bb",
        vec![
            WordTiming {
                text: "aa".to_string(),
                start_secs: 0.0,
                end_secs: 0.5,
            },
            WordTiming {
                text: "bb".to_string(),
                start_secs: 0.5,
                end_secs: 1.0,
            },
        ],
    );
    // Past every word's own end — no word is "current" here, so nothing should be highlighted.
    let buf = render_text_clip_rgba(&clip, 300, 100, 5.0);
    assert!(!contains_pixel(&buf, clip.highlight_color_rgba));
    assert!(contains_pixel(&buf, clip.color_rgba));
}

#[test]
fn highlight_disabled_never_draws_the_highlight_color_even_within_a_words_window() {
    let mut clip = sample_text_with_words(
        "aa bb",
        vec![WordTiming {
            text: "aa".to_string(),
            start_secs: 0.0,
            end_secs: 0.5,
        }],
    );
    clip.highlight_enabled = false;
    let buf = render_text_clip_rgba(&clip, 300, 100, 0.2);
    assert!(!contains_pixel(&buf, clip.highlight_color_rgba));
}

#[test]
fn a_highlighted_word_follows_the_base_layout_onto_the_next_line() {
    let mut clip = sample_text_with_words(
        "WWWW WWWW",
        vec![
            WordTiming {
                text: "WWWW".to_string(),
                start_secs: 0.0,
                end_secs: 0.5,
            },
            WordTiming {
                text: "WWWW".to_string(),
                start_secs: 0.5,
                end_secs: 1.0,
            },
        ],
    );
    clip.pos_x = 0.0;
    clip.pos_y = 0.0;
    clip.font_size = 32.0;
    clip.color_rgba = [255, 255, 255, 0];
    let width = 110;
    let height = 140;

    let first = render_text_clip_rgba(&clip, width, height, 0.2);
    let second = render_text_clip_rgba(&clip, width, height, 0.7);
    let (first_min_y, _) = opaque_y_extent(&first, width, height);
    let (second_min_y, _) = opaque_y_extent(&second, width, height);

    assert!(
        second_min_y > first_min_y,
        "expected the wrapped second word below the first ({second_min_y} <= {first_min_y})"
    );
}

/// The vertical extent (min/max y) covering every opaque pixel in an RGBA buffer — a one-line
/// render stays within roughly one line height, a wrapped multi-line render spans much more.
fn opaque_y_extent(buf: &[u8], width: u32, height: u32) -> (u32, u32) {
    let mut min_y = height;
    let mut max_y = 0;
    for y in 0..height {
        for x in 0..width {
            if pixel(buf, width, x, y)[3] > 0 {
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    (min_y, max_y)
}

#[test]
fn long_text_wraps_onto_multiple_lines_once_narrower_than_the_canvas() {
    let mut clip = sample_text("wwww wwww wwww wwww");
    clip.pos_x = 0.0;
    clip.font_size = 20.0;

    let (wide_min, wide_max) =
        opaque_y_extent(&render_text_clip_rgba(&clip, 800, 300, 0.0), 800, 300);
    let (narrow_min, narrow_max) =
        opaque_y_extent(&render_text_clip_rgba(&clip, 100, 300, 0.0), 100, 300);

    // A canvas wide enough for the whole string renders on one line — under a generous single
    // line-height bound (comfortably more than the font size, well short of two stacked lines).
    assert!(
        wide_max - wide_min < clip.font_size as u32 * 3 / 2,
        "expected a single line on a wide canvas, got a {}px vertical span",
        wide_max - wide_min
    );
    // The same text on a canvas too narrow for one word per line wraps across multiple lines,
    // spanning noticeably more vertical space than the unwrapped render above.
    assert!(
        narrow_max - narrow_min > wide_max - wide_min,
        "expected wrapping on a narrow canvas to span more vertical space than the unwrapped \
         render ({}px vs {}px)",
        narrow_max - narrow_min,
        wide_max - wide_min
    );
}

fn gray_pixel(buf: &[u8], width: u32, x: u32, y: u32) -> u8 {
    buf[(y * width + x) as usize]
}

#[test]
fn none_mask_shape_is_fully_masked_out() {
    let buf = render_mask_shape_gray8(MaskShape::None, 0.0, 100, 100);
    assert!(buf.iter().all(|&b| b == 0));
}

#[test]
fn circle_mask_center_is_visible_and_corners_are_masked() {
    let buf = render_mask_shape_gray8(MaskShape::Circle, 0.0, 100, 100);
    assert_eq!(gray_pixel(&buf, 100, 50, 50), 255);
    assert_eq!(gray_pixel(&buf, 100, 0, 0), 0);
}

#[test]
fn rounded_rect_mask_with_zero_radius_covers_almost_the_whole_frame() {
    let buf = render_mask_shape_gray8(MaskShape::RoundedRect, 0.0, 100, 100);
    assert_eq!(gray_pixel(&buf, 100, 50, 50), 255);
    assert_eq!(gray_pixel(&buf, 100, 1, 1), 255);
}

#[test]
fn rounded_rect_mask_with_full_radius_masks_out_the_corner() {
    let buf = render_mask_shape_gray8(MaskShape::RoundedRect, 1.0, 100, 100);
    assert_eq!(gray_pixel(&buf, 100, 50, 50), 255);
    assert_eq!(gray_pixel(&buf, 100, 0, 0), 0);
}

#[test]
fn text_horizontal_box_auto_and_left_anchor_the_left_edge_at_the_pos_x_pixel() {
    // Auto and Left must be pixel-identical to this clip's exact pre-existing behavior: the box
    // starts at the anchor and extends to the canvas's right edge.
    for align in [
        crate::timeline::TextAlign::Auto,
        crate::timeline::TextAlign::Left,
    ] {
        let (origin_x, max_width) = text_horizontal_box(align, 40.0, 200.0);
        assert_eq!(origin_x, 40.0);
        assert_eq!(max_width, 160.0);
    }
}

#[test]
fn text_horizontal_box_center_is_symmetric_around_the_anchor() {
    let (origin_x, max_width) =
        text_horizontal_box(crate::timeline::TextAlign::Center, 50.0, 200.0);
    // The box's own center (origin_x + max_width / 2) must land exactly on the anchor.
    assert!((origin_x + max_width / 2.0 - 50.0).abs() < 0.001);
    // Symmetric around a near-left anchor means the box is bounded by the closer edge (the
    // anchor itself, 50px from the left) on one side and mirrors that same 50px on the other.
    assert_eq!(origin_x, 0.0);
    assert_eq!(max_width, 100.0);
}

#[test]
fn text_horizontal_box_center_stays_symmetric_when_the_anchor_is_past_the_midpoint() {
    let (origin_x, max_width) =
        text_horizontal_box(crate::timeline::TextAlign::Center, 150.0, 200.0);
    assert!((origin_x + max_width / 2.0 - 150.0).abs() < 0.001);
    // Now the canvas's right edge (50px from the anchor) is the closer bound.
    assert_eq!(origin_x, 100.0);
    assert_eq!(max_width, 100.0);
}

#[test]
fn text_horizontal_box_right_anchors_the_right_edge_at_the_pos_x_pixel() {
    let (origin_x, max_width) =
        text_horizontal_box(crate::timeline::TextAlign::Right, 150.0, 200.0);
    assert_eq!(origin_x, 0.0);
    assert_eq!(max_width, 150.0);
    assert!((origin_x + max_width - 150.0).abs() < 0.001);
}

#[test]
fn text_horizontal_box_never_produces_a_non_positive_width() {
    // An anchor pinned exactly at a canvas edge (or past it) must still produce a box a caller
    // can hand to shape()'s max_width_px without it collapsing to zero/negative.
    for align in [
        crate::timeline::TextAlign::Auto,
        crate::timeline::TextAlign::Left,
        crate::timeline::TextAlign::Center,
        crate::timeline::TextAlign::Right,
    ] {
        for anchor in [0.0, 200.0, -10.0, 250.0] {
            let (_, max_width) = text_horizontal_box(align, anchor, 200.0);
            assert!(
                max_width > 0.0,
                "align={align:?} anchor={anchor} produced max_width={max_width}"
            );
        }
    }
}

#[test]
fn center_alignment_lands_the_lines_own_center_on_the_pos_x_anchor() {
    // Real end-to-end proof, not just the box-geometry math above: cosmic-text's own per-line
    // centering within a symmetric box does land each line's center exactly on the anchor pixel.
    let mut engine = crate::text_layout::TextLayoutEngine::new_from_locked_catalog();
    let (origin_x, max_width) =
        text_horizontal_box(crate::timeline::TextAlign::Center, 100.0, 200.0);
    let shaped = engine.shape(
        "Hi",
        crate::timeline::TextFontFamily::Lato,
        crate::timeline::TextFontStyle::Regular,
        None,
        32.0,
        Some(max_width),
        (origin_x, 0.0),
        crate::timeline::TextDirection::Auto,
        crate::timeline::TextAlign::Center,
    );
    let line = &shaped.lines[0];
    let min_x = line
        .glyphs
        .iter()
        .map(|g| g.x)
        .fold(f32::INFINITY, f32::min);
    let max_x = line
        .glyphs
        .iter()
        .map(|g| g.x + g.w)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!(((min_x + max_x) / 2.0 - 100.0).abs() < 0.5);
}

#[test]
fn right_alignment_lands_the_lines_own_right_edge_on_the_pos_x_anchor() {
    let mut engine = crate::text_layout::TextLayoutEngine::new_from_locked_catalog();
    let (origin_x, max_width) =
        text_horizontal_box(crate::timeline::TextAlign::Right, 150.0, 200.0);
    let shaped = engine.shape(
        "Hi",
        crate::timeline::TextFontFamily::Lato,
        crate::timeline::TextFontStyle::Regular,
        None,
        32.0,
        Some(max_width),
        (origin_x, 0.0),
        crate::timeline::TextDirection::Auto,
        crate::timeline::TextAlign::Right,
    );
    let line = &shaped.lines[0];
    let max_x = line
        .glyphs
        .iter()
        .map(|g| g.x + g.w)
        .fold(f32::NEG_INFINITY, f32::max);
    assert!((max_x - 150.0).abs() < 0.5);
}
