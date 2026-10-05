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

//! CF-07: parameterized motion-graphics templates
//! ([`spec/architecture/competitive-feature-plan.md`](../../../../spec/architecture/competitive-feature-plan.md)'s
//! own "Outcome: reusable channel assets such as lower thirds, scoreboards, subscribe prompts,
//! webcam frames").
//!
//! This module is CF-07's own slice 1: "Define a versioned declarative JSON format with
//! allowlisted primitives and editable parameters." A [`GraphicTemplate`] is a named, portable
//! JSON asset — deliberately plain `serde_json`, not this crate's usual gzip-MessagePack
//! `.ocproj` framing, since a template is meant to be inspected and shared, not just persisted
//! (CF-07 slice 3's own "package templates as data" goal). Its primitives are deliberately just
//! [`TemplateElement::Text`]/[`TemplateElement::Shape`] for this slice — the exact two overlay
//! kinds `crate::timeline`/`crate::overlay_render`/`crate::shape_render` already render, per
//! `spec/RULES.md`'s reuse-before-building rule. An `Image` primitive (slice 2's own scope) has
//! no existing overlay-clip kind to reuse yet — [`crate::timeline::TrackKind`] has no `Image`
//! variant — so adding it here now would mean inventing new overlay-rendering infrastructure
//! this slice was never meant to cover; a real, deliberate follow-up, not an oversight.
//!
//! `TemplateElement` is a closed enum (no `#[serde(other)]`, matching
//! [`crate::gameplay_events::GameplayEventKind`]'s own precedent) — an unrecognized primitive
//! kind fails to deserialize outright, per CF-07's own "Unknown primitives/parameters are
//! rejected rather than executed or ignored" acceptance criterion. The other half of that
//! criterion — an element bound to a parameter id nothing declares — is [`GraphicTemplate::
//! validate`]'s job. Templates contain no scripts or executable expressions anywhere in this
//! format; every field is a plain literal or a named reference into [`GraphicTemplate::
//! parameters`], matching the doc's own "no scripts or executable expressions" constraint for
//! this whole feature.
//!
//! Slice 2 ("support text, color, image, timing, safe-area anchors, and aspect-ratio variants")
//! adds three of those five here: [`safe_area_violations`] (a non-blocking design-time check, not
//! a repositioning mechanism — see its own doc comment), [`TemplateFamily`] (grouping sibling
//! [`GraphicTemplate`]s prepared for different [`crate::export::ExportAspectRatio`]s, rather than
//! one template auto-adapting its own layout across canvas shapes), and [`TemplateTextElement::
//! timing`] (fade-in/fade-out, resolved into real opacity keyframes by
//! [`timing_opacity_keyframes`] once a `ui`-side caller knows the instantiated clip's actual
//! placed duration). `Image` remains open, for the same "no overlay-clip kind to reuse yet"
//! reason slice 1 documented; so does timing for `TemplateShapeElement` specifically —
//! [`crate::timeline::ShapeClip`] has no opacity-keyframe field of its own yet (only
//! center/width/height/rotation), a real, separate follow-up from adding one.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::export::ExportAspectRatio;
use crate::keyframe::Keyframe;
use crate::timeline::{ShapeKind, TextFontFamily, TextFontStyle};

/// The only schema version this build understands — see [`TemplateValidationError::
/// UnsupportedSchemaVersion`]. Bumped, with an explicit migration, once a real format change
/// needs one (CF-07 slice 4's own "migration tests").
pub const TEMPLATE_SCHEMA_VERSION: u32 = 1;

/// The type of value a [`TemplateParameter`] expects — deliberately just the two CF-07 slice 1
/// calls out ("text, color" lead that list before "image, timing..." arrive in slice 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemplateParameterKind {
    Text,
    Color,
}

