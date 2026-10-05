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

//! Builds the `geq` avfilter node description for one [`crate::timeline::ShapeClip`] — the
//! Rust-side half of the "Rust builds the filter string, C just chains it" split
//! `avbridge::apply_shape_overlays` uses (see that C function's doc comment), same shape as
//! `ClipInstance::video_filter_chain` for the main export path.
//!
//! **Why `geq` and not a native avfilter primitive:** `drawbox` draws an axis-aligned
//! rectangle but has no rotation option, and no native avfilter draws an arbitrary polygon or
//! an ellipse at all. Every shape is instead tested per-pixel: rotate the pixel into the
//! shape's own local (unrotated) frame, then run an in/out test there — a quadratic test for
//! [`ShapeKind::Ellipse`], or a ray-casting point-in-polygon test (handles concave outlines
//! like the arrow preset, not just convex ones) for [`ShapeKind::Polygon`].
//!
//! **Verification:** the generated `geq` expression was rendered for real — `ffmpeg -f lavfi
//! -i color=... -vf "<generated filter>" -frames:v 1 out.png` — and visually confirmed to
//! produce a correctly positioned, rotated, and colored shape, including the arrow preset's
//! concave notches (which a convex-only in/out test would get wrong). That check also caught
//! two real bugs before they shipped: (1) an early version set `lum`/`cb`/`cr` directly to
//! `r`/`g`/`b`, which isn't how YCbCr works — fixed by [`rgb_to_ycbcr`]; (2) running the geq
//! chain directly against a `yuv420p` frame corrupted color in a way that looked like a
//! geometry bug (a stray block in the frame corner) but was actually `cb`/`cr` being
//! chroma-subsampled to half the luma plane's resolution, so the same `(X,Y)` pixel offsets
//! landed in the wrong place on those planes — fixed by wrapping the chain in
//! `format=yuv444p,...,format=yuv420p` in `avbridge::apply_shape_overlays`. This dev machine's
//! FFmpeg build can't open an *encoder* right now (see `CLAUDE.md`'s GPU encode note), so the
//! full decode-filter-**encode** pipeline inside `apply_shape_overlays` itself is still
//! encode-unverified here — same caveat class as the rest of this codebase's export path — but
//! the filter graph itself (parsing, configuring, and evaluating per-pixel against real frames)
//! is confirmed correct, not just assumed. The underlying geometry (point-in-polygon, ellipse
//! test) is additionally unit tested as pure Rust math, which the generated expression string
//! is built to mirror term-for-term.

use crate::keyframe::{self, Keyframe};
use crate::timeline::ShapeKind;

/// Everything [`build_shape_filter_desc`] needs about one shape instance, decoupled from
/// [`crate::timeline::ShapeClip`] so this module doesn't need to depend on the whole timeline
/// type graph (and stays easy to unit test with hand-built fixtures).
pub struct ShapeRenderInput<'a> {
    pub shape_kind: &'a ShapeKind,
    /// Center, as a `0.0..=1.0` fraction of canvas width/height.
    pub center_x: f32,
    pub center_y: f32,
    /// General keyframe animation for this shape's center position over its own on-timeline
    /// duration, per the keyframe-expansion gap found while surveying what else the existing
    /// keyframe system could drive (`spec/ROADMAP.md` P4 item 34) — independently overriding
    /// `center_x`/`center_y` above when non-empty, same "keyframes win when present" relationship
    /// every other keyframe field in this codebase already has with its own constant. Built into
    /// a `T`-keyed expression via [`keyframe::shape_axis_expr`] — see that function's doc comment
    /// for why `T` is offset by this shape's `start_secs` here, unlike every other keyframe
    /// expression builder in this codebase.
    pub center_x_keyframes: &'a [Keyframe<f32>],
    pub center_y_keyframes: &'a [Keyframe<f32>],
    /// Size, as a `0.0..=1.0` fraction of canvas width/height.
    pub width: f32,
    pub height: f32,
    /// General keyframe animation for width/height, same "keyframes win when present"
    /// relationship as `center_x_keyframes`/`center_y_keyframes` above. Built the same way, via
    /// [`keyframe::shape_axis_expr`], and multiplied up to a pixel expression the same way
    /// `center_x`/`center_y` already are — `inside_expr`'s geometry math accepts these as
    /// `geq`-expression-language sub-expressions rather than literal half-extents, so an
    /// unkeyframed width/height still degenerates to the same plain numeric literal as before.
    pub width_keyframes: &'a [Keyframe<f32>],
    pub height_keyframes: &'a [Keyframe<f32>],
    /// Clockwise rotation around the shape's own center, in degrees.
    pub rotation_deg: f32,
    /// General keyframe animation for `rotation_deg`. Unlike width/height, this doesn't touch
    /// `inside_expr` — only how `rx`/`ry` are built below, swapping the Rust-precomputed
    /// `sin_cos` literals for `geq`'s own `sin(...)`/`cos(...)`/`PI` expression-language
    /// functions evaluated per pixel.
    pub rotation_keyframes: &'a [Keyframe<f32>],
    pub color_rgba: [u8; 4],
    /// `0.0` = filled; `> 0.0` = outline only, that many pixels thick.
    pub stroke_thickness_px: f32,
    pub start_secs: f64,
    pub duration_secs: f64,
    pub canvas_width: u32,
    pub canvas_height: u32,
}

