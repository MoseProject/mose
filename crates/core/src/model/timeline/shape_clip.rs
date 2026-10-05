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

use serde::{Deserialize, Serialize};

use crate::keyframe::Keyframe;

/// Which geometric primitive a [`ShapeClip`] draws. `Polygon` covers every straight-edged
/// preset `request.md`'s "geometric forms" ask names (rectangle, square, triangle, trapezoid,
/// arrow) plus a user-drawn custom shape — all just a list of vertices in the shape's own
/// `-0.5..=0.5` local unit square, tested for point-in-polygon via ray casting (handles
/// concave outlines like the arrow's, not just convex ones) — see [`ShapeKind::rectangle`] etc.
/// for the fixed presets and `avcore::shape_render` for the rendering math. `Ellipse` (also
/// used for a locked-aspect "circle") gets its own variant since a quadratic in/out test is far
/// cheaper than ray-casting an approximated polygon would be.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeKind {
    Ellipse,
    /// Vertices in order (clockwise or counter-clockwise, either works for the ray-casting
    /// test), each roughly `-0.5..=0.5` — the shape's own local unit square before it's scaled
    /// by [`ShapeClip::width`]/[`ShapeClip::height`], rotated, and translated to
    /// [`ShapeClip::center_x`]/[`ShapeClip::center_y`]. `request.md`'s "opção de desenhar uma
    /// forma personalizada" (custom shape) is this same variant with user-placed vertices — the
    /// properties panel's per-vertex X/Y editor (`ui`'s `polygon_vertex_editor`, shown whenever
    /// a `ShapeClip`'s `shape_kind` is a `Polygon` — every preset included, since they're all
    /// `Polygon` under the hood too, see `ShapeKind::rectangle()` etc.) lets a user hand-edit,
    /// add, or remove vertices starting from any preset or from scratch, so this is no longer
    /// data-model-only.
    Polygon(Vec<(f32, f32)>),
}

impl Default for ShapeKind {
    fn default() -> Self {
        Self::rectangle()
    }
}

impl ShapeKind {
    pub fn rectangle() -> Self {
        Self::Polygon(vec![(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)])
    }

    /// Same vertices as [`Self::rectangle`] — "square" is a UI-level aspect lock (equal
    /// `width`/`height` on the [`ShapeClip`]), not a different shape.
    pub fn square() -> Self {
        Self::rectangle()
    }

    pub fn triangle() -> Self {
        Self::Polygon(vec![(0.0, -0.5), (0.5, 0.5), (-0.5, 0.5)])
    }

    /// Narrower top edge than bottom, per the conventional trapezoid look.
    pub fn trapezoid() -> Self {
        Self::Polygon(vec![(-0.25, -0.5), (0.25, -0.5), (0.5, 0.5), (-0.5, 0.5)])
    }

    /// A right-pointing arrow: a thin shaft plus a wide triangular head, as one seven-vertex
    /// concave polygon (correct under ray casting, unlike a convex-only in/out test).
    pub fn arrow() -> Self {
        Self::Polygon(vec![
            (-0.5, -0.15),
            (0.1, -0.15),
            (0.1, -0.35),
            (0.5, 0.0),
            (0.1, 0.35),
            (0.1, 0.15),
            (-0.5, 0.15),
        ])
    }
}

/// A `Copy`-friendly name for [`ShapeKind`]'s fixed presets, shared by every shell's shape
/// picker (`ShapeKind` itself can't be `Copy`: its `Polygon` variant holds a `Vec`). `Custom`
/// stands for a `Polygon` that matches no preset's exact vertex list -- only ever shown as the
/// current value, never offered as something to pick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShapePreset {
    Ellipse,
    Rectangle,
    Triangle,
    Trapezoid,
    Arrow,
    Custom,
}

impl ShapePreset {
    /// The presets a picker offers, in display order (`Custom` excluded).
    pub const PICKABLE: [Self; 5] = [
        Self::Ellipse,
        Self::Rectangle,
        Self::Triangle,
        Self::Trapezoid,
        Self::Arrow,
    ];

    /// Classifies `kind` by exact vertex comparison, not vertex count.
    pub fn from_kind(kind: &ShapeKind) -> Self {
        Self::PICKABLE
            .into_iter()
            .find(|preset| preset.to_kind().as_ref() == Some(kind))
            .unwrap_or(Self::Custom)
    }

    /// The shape this preset stands for; `None` for `Custom`, which has no fixed geometry.
    pub fn to_kind(self) -> Option<ShapeKind> {
        match self {
            Self::Ellipse => Some(ShapeKind::Ellipse),
            Self::Rectangle => Some(ShapeKind::rectangle()),
            Self::Triangle => Some(ShapeKind::triangle()),
            Self::Trapezoid => Some(ShapeKind::trapezoid()),
            Self::Arrow => Some(ShapeKind::arrow()),
            Self::Custom => None,
        }
    }
}