/// One named, editable slot a template exposes — e.g. "player_name" (`Text`) or
/// "team_color" (`Color`) for a scoreboard template. `id` is referenced by
/// [`TextBinding::Parameter`]/[`ColorBinding::Parameter`] elsewhere in the same
/// [`GraphicTemplate`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateParameter {
    pub id: String,
    /// User-facing label for this slot in an "apply template" form — not a translatable UI
    /// string (same reasoning `Project::summary` documents: this is user-authored template
    /// content, not application chrome).
    pub label: String,
    pub kind: TemplateParameterKind,
}

/// Where a text element's string comes from: a literal baked into the template, or a named
/// [`TemplateParameter`] (of [`TemplateParameterKind::Text`]) the applying user fills in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TextBinding {
    Fixed(String),
    Parameter(String),
}

/// Where a color field comes from — the same shape as [`TextBinding`], for
/// [`TemplateParameterKind::Color`] parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ColorBinding {
    /// `[r, g, b, a]`, each 0–255 — same convention every other RGBA color field in this crate
    /// uses (e.g. [`crate::timeline::TextClip::color_rgba`]).
    Fixed([u8; 4]),
    Parameter(String),
}

/// A [`crate::timeline::TextClip`]-shaped template primitive. Position is a safe-area-agnostic
/// `0.0..=1.0` canvas fraction for this slice (matching `TextClip::pos_x`/`pos_y`'s own existing
/// convention) — CF-07 slice 2's own dedicated safe-area-anchor semantics are a real, separate
/// follow-up, not assumed here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateTextElement {
    /// Unique within the owning [`GraphicTemplate`] — see [`TemplateValidationError::
    /// DuplicateElementId`].
    pub id: String,
    pub text: TextBinding,
    pub color_rgba: ColorBinding,
    pub font_family: TextFontFamily,
    pub font_style: TextFontStyle,
    pub font_size: f32,
    pub pos_x: f32,
    pub pos_y: f32,
    /// CF-07 slice 2's "timing" (animation in/out) — a fully optional fade-in/fade-out spec
    /// applied automatically at apply time (see [`timing_opacity_keyframes`]), rather than
    /// requiring a manual keyframe edit after placement like every other template-instantiated
    /// clip today. `#[serde(default)]` (all-zero, i.e. no animation) so a template authored
    /// before this field existed still applies with its exact old static behavior. Text-only for
    /// this slice: unlike [`crate::timeline::TextClip`], [`crate::timeline::ShapeClip`] has no
    /// opacity-keyframe field of its own yet (only center/width/height/rotation) — animating a
    /// shape's fade in/out needs that field added first, a real, separate follow-up.
    #[serde(default)]
    pub timing: TemplateTiming,
}

/// CF-07 slice 2's "timing": how long a template-instantiated element takes to fade in at the
/// start of its on-timeline duration and fade out at the end, in seconds. `Default` (`0.0`/
/// `0.0`) means no animation at all — the exact behavior every template had before this field
/// existed. Negative values are rejected by [`GraphicTemplate::validate`]; a combined fade time
/// exceeding the clip's actual placed duration is handled by [`timing_opacity_keyframes`]
/// shrinking both proportionally, not by validation here (this type has no duration of its own
/// to compare against — that's only known once a caller places the instantiated clip).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct TemplateTiming {
    pub fade_in_secs: f32,
    pub fade_out_secs: f32,
}