/// Small epsilon added to a ray-casting edge's `y2 - y1` denominator so an exactly-horizontal
/// edge (common in every straight-edged preset — a rectangle's top/bottom, a trapezoid's
/// parallel sides) never divides by exactly zero. The edge's crossing term is already forced to
/// zero by the `y1`/`y2`-straddle test in that case regardless of this term's value, so the
/// epsilon doesn't perturb which pixels count as inside — it only keeps the expression from
/// evaluating a `0/0`.
const RAY_EPSILON: f64 = 1e-6;

/// Builds the complete `geq=lum=...:cb=...:cr=...:enable=...` filter node description for one
/// shape instance — ready to hand across the FFI boundary as [`avbridge::ShapeSegment`]'s
/// `filter_desc`.
pub fn build_shape_filter_desc(input: &ShapeRenderInput) -> String {
    // `keyframe::shape_axis_expr` returns either a plain constant ("0.1234", no keyframes) or a
    // `T`-keyed piecewise expression ("if(lt((T-...` -- either way, a fraction of canvas width/
    // height, same unit `ShapeClip::center_x`/`center_y` already use, so it's multiplied up to
    // pixels here exactly like the un-keyframed `input.center_x`/`center_y` used to be.
    let cx_frac = keyframe::shape_axis_expr(
        input.center_x_keyframes,
        input.center_x,
        input.start_secs,
        input.duration_secs,
    );
    let cy_frac = keyframe::shape_axis_expr(
        input.center_y_keyframes,
        input.center_y,
        input.start_secs,
        input.duration_secs,
    );
    let cx = format!("(({cx_frac})*{})", input.canvas_width);
    let cy = format!("(({cy_frac})*{})", input.canvas_height);

    // Same "plain literal when unkeyframed, T-keyed piecewise expression otherwise" shape as
    // cx_frac/cy_frac above.
    let w_frac = keyframe::shape_axis_expr(
        input.width_keyframes,
        input.width,
        input.start_secs,
        input.duration_secs,
    );
    let h_frac = keyframe::shape_axis_expr(
        input.height_keyframes,
        input.height,
        input.start_secs,
        input.duration_secs,
    );
    let w_expr = format!("(({w_frac})*{})", input.canvas_width);
    let h_expr = format!("(({h_frac})*{})", input.canvas_height);

    let angle_deg_expr = keyframe::shape_axis_expr(
        input.rotation_keyframes,
        input.rotation_deg,
        input.start_secs,
        input.duration_secs,
    );
    // FFmpeg's expression evaluator natively provides PI, sin() and cos() (confirmed against
    // its own eval.c-backed docs, not assumed) -- an unkeyframed angle still evaluates to the
    // same numeric sin/cos this used to precompute in Rust, just per-pixel instead of once.
    let angle_rad_expr = format!("(({angle_deg_expr})*PI/180)");
    let sin_expr = format!("sin({angle_rad_expr})");
    let cos_expr = format!("cos({angle_rad_expr})");

    // Pixel coordinate (X,Y), rotated by -angle around the shape's center — so RX/RY are the
    // pixel's position in the shape's own unrotated local frame, in pixels relative to center.
    let rx = format!("((X-{cx})*{cos_expr}+(Y-{cy})*{sin_expr})");
    let ry = format!("(-(X-{cx})*{sin_expr}+(Y-{cy})*{cos_expr})");

    let outer = inside_expr(input.shape_kind, &rx, &ry, &w_expr, &h_expr);
    let inside = if input.stroke_thickness_px > 0.0 {
        // Approximate an outline by shrinking the shape toward its own center by the stroke
        // thickness on each axis — exact for the ellipse's quadratic test, an approximation for
        // polygons (a true inward offset of a concave outline, e.g. the arrow, is a real
        // computational-geometry operation this doesn't attempt). `max(...,0)` (FFmpeg's own
        // expression-language function, confirmed alongside sin/cos/PI above) keeps a keyframed
        // width/height that dips below the stroke thickness from going negative, same as the
        // Rust-side `.max(0.0)` this replaces did for the always-constant case.
        let shrink = 2.0 * input.stroke_thickness_px as f64;
        let inner_w = format!("max((({w_expr})-{shrink})\\,0)");
        let inner_h = format!("max((({h_expr})-{shrink})\\,0)");
        let inner = inside_expr(input.shape_kind, &rx, &ry, &inner_w, &inner_h);
        format!("(({outer})*(1-({inner})))")
    } else {
        format!("({outer})")
    };

    let alpha = input.color_rgba[3] as f64 / 255.0;
    let end_secs = input.start_secs + input.duration_secs;
    let start = input.start_secs;
    let (y, cb, cr) = rgb_to_ycbcr(input.color_rgba);

    format!(
        "geq=\
         lum='lum(X\\,Y)*(1-{inside}*{alpha:.4})+{y:.2}*{inside}*{alpha:.4}':\
         cb='cb(X\\,Y)*(1-{inside}*{alpha:.4})+{cb:.2}*{inside}*{alpha:.4}':\
         cr='cr(X\\,Y)*(1-{inside}*{alpha:.4})+{cr:.2}*{inside}*{alpha:.4}':\
         enable='between(t\\,{start:.4}\\,{end_secs:.4})'"
    )
}

