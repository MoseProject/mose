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
use crate::timeline::ShapeKind;

fn rectangle_vertices() -> Vec<(f64, f64)> {
    vec![(-50.0, -25.0), (50.0, -25.0), (50.0, 25.0), (-50.0, 25.0)]
}

#[test]
fn point_in_polygon_center_of_rectangle_is_inside() {
    assert!(point_in_polygon((0.0, 0.0), &rectangle_vertices()));
}

#[test]
fn point_in_polygon_outside_rectangle_bounds_is_outside() {
    assert!(!point_in_polygon((100.0, 0.0), &rectangle_vertices()));
    assert!(!point_in_polygon((0.0, 100.0), &rectangle_vertices()));
    assert!(!point_in_polygon((-100.0, -100.0), &rectangle_vertices()));
}

#[test]
fn point_in_polygon_just_inside_and_just_outside_an_edge() {
    let v = rectangle_vertices();
    assert!(point_in_polygon((49.0, 0.0), &v));
    assert!(!point_in_polygon((51.0, 0.0), &v));
}

#[test]
fn point_in_polygon_triangle_apex_and_outside_corner() {
    // Apex-up triangle spanning -50..50 in x, -50..50 in y (matches ShapeKind::triangle's
    // vertex layout scaled to pixels).
    let v = vec![(0.0, -50.0), (50.0, 50.0), (-50.0, 50.0)];
    assert!(point_in_polygon((0.0, 0.0), &v)); // centroid-ish, inside
    assert!(!point_in_polygon((-49.0, -49.0), &v)); // near apex's far corner, outside the slope
    assert!(!point_in_polygon((49.0, -49.0), &v));
}

#[test]
fn point_in_polygon_handles_a_concave_arrow_shape() {
    // Same layout as ShapeKind::arrow(), scaled to a 100x100 box (-50..50 each axis).
    let v = vec![
        (-50.0, -15.0),
        (10.0, -15.0),
        (10.0, -35.0),
        (50.0, 0.0),
        (10.0, 35.0),
        (10.0, 15.0),
        (-50.0, 15.0),
    ];
    // Inside the shaft.
    assert!(point_in_polygon((0.0, 0.0), &v));
    // Inside the arrowhead, near the tip.
    assert!(point_in_polygon((40.0, 0.0), &v));
    // In the concave notch above the shaft but below the head's upper point — outside the
    // shape (this is exactly the case a convex-only half-plane test would get wrong).
    assert!(!point_in_polygon((5.0, -25.0), &v));
    // Past the tip entirely.
    assert!(!point_in_polygon((60.0, 0.0), &v));
}

#[test]
fn shape_kind_presets_are_closed_polygons_with_expected_vertex_counts() {
    let ShapeKind::Polygon(rect) = ShapeKind::rectangle() else {
        panic!("expected Polygon")
    };
    assert_eq!(rect.len(), 4);
    let ShapeKind::Polygon(tri) = ShapeKind::triangle() else {
        panic!("expected Polygon")
    };
    assert_eq!(tri.len(), 3);
    let ShapeKind::Polygon(trap) = ShapeKind::trapezoid() else {
        panic!("expected Polygon")
    };
    assert_eq!(trap.len(), 4);
    let ShapeKind::Polygon(arrow) = ShapeKind::arrow() else {
        panic!("expected Polygon")
    };
    assert_eq!(arrow.len(), 7);
}

#[test]
fn shape_kind_square_is_the_same_as_rectangle() {
    assert_eq!(ShapeKind::square(), ShapeKind::rectangle());
}

fn fixture_input(shape_kind: &ShapeKind) -> ShapeRenderInput<'_> {
    ShapeRenderInput {
        shape_kind,
        center_x: 0.5,
        center_y: 0.5,
        center_x_keyframes: &[],
        center_y_keyframes: &[],
        width: 0.2,
        height: 0.2,
        width_keyframes: &[],
        height_keyframes: &[],
        rotation_deg: 0.0,
        rotation_keyframes: &[],
        color_rgba: [255, 0, 0, 255],
        stroke_thickness_px: 0.0,
        start_secs: 1.0,
        duration_secs: 2.0,
        canvas_width: 1920,
        canvas_height: 1080,
    }
}

#[test]
fn rgb_to_ycbcr_white_is_full_luma_neutral_chroma() {
    let (y, cb, cr) = rgb_to_ycbcr([255, 255, 255, 255]);
    assert!((y - 255.0).abs() < 0.5);
    assert!((cb - 128.0).abs() < 0.5);
    assert!((cr - 128.0).abs() < 0.5);
}

#[test]
fn rgb_to_ycbcr_black_is_zero_luma_neutral_chroma() {
    let (y, cb, cr) = rgb_to_ycbcr([0, 0, 0, 255]);
    assert!(y.abs() < 0.5);
    assert!((cb - 128.0).abs() < 0.5);
    assert!((cr - 128.0).abs() < 0.5);
}