/// Converts `timing` into `Keyframe<f32>` opacity points, expressed as
/// [`crate::keyframe::Keyframe`]'s own `0.0..=1.0`-of-`duration_secs` `time_fraction`
/// convention, ready to drop straight into [`crate::timeline::TextClip::opacity_keyframes`].
/// Returns an empty `Vec` — [`crate::keyframe::evaluate_keyframes`]'s own "0 keyframes -> the
/// caller's default (fully opaque)" behavior — when neither fade is positive or `duration_secs`
/// is non-positive, so a template authored before `TemplateTiming` existed (all-zero, via
/// `#[serde(default)]`) instantiates with the exact same "no opacity animation at all" result it
/// always had.
///
/// A combined fade time exceeding `duration_secs` is scaled down proportionally rather than
/// producing out-of-order `time_fraction`s (an authored/parameter-driven duration mismatch,
/// not a validation-time concern — [`GraphicTemplate::validate`] only checks
/// non-negativity, since a `TemplateTextElement` has no duration of its own to compare
/// against).
pub fn timing_opacity_keyframes(timing: TemplateTiming, duration_secs: f64) -> Vec<Keyframe<f32>> {
    let fade_in_secs = timing.fade_in_secs.max(0.0) as f64;
    let fade_out_secs = timing.fade_out_secs.max(0.0) as f64;
    if (fade_in_secs <= 0.0 && fade_out_secs <= 0.0) || duration_secs <= 0.0 {
        return Vec::new();
    }

    let total = fade_in_secs + fade_out_secs;
    let scale = if total > duration_secs {
        duration_secs / total
    } else {
        1.0
    };
    let fade_in_secs = fade_in_secs * scale;
    let fade_out_secs = fade_out_secs * scale;

    let mut keyframes = Vec::new();
    if fade_in_secs > 0.0 {
        keyframes.push(Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        });
        keyframes.push(Keyframe {
            time_fraction: (fade_in_secs / duration_secs) as f32,
            value: 1.0,
        });
    }
    if fade_out_secs > 0.0 {
        keyframes.push(Keyframe {
            time_fraction: (1.0 - fade_out_secs / duration_secs) as f32,
            value: 1.0,
        });
        keyframes.push(Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        });
    }
    keyframes
}

/// A [`crate::timeline::ShapeClip`]-shaped template primitive — the same field set, minus the
/// keyframe-animation lists (a template element is a static starting point; animating an
/// instantiated shape happens after placement, the same as any other `ShapeClip` today).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateShapeElement {
    pub id: String,
    pub shape_kind: ShapeKind,
    pub color_rgba: ColorBinding,
    pub center_x: f32,
    pub center_y: f32,
    pub width: f32,
    pub height: f32,
    pub rotation_deg: f32,
    pub stroke_thickness_px: f32,
}

/// One allowlisted template primitive — see this module's own doc comment for why only these
/// two exist in this slice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TemplateElement {
    Text(TemplateTextElement),
    Shape(TemplateShapeElement),
}

impl TemplateElement {
    fn id(&self) -> &str {
        match self {
            Self::Text(t) => &t.id,
            Self::Shape(s) => &s.id,
        }
    }
}

/// A reusable, parameterized graphic asset — CF-07's own "lower thirds, scoreboards, subscribe
/// prompts, webcam frames" — as plain declarative data. See this module's own doc comment for
/// the format's scope and constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphicTemplate {
    pub schema_version: u32,
    pub name: String,
    /// The canvas this template's `pos_x`/`pos_y`/`center_x`/`center_y` fractions were designed
    /// against — see [`TemplateFamily`] for how a template groups with sibling variants designed
    /// for other aspect ratios.
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// CF-07 slice 2's own "safe-area anchors," scoped as a non-blocking design-time check
    /// rather than a repositioning mechanism: the fraction (`0.0..=1.0`) of the shorter canvas
    /// dimension near every edge that platform chrome (captions, app UI, rounded-corner crop on
    /// some players) commonly occupies — [`safe_area_violations`] flags any element whose
    /// position/extent intrudes into it. `0.0` (the default) disables the check entirely, so a
    /// template authored under slice 1 (before this field existed) loads with the exact same
    /// "never flagged" behavior it always had. `#[serde(default)]` for that same forwards-
    /// compatibility reason.
    #[serde(default)]
    pub safe_area_margin: f32,
    pub parameters: Vec<TemplateParameter>,
    pub elements: Vec<TemplateElement>,
}