/// Converts an `[r, g, b, a]` (`0..=255` each) color to full-range BT.601 `(Y, Cb, Cr)`
/// (`Y` `0.0..=255.0`, `Cb`/`Cr` `0.0..=255.0` centered on `128.0`) — the geq filter's
/// `lum`/`cb`/`cr` channels are luma/chroma, not RGB, so a fill color has to convert before it
/// can be baked into the expression (setting `lum`/`cb`/`cr` directly to `r`/`g`/`b`, which an
/// earlier version of this function did, silently renders the wrong color — confirmed visually:
/// a `[255,0,0]` red rendered as near-white).
fn rgb_to_ycbcr([r, g, b, _]: [u8; 4]) -> (f64, f64, f64) {
    let (r, g, b) = (r as f64, g as f64, b as f64);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    let cb = -0.168736 * r - 0.331264 * g + 0.5 * b + 128.0;
    let cr = 0.5 * r - 0.418688 * g - 0.081312 * b + 128.0;
    (y, cb, cr)
}

/// Builds the `geq`-expression-language 0/1 "is this pixel inside the shape" test, given
/// already-rotated local-frame pixel coordinate expressions `rx`/`ry` and the shape's own
/// (possibly shrunk, for an outline's inner boundary; possibly keyframed) pixel width/height
/// as `geq`-expression-language sub-expressions — a plain numeric literal like `"192.0000"`
/// when unkeyframed, same as every other unkeyframed axis in this module.
fn inside_expr(kind: &ShapeKind, rx: &str, ry: &str, w_expr: &str, h_expr: &str) -> String {
    match kind {
        ShapeKind::Ellipse => ellipse_inside_expr(rx, ry, w_expr, h_expr),
        ShapeKind::Polygon(vertices) => polygon_inside_expr(rx, ry, vertices, w_expr, h_expr),
    }
}