#[test]
fn rgb_to_ycbcr_pure_red_has_high_cr_low_cb() {
    let (_y, cb, cr) = rgb_to_ycbcr([255, 0, 0, 255]);
    assert!(cb < 128.0);
    assert!(cr > 128.0);
}

#[test]
fn build_shape_filter_desc_starts_with_geq_and_has_all_channels() {
    let kind = ShapeKind::rectangle();
    let desc = build_shape_filter_desc(&fixture_input(&kind));
    assert!(desc.starts_with("geq="));
    assert!(desc.contains("lum="));
    assert!(desc.contains("cb="));
    assert!(desc.contains("cr="));
    assert!(desc.contains("enable="));
}

#[test]
fn build_shape_filter_desc_bakes_in_the_visible_time_window() {
    let kind = ShapeKind::rectangle();
    let desc = build_shape_filter_desc(&fixture_input(&kind));
    assert!(desc.contains("between(t\\,1.0000\\,3.0000)"));
}

#[test]
fn build_shape_filter_desc_animates_position_when_center_keyframes_are_present() {
    let kind = ShapeKind::rectangle();
    let mut input = fixture_input(&kind);
    let center_x_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.2,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.8,
        },
    ];
    input.center_x_keyframes = &center_x_keyframes;
    let desc = build_shape_filter_desc(&input);
    // fixture_input's start_secs is 1.0 -- the T offset this shape's keyframes are built around.
    assert!(desc.contains("(T-1.000000)"));
}

#[test]
fn build_shape_filter_desc_animates_size_when_width_or_height_keyframes_are_present() {
    let kind = ShapeKind::rectangle();
    let mut input = fixture_input(&kind);
    let width_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.1,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.5,
        },
    ];
    input.width_keyframes = &width_keyframes;
    let desc = build_shape_filter_desc(&input);
    assert!(desc.contains("(T-1.000000)"));
}

#[test]
fn build_shape_filter_desc_animates_rotation_via_geq_sin_cos_when_rotation_keyframes_are_present() {
    let kind = ShapeKind::rectangle();
    let mut input = fixture_input(&kind);
    let rotation_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 90.0,
        },
    ];
    input.rotation_keyframes = &rotation_keyframes;
    let desc = build_shape_filter_desc(&input);
    assert!(desc.contains("sin("));
    assert!(desc.contains("cos("));
    assert!(desc.contains("PI"));
    assert!(desc.contains("(T-1.000000)"));
}

#[test]
fn build_shape_filter_desc_unkeyframed_rotation_still_uses_geq_sin_cos() {
    // Not a regression in the unkeyframed case's *output* (the angle is still a plain constant
    // fed into sin()/cos()), but confirms the always-embed-sin/cos design doesn't silently skip
    // emitting them when there's nothing to animate.
    let kind = ShapeKind::rectangle();
    let input = fixture_input(&kind);
    let desc = build_shape_filter_desc(&input);
    assert!(desc.contains("sin("));
    assert!(desc.contains("cos("));
}

#[test]
fn ellipse_inside_expr_keyframed_width_produces_a_valid_multiply_through_test() {
    // The keyframed case's w_expr/h_expr are themselves parenthesized sub-expressions rather
    // than plain numbers -- confirm the multiply-through-avoid-division-by-a-possibly-zero-
    // half-extent test still assembles into one well-formed lte(...) embedding the animated
    // sub-expression verbatim. The only division in the result is `/2` to get a half-extent
    // from the full width/height -- a fixed literal divisor, never the possibly-zero half-
    // extent itself, so it's not the division this test's name refers to.
    let kind = ShapeKind::Ellipse;
    let animated_w = "if(lt((T-0),0.5),10,20)";
    let expr = inside_expr(&kind, "RX", "RY", animated_w, "100.0");
    assert!(expr.starts_with("lte("));
    assert!(expr.contains(animated_w));
}

#[test]
fn polygon_inside_expr_has_one_crossing_term_per_edge() {
    let ShapeKind::Polygon(vertices) = ShapeKind::triangle() else {
        panic!("expected Polygon")
    };
    let expr = polygon_inside_expr("RX", "RY", &vertices, "100.0", "100.0");
    assert_eq!(expr.matches("abs(gt(").count(), 3);
    assert!(expr.starts_with("mod("));
}

#[test]
fn build_shape_filter_desc_ellipse_uses_lte_not_mod() {
    let kind = ShapeKind::Ellipse;
    let desc = build_shape_filter_desc(&fixture_input(&kind));
    assert!(desc.contains("lte("));
    assert!(!desc.contains("mod("));
}

#[test]
fn inside_expr_outline_wraps_outer_and_inner_tests() {
    let kind = ShapeKind::Ellipse;
    let outer = inside_expr(&kind, "RX", "RY", "100.0", "100.0");
    let inner = inside_expr(&kind, "RX", "RY", "92.0", "92.0");
    let wrapped = format!("(({outer})*(1-({inner})))");
    // Two lte(...) tests present: the outer boundary and the shrunk inner boundary.
    assert_eq!(wrapped.matches("lte(").count(), 2);
}