/// Why a [`GraphicTemplate`] failed [`GraphicTemplate::validate`] — each variant's
/// [`Display`](std::fmt::Display) names the offending id/value, matching this crate's
/// established convention for structured, actionable validation errors (e.g.
/// [`crate::gameplay_events::SidecarValidationError`]).
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateValidationError {
    UnsupportedSchemaVersion {
        found: u32,
    },
    InvalidCanvasSize {
        width: u32,
        height: u32,
    },
    DuplicateParameterId(String),
    DuplicateElementId(String),
    UnknownParameterReference {
        element_id: String,
        parameter_id: String,
    },
    ParameterKindMismatch {
        element_id: String,
        parameter_id: String,
        expected: TemplateParameterKind,
    },
    NonPositiveFontSize {
        element_id: String,
        value: f32,
    },
    NonPositiveShapeExtent {
        element_id: String,
        width: f32,
        height: f32,
    },
    NegativeTiming {
        element_id: String,
        fade_in_secs: f32,
        fade_out_secs: f32,
    },
}

impl std::fmt::Display for TemplateValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found } => write!(
                f,
                "unsupported schema_version {found} (this build only understands \
                 {TEMPLATE_SCHEMA_VERSION})"
            ),
            Self::InvalidCanvasSize { width, height } => write!(
                f,
                "canvas_width/canvas_height must both be positive (got {width}x{height})"
            ),
            Self::DuplicateParameterId(id) => {
                write!(f, "duplicate parameter id {id:?}")
            }
            Self::DuplicateElementId(id) => {
                write!(f, "duplicate element id {id:?}")
            }
            Self::UnknownParameterReference {
                element_id,
                parameter_id,
            } => write!(
                f,
                "element {element_id:?} references parameter {parameter_id:?}, which this \
                 template does not declare"
            ),
            Self::ParameterKindMismatch {
                element_id,
                parameter_id,
                expected,
            } => write!(
                f,
                "element {element_id:?} references parameter {parameter_id:?} as {expected:?}, \
                 but that parameter is declared with a different kind"
            ),
            Self::NonPositiveFontSize { element_id, value } => write!(
                f,
                "text element {element_id:?}: font_size must be positive (got {value})"
            ),
            Self::NonPositiveShapeExtent {
                element_id,
                width,
                height,
            } => write!(
                f,
                "shape element {element_id:?}: width/height must both be positive (got \
                 {width}x{height})"
            ),
            Self::NegativeTiming {
                element_id,
                fade_in_secs,
                fade_out_secs,
            } => write!(
                f,
                "text element {element_id:?}: timing.fade_in_secs/fade_out_secs must both be \
                 non-negative (got {fade_in_secs}/{fade_out_secs})"
            ),
        }
    }
}

impl std::error::Error for TemplateValidationError {}

/// Either half of [`GraphicTemplate::parse_and_validate`]'s failure — kept distinct from
/// [`TemplateValidationError`] alone since a malformed-JSON failure and a well-formed-but-invalid
/// failure are different classes of problem for a caller to report (same reasoning
/// [`crate::gameplay_events::SidecarParseOrValidationError`] documents).
#[derive(Debug)]
pub enum TemplateParseOrValidationError {
    Parse(serde_json::Error),
    Validation(TemplateValidationError),
}

impl std::fmt::Display for TemplateParseOrValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "could not parse graphic template JSON: {e}"),
            Self::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TemplateParseOrValidationError {}

impl From<TemplateValidationError> for TemplateParseOrValidationError {
    fn from(e: TemplateValidationError) -> Self {
        Self::Validation(e)
    }
}

fn parameter_kind(parameters: &[TemplateParameter], id: &str) -> Option<TemplateParameterKind> {
    parameters.iter().find(|p| p.id == id).map(|p| p.kind)
}

impl GraphicTemplate {
    /// Parses and validates `json` in one step — the only entry point external callers should
    /// use, so an accepted [`GraphicTemplate`] is always known-valid (same "sidecars are
    /// untrusted input" convention [`crate::gameplay_events::EventSidecar::parse_and_validate`]
    /// established).
    pub fn parse_and_validate(json: &str) -> Result<Self, TemplateParseOrValidationError> {
        let template: GraphicTemplate =
            serde_json::from_str(json).map_err(TemplateParseOrValidationError::Parse)?;
        template.validate()?;
        Ok(template)
    }