/// One placed geometric shape on a [`super::Track`] whose [`super::TrackKind`] is
/// [`super::TrackKind::Shape`]. Rendered the same way [`super::TextClip`] is — a `geq`-based
/// post-processing pass after the main timeline encode, see `avcore::shape_render`/
/// `avbridge::apply_shape_overlays`.
///
/// Previewed via [`crate::overlay_render::render_shape_clip_rgba`], same static-overlay-branch
/// mechanism as [`super::TextClip`]'s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShapeClip {
    pub id: u64,
    /// Start time on the timeline, in seconds.
    pub start_secs: f64,
    /// How long the shape stays visible, in seconds.
    pub duration_secs: f64,
    pub shape_kind: ShapeKind,
    /// Center, as a `0.0..=1.0` fraction of canvas width/height.
    pub center_x: f32,
    pub center_y: f32,
    /// General keyframe animation for this shape's center position over its own on-timeline
    /// duration, per the keyframe-expansion gap found while surveying what else the existing
    /// keyframe system could drive (`spec/ROADMAP.md` P4 item 34) — the first slice of that
    /// item to ship, since `ShapeClip`'s export path (a self-contained `geq` expression built
    /// entirely in Rust, see `crate::shape_render`) turned out to already support a `T`-keyed
    /// per-pixel expression without any FFI/C changes, unlike `TextClip`'s pre-rasterized-PNG
    /// overlay approach (see `TextClip::pos_x_keyframes` for how that one ships instead, as a
    /// pixel-offset `overlay` stage rather than a `geq` formula). Each field independently
    /// overrides its own constant (`center_x`/`center_y`) when
    /// non-empty, same "keyframes win when present" relationship every other keyframe field in
    /// this codebase already has. `#[serde(default)]` so older saved projects load with no
    /// position animation.
    #[serde(default)]
    pub center_x_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub center_y_keyframes: Vec<Keyframe<f32>>,
    /// Size, as a `0.0..=1.0` fraction of canvas width/height — what dragging a resize handle
    /// changes.
    pub width: f32,
    pub height: f32,
    /// General keyframe animation for width/height over this shape's own on-timeline duration —
    /// the rest of `spec/ROADMAP.md` P4 item 34's `ShapeClip` scope, shipped after position
    /// animation. `shape_render::inside_expr`'s geometry math (`ellipse_inside_expr`/
    /// `polygon_inside_expr`) was reworked to accept `geq`-expression-language sub-expressions
    /// for the half-extents instead of literal `f64`s, so the same multiply-through-avoid-
    /// division trick the static case always used still applies verbatim — an unkeyframed
    /// `width`/`height` degenerates back to the same plain numeric literal
    /// `keyframe::shape_axis_expr` already returns for the unkeyframed position case, so the
    /// static-shape math is unchanged in that (still the common) case. `#[serde(default)]` so
    /// older saved projects load with no size animation.
    #[serde(default)]
    pub width_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub height_keyframes: Vec<Keyframe<f32>>,
    /// Clockwise rotation around the shape's own center, in degrees.
    pub rotation_deg: f32,
    /// General keyframe animation for `rotation_deg` over this shape's own on-timeline
    /// duration — the last piece of P4 item 34's `ShapeClip` scope. Unlike width/height, this
    /// doesn't touch `inside_expr` at all: only the local-frame rotation (`rx`/`ry` in
    /// `shape_render::build_shape_filter_desc`) changes, from Rust-precomputed `sin`/`cos`
    /// literals to `geq`'s own `sin(...)`/`cos(...)`/`PI` expression-language functions (all
    /// confirmed present in FFmpeg's expression evaluator, not assumed) evaluated per pixel.
    /// `#[serde(default)]` so older saved projects load with no rotation animation.
    #[serde(default)]
    pub rotation_keyframes: Vec<Keyframe<f32>>,
    /// RGBA fill/stroke color: `[r, g, b, a]`, each 0–255. Alpha 255 = fully opaque.
    pub color_rgba: [u8; 4],
    /// Outline thickness in pixels. `0.0` = filled shape; `> 0.0` = outline only, that thick
    /// (clamped to the shape's own half-extent, so a thickness larger than the shape still
    /// renders as filled rather than vanishing).
    pub stroke_thickness_px: f32,
}

impl ShapeClip {
    /// The manually inserted default: a centered white rectangle, 30% of the canvas on each
    /// axis, lasting 3 seconds from `start_secs`. Every shell's "add shape" goes through this.
    pub fn new(id: u64, start_secs: f64) -> Self {
        Self {
            id,
            start_secs,
            duration_secs: 3.0,
            shape_kind: ShapeKind::rectangle(),
            center_x: 0.5,
            center_y: 0.5,
            center_x_keyframes: Vec::new(),
            center_y_keyframes: Vec::new(),
            width: 0.3,
            height: 0.3,
            width_keyframes: Vec::new(),
            height_keyframes: Vec::new(),
            rotation_deg: 0.0,
            rotation_keyframes: Vec::new(),
            color_rgba: [255, 255, 255, 255],
            stroke_thickness_px: 0.0,
        }
    }
}