/// `(rx/hw)^2 + (ry/hh)^2 <= 1`, multiplied through by `(hw*hh)^2` to avoid dividing by a
/// possibly-zero half-extent. `hw`/`hh` are half-width/half-height sub-expressions (`w_expr`/
/// `h_expr` are the full-extent expressions) — the same multiply-through trick as before,
/// generalized from literal `f64` arithmetic to expression-string arithmetic, so it still holds
/// even once `w_expr`/`h_expr` are themselves `T`-keyed piecewise expressions rather than plain
/// numbers.
fn ellipse_inside_expr(rx: &str, ry: &str, w_expr: &str, h_expr: &str) -> String {
    let hw = format!("(({w_expr})/2)");
    let hh = format!("(({h_expr})/2)");
    format!("lte({rx}*{rx}*{hh}*{hh}+{ry}*{ry}*{hw}*{hw}\\,{hw}*{hw}*{hh}*{hh})")
}

/// Ray-casting point-in-polygon test, expressed in `geq`'s expression language: for each edge,
/// a term that's `1` if a ray from `(rx,ry)` in the `+X` direction crosses that edge, `0`
/// otherwise; the point is inside iff the sum of crossing terms is odd (`mod(sum, 2)`). Mirrors
/// [`point_in_polygon`] term-for-term — that function is the unit-tested reference this
/// expression is built to match. Each vertex is `vertex_fraction * w_expr`/`h_expr` (a
/// sub-expression when `w_expr`/`h_expr` are keyframed, a plain number otherwise) rather than a
/// precomputed pixel literal, same generalization as [`ellipse_inside_expr`].
fn polygon_inside_expr(
    rx: &str,
    ry: &str,
    vertices: &[(f32, f32)],
    w_expr: &str,
    h_expr: &str,
) -> String {
    let mut terms = Vec::with_capacity(vertices.len());
    for i in 0..vertices.len() {
        let (vx1, vy1) = vertices[i];
        let (vx2, vy2) = vertices[(i + 1) % vertices.len()];
        let x1 = format!("({vx1:.6}*{w_expr})");
        let y1 = format!("({vy1:.6}*{h_expr})");
        let x2 = format!("({vx2:.6}*{w_expr})");
        let y2 = format!("({vy2:.6}*{h_expr})");
        // straddle = 1 if the edge's endpoints are on opposite sides of the ray's y — gt()
        // returns 0/1, so their difference is -1/0/1 and abs() collapses that to 0/1.
        let straddle = format!("abs(gt({y1}\\,{ry})-gt({y2}\\,{ry}))");
        let x_intersect = format!("({x1}+({ry}-{y1})/({y2}-{y1}+{RAY_EPSILON})*({x2}-{x1}))");
        terms.push(format!("{straddle}*gt({x_intersect}\\,{rx})"));
    }
    format!("mod({sum}\\,2)", sum = terms.join("+"))
}

/// Pure-Rust reference implementation of the same ray-casting algorithm
/// [`polygon_inside_expr`] encodes as a `geq` expression — the thing that's actually unit
/// tested, since the generated expression string itself can only be checked by eye/by FFmpeg
/// (see this module's verification caveat).
pub fn point_in_polygon(point: (f64, f64), vertices: &[(f64, f64)]) -> bool {
    let (px, py) = point;
    let mut crossings = 0u32;
    for i in 0..vertices.len() {
        let (x1, y1) = vertices[i];
        let (x2, y2) = vertices[(i + 1) % vertices.len()];
        if (y1 > py) != (y2 > py) {
            let x_intersect = x1 + (py - y1) / (y2 - y1 + RAY_EPSILON) * (x2 - x1);
            if x_intersect > px {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

#[cfg(test)]
#[path = "shape_render/shape_render_test.rs"]
mod tests;