    /// Checks every CF-07 slice-1-relevant constraint: schema version, canvas size, unique
    /// parameter/element ids, and every parameter reference resolving to a declared parameter of
    /// the matching kind. An unrecognized primitive *kind* never reaches this function at all —
    /// it already fails at JSON deserialization, since [`TemplateElement`] is a closed enum.
    pub fn validate(&self) -> Result<(), TemplateValidationError> {
        if self.schema_version != TEMPLATE_SCHEMA_VERSION {
            return Err(TemplateValidationError::UnsupportedSchemaVersion {
                found: self.schema_version,
            });
        }
        if self.canvas_width == 0 || self.canvas_height == 0 {
            return Err(TemplateValidationError::InvalidCanvasSize {
                width: self.canvas_width,
                height: self.canvas_height,
            });
        }

        let mut seen_parameter_ids = std::collections::HashSet::new();
        for parameter in &self.parameters {
            if !seen_parameter_ids.insert(parameter.id.as_str()) {
                return Err(TemplateValidationError::DuplicateParameterId(
                    parameter.id.clone(),
                ));
            }
        }

        let mut seen_element_ids = std::collections::HashSet::new();
        for element in &self.elements {
            if !seen_element_ids.insert(element.id()) {
                return Err(TemplateValidationError::DuplicateElementId(
                    element.id().to_string(),
                ));
            }
            match element {
                TemplateElement::Text(t) => {
                    if t.font_size <= 0.0 {
                        return Err(TemplateValidationError::NonPositiveFontSize {
                            element_id: t.id.clone(),
                            value: t.font_size,
                        });
                    }
                    if t.timing.fade_in_secs < 0.0 || t.timing.fade_out_secs < 0.0 {
                        return Err(TemplateValidationError::NegativeTiming {
                            element_id: t.id.clone(),
                            fade_in_secs: t.timing.fade_in_secs,
                            fade_out_secs: t.timing.fade_out_secs,
                        });
                    }
                    if let TextBinding::Parameter(pid) = &t.text {
                        self.check_reference(&t.id, pid, TemplateParameterKind::Text)?;
                    }
                    if let ColorBinding::Parameter(pid) = &t.color_rgba {
                        self.check_reference(&t.id, pid, TemplateParameterKind::Color)?;
                    }
                }
                TemplateElement::Shape(s) => {
                    if s.width <= 0.0 || s.height <= 0.0 {
                        return Err(TemplateValidationError::NonPositiveShapeExtent {
                            element_id: s.id.clone(),
                            width: s.width,
                            height: s.height,
                        });
                    }
                    if let ColorBinding::Parameter(pid) = &s.color_rgba {
                        self.check_reference(&s.id, pid, TemplateParameterKind::Color)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn check_reference(
        &self,
        element_id: &str,
        parameter_id: &str,
        expected: TemplateParameterKind,
    ) -> Result<(), TemplateValidationError> {
        match parameter_kind(&self.parameters, parameter_id) {
            None => Err(TemplateValidationError::UnknownParameterReference {
                element_id: element_id.to_string(),
                parameter_id: parameter_id.to_string(),
            }),
            Some(found) if found != expected => {
                Err(TemplateValidationError::ParameterKindMismatch {
                    element_id: element_id.to_string(),
                    parameter_id: parameter_id.to_string(),
                    expected,
                })
            }
            Some(_) => Ok(()),
        }
    }
}

/// A concrete value supplied for one [`TemplateParameter`] when instantiating a
/// [`GraphicTemplate`] — see [`instantiate`].
#[derive(Debug, Clone, PartialEq)]
pub enum ParameterValue {
    Text(String),
    Color([u8; 4]),
}

/// One [`TemplateElement`] with every binding resolved to a concrete value — ready for a `ui`-
/// side caller to build a real [`crate::timeline::TextClip`]/[`crate::timeline::ShapeClip`] from
/// (assigning its own id/`start_secs`/`duration_secs` and placing it on a track — deliberately
/// left to that caller, since this module stays timeline/track-agnostic per its own "UI/render
/// code" boundary).
#[derive(Debug, Clone, PartialEq)]
pub enum InstantiatedElement {
    Text {
        source_id: String,
        text: String,
        color_rgba: [u8; 4],
        font_family: TextFontFamily,
        font_style: TextFontStyle,
        font_size: f32,
        pos_x: f32,
        pos_y: f32,
        timing: TemplateTiming,
    },
    Shape {
        source_id: String,
        shape_kind: ShapeKind,
        color_rgba: [u8; 4],
        center_x: f32,
        center_y: f32,
        width: f32,
        height: f32,
        rotation_deg: f32,
        stroke_thickness_px: f32,
    },
}

/// Why [`instantiate`] couldn't resolve a template against the supplied `values` — a missing
/// value for a declared parameter, or a value of the wrong [`TemplateParameterKind`] for what an
/// element expects.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateInstantiateError {
    Validation(TemplateValidationError),
    MissingParameterValue(String),
    WrongParameterValueKind {
        parameter_id: String,
        expected: TemplateParameterKind,
    },
}

impl std::fmt::Display for TemplateInstantiateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Validation(e) => write!(f, "{e}"),
            Self::MissingParameterValue(id) => {
                write!(f, "no value supplied for parameter {id:?}")
            }
            Self::WrongParameterValueKind {
                parameter_id,
                expected,
            } => write!(
                f,
                "value supplied for parameter {parameter_id:?} is not a {expected:?}"
            ),
        }
    }
}

impl std::error::Error for TemplateInstantiateError {}

fn resolve_text(
    binding: &TextBinding,
    values: &HashMap<String, ParameterValue>,
) -> Result<String, TemplateInstantiateError> {
    match binding {
        TextBinding::Fixed(text) => Ok(text.clone()),
        TextBinding::Parameter(id) => match values.get(id) {
            Some(ParameterValue::Text(text)) => Ok(text.clone()),
            Some(ParameterValue::Color(_)) => {
                Err(TemplateInstantiateError::WrongParameterValueKind {
                    parameter_id: id.clone(),
                    expected: TemplateParameterKind::Text,
                })
            }
            None => Err(TemplateInstantiateError::MissingParameterValue(id.clone())),
        },
    }
}

fn resolve_color(
    binding: &ColorBinding,
    values: &HashMap<String, ParameterValue>,
) -> Result<[u8; 4], TemplateInstantiateError> {
    match binding {
        ColorBinding::Fixed(rgba) => Ok(*rgba),
        ColorBinding::Parameter(id) => match values.get(id) {
            Some(ParameterValue::Color(rgba)) => Ok(*rgba),
            Some(ParameterValue::Text(_)) => {
                Err(TemplateInstantiateError::WrongParameterValueKind {
                    parameter_id: id.clone(),
                    expected: TemplateParameterKind::Color,
                })
            }
            None => Err(TemplateInstantiateError::MissingParameterValue(id.clone())),
        },
    }
}

/// Resolves every element of `template` against `values` into placement-ready
/// [`InstantiatedElement`]s — CF-07's "editable parameters" made real, not just declared.
/// Validates `template` first (so a caller never needs to call [`GraphicTemplate::validate`]
/// itself before this), then fails on the first missing/mismatched parameter value.
pub fn instantiate(
    template: &GraphicTemplate,
    values: &HashMap<String, ParameterValue>,
) -> Result<Vec<InstantiatedElement>, TemplateInstantiateError> {
    template
        .validate()
        .map_err(TemplateInstantiateError::Validation)?;

    template
        .elements
        .iter()
        .map(|element| match element {
            TemplateElement::Text(t) => Ok(InstantiatedElement::Text {
                source_id: t.id.clone(),
                text: resolve_text(&t.text, values)?,
                color_rgba: resolve_color(&t.color_rgba, values)?,
                font_family: t.font_family.clone(),
                font_style: t.font_style,
                font_size: t.font_size,
                pos_x: t.pos_x,
                pos_y: t.pos_y,
                timing: t.timing,
            }),
            TemplateElement::Shape(s) => Ok(InstantiatedElement::Shape {
                source_id: s.id.clone(),
                shape_kind: s.shape_kind.clone(),
                color_rgba: resolve_color(&s.color_rgba, values)?,
                center_x: s.center_x,
                center_y: s.center_y,
                width: s.width,
                height: s.height,
                rotation_deg: s.rotation_deg,
                stroke_thickness_px: s.stroke_thickness_px,
            }),
        })
        .collect()
}

/// One element [`safe_area_violations`] flagged, and why — always names the offending element,
/// same "actionable, not just a bare bool" convention this module's other diagnostics follow.
#[derive(Debug, Clone, PartialEq)]
pub struct SafeAreaViolation {
    pub element_id: String,
    pub message: String,
}

/// Flags every element whose position (text) or bounding box (shape) intrudes into
/// [`GraphicTemplate::safe_area_margin`] — a non-blocking design-time check, never a
/// [`GraphicTemplate::validate`] failure, since a template legitimately wanting a full-bleed
/// background or edge-anchored element is a real, valid design choice this shouldn't forbid
/// (same "warn, don't block" precedent [`crate::text_layout::scan_bidi_controls`] established
/// for a different feature). A no-op returning no violations when `safe_area_margin <= 0.0`.
///
/// A text element has no baked width in this format (no shaping happens until the `ui`-side
/// apply step) — this only checks its anchor point (`pos_x`/`pos_y`) clears the margin, not the
/// full rendered text extent, a real, documented simplification. A shape element's bounding box
/// is conservatively approximated as a square of side `max(width, height)` centered on
/// `center_x`/`center_y` — cheap and rotation-safe (the true rotated extent can only shrink
/// toward this square, never exceed it), at the cost of occasionally over-flagging a shape that
/// would actually clear the margin once its real rotated footprint is considered.
pub fn safe_area_violations(template: &GraphicTemplate) -> Vec<SafeAreaViolation> {
    let margin = template.safe_area_margin;
    if margin <= 0.0 {
        return Vec::new();
    }

    let mut violations = Vec::new();
    for element in &template.elements {
        match element {
            TemplateElement::Text(t) => {
                let clear = t.pos_x >= margin
                    && t.pos_x <= 1.0 - margin
                    && t.pos_y >= margin
                    && t.pos_y <= 1.0 - margin;
                if !clear {
                    violations.push(SafeAreaViolation {
                        element_id: t.id.clone(),
                        message: format!(
                            "text anchor at ({:.3}, {:.3}) is inside the {:.0}% safe-area margin",
                            t.pos_x,
                            t.pos_y,
                            margin * 100.0
                        ),
                    });
                }
            }
            TemplateElement::Shape(s) => {
                let half_extent = s.width.max(s.height) / 2.0;
                let left = s.center_x - half_extent;
                let right = s.center_x + half_extent;
                let top = s.center_y - half_extent;
                let bottom = s.center_y + half_extent;
                let clear = left >= margin
                    && right <= 1.0 - margin
                    && top >= margin
                    && bottom <= 1.0 - margin;
                if !clear {
                    violations.push(SafeAreaViolation {
                        element_id: s.id.clone(),
                        message: format!(
                            "shape bounding box ({:.3}, {:.3}) to ({:.3}, {:.3}) intrudes into \
                             the {:.0}% safe-area margin",
                            left,
                            top,
                            right,
                            bottom,
                            margin * 100.0
                        ),
                    });
                }
            }
        }
    }
    violations
}

/// One aspect-ratio-specific rendition of a [`TemplateFamily`] — CF-07 slice 2's own
/// "aspect-ratio variants," modeled as sibling [`GraphicTemplate`]s rather than a single
/// template with a formula that auto-adapts position across canvas shapes: a lower third
/// designed for 16:9 and one designed for 9:16 are, in practice, different layouts (different
/// element placement, not just a rescale), so this reuses [`crate::export::ExportAspectRatio`]
/// (already this crate's own aspect-ratio vocabulary — see `spec/RULES.md`'s reuse-before-
/// building rule) to tag which one each prepared variant is for, rather than inventing a second
/// aspect-ratio type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateVariant {
    pub aspect_ratio: ExportAspectRatio,
    pub template: GraphicTemplate,
}

/// A named group of [`TemplateVariant`]s — what CF-07's own doc means by "reusable channel
/// assets... and aspect-ratio variants": one conceptual template (e.g. "Scoreboard"), prepared
/// once per aspect ratio a caller wants to support, rather than a single template forced to
/// auto-adapt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemplateFamily {
    pub name: String,
    pub variants: Vec<TemplateVariant>,
}

/// Why a [`TemplateFamily`] failed [`TemplateFamily::validate`].
#[derive(Debug, Clone, PartialEq)]
pub enum TemplateFamilyValidationError {
    /// A family with no variants at all has nothing a caller could ever apply.
    Empty,
    DuplicateAspectRatio(ExportAspectRatio),
    Variant {
        aspect_ratio: ExportAspectRatio,
        error: TemplateValidationError,
    },
}

impl std::fmt::Display for TemplateFamilyValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "template family has no variants"),
            Self::DuplicateAspectRatio(ratio) => {
                write!(f, "template family has more than one variant for {ratio:?}")
            }
            Self::Variant {
                aspect_ratio,
                error,
            } => {
                write!(f, "variant {aspect_ratio:?}: {error}")
            }
        }
    }
}

impl std::error::Error for TemplateFamilyValidationError {}

impl TemplateFamily {
    /// Checks every variant's own [`GraphicTemplate::validate`], plus this family's own
    /// constraints: at least one variant, and no aspect ratio repeated across variants (which
    /// aspect_ratio a caller should pick would otherwise be ambiguous).
    pub fn validate(&self) -> Result<(), TemplateFamilyValidationError> {
        if self.variants.is_empty() {
            return Err(TemplateFamilyValidationError::Empty);
        }
        // `ExportAspectRatio` doesn't derive `Hash`, and this crate's own convention is to reuse
        // shared types as-is rather than adding derives elsewhere for one caller's convenience —
        // a linear scan is plenty for the handful of variants a real template family ever has.
        let mut seen: Vec<ExportAspectRatio> = Vec::new();
        for variant in &self.variants {
            if seen.contains(&variant.aspect_ratio) {
                return Err(TemplateFamilyValidationError::DuplicateAspectRatio(
                    variant.aspect_ratio,
                ));
            }
            seen.push(variant.aspect_ratio);
            variant.template.validate().map_err(|error| {
                TemplateFamilyValidationError::Variant {
                    aspect_ratio: variant.aspect_ratio,
                    error,
                }
            })?;
        }
        Ok(())
    }

    /// The prepared variant for `aspect_ratio`, if this family has one — the lookup a `ui`-side
    /// "apply template to the current sequence" flow would use, matching the sequence's own
    /// [`crate::project::SequenceExportSettings::aspect_ratio`].
    pub fn variant_for(&self, aspect_ratio: ExportAspectRatio) -> Option<&GraphicTemplate> {
        self.variants
            .iter()
            .find(|v| v.aspect_ratio == aspect_ratio)
            .map(|v| &v.template)
    }
}

#[cfg(test)]
#[path = "motion_template/motion_template_test.rs"]
mod tests;
