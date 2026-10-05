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

use serde::{Deserialize, Serialize};

/// Layer mask shape for a block, per `request.md`'s Fase 4 "Máscaras" spec — clips a layer to a
/// shape instead of the plain rectangular crop ([`ClipInstance::crop_x`] etc.), e.g. for webcam
/// frames. `Custom` (a user-drawn shape) isn't supported yet — only the two fixed shapes the
/// spec names alongside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MaskShape {
    #[default]
    None,
    Circle,
    RoundedRect,
}

/// Color filter for a block, per `request.md`'s Fase 4 "Efeitos visuais" spec ("Preto e branco
/// e sépia"). A bounded subset of the eventual "Filtros de cor e LUTs" library — just these two
/// fixed looks, no adjustable LUT yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ColorFilter {
    #[default]
    None,
    BlackAndWhite,
    Sepia,
}

/// Compositing blend mode for an overlay-track block, per
/// `spec/architecture/editor-ui-visual-redesign.md`'s Inspector section ("COMPOSITE → Blend
/// Mode") — the full mode set FFmpeg's `blend`/`tblend` filter supports (`all_mode`, verified
/// against a real `ffmpeg -h filter=blend` build rather than assumed from docs; two of
/// FFmpeg's own mode names are aliases sharing a numeric mode with another name already listed
/// here — `addition128`/`grainmerge` both mean mode 28, `difference128`/`grainextract` both
/// mean mode 7 — only one canonical name per mode is kept). `Normal` (the default) means "no
/// blend mode" — the clip composites via plain alpha-over `overlay`, exactly as before this
/// enum existed. Any other variant means the clip's whole overlay-track layer blends against
/// the accumulated canvas below it using that mode's per-pixel formula, at full canvas size —
/// see [`ClipInstance::blend_mode`]'s own doc comment for the position/PIP caveat this implies.
/// Only meaningful for a clip on an overlay track (track 1+ in a multi-track export/composited
/// preview) — a single/background track has no layer below it to blend against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum BlendMode {
    #[default]
    Normal,
    Addition,
    And,
    Average,
    Burn,
    Darken,
    Difference,
    GrainExtract,
    Divide,
    Dodge,
    Exclusion,
    HardLight,
    Lighten,
    Multiply,
    Negation,
    Or,
    Overlay,
    Phoenix,
    PinLight,
    Reflect,
    Screen,
    SoftLight,
    Subtract,
    VividLight,
    Xor,
    HardMix,
    LinearLight,
    Glow,
    GrainMerge,
    Multiply128,
    Heat,
    Freeze,
    Extremity,
    SoftDifference,
    Geometric,
    Harmonic,
    Bleach,
    Stain,
    Interpolate,
    HardOverlay,
}

impl BlendMode {
    /// The exact mode name FFmpeg's `blend` filter's `all_mode` option expects (`blend=all_mode=
    /// <name>`) — built into the overlay filtergraph by `avbridge`'s `timeline_export_multi.c`.
    /// `Normal` has no meaningful name here (callers gate on [`ClipInstance::has_blend_mode`]
    /// and use plain `overlay` instead of ever reaching this), but returns `"normal"` (a real,
    /// valid FFmpeg mode name — a no-op blend) rather than panicking, so a caller that calls
    /// this unconditionally still gets defined, harmless behavior.
    pub fn ffmpeg_name(self) -> &'static str {
        match self {
            BlendMode::Normal => "normal",
            BlendMode::Addition => "addition",
            BlendMode::And => "and",
            BlendMode::Average => "average",
            BlendMode::Burn => "burn",
            BlendMode::Darken => "darken",
            BlendMode::Difference => "difference",
            BlendMode::GrainExtract => "grainextract",
            BlendMode::Divide => "divide",
            BlendMode::Dodge => "dodge",
            BlendMode::Exclusion => "exclusion",
            BlendMode::HardLight => "hardlight",
            BlendMode::Lighten => "lighten",
            BlendMode::Multiply => "multiply",
            BlendMode::Negation => "negation",
            BlendMode::Or => "or",
            BlendMode::Overlay => "overlay",
            BlendMode::Phoenix => "phoenix",
            BlendMode::PinLight => "pinlight",
            BlendMode::Reflect => "reflect",
            BlendMode::Screen => "screen",
            BlendMode::SoftLight => "softlight",
            BlendMode::Subtract => "subtract",
            BlendMode::VividLight => "vividlight",
            BlendMode::Xor => "xor",
            BlendMode::HardMix => "hardmix",
            BlendMode::LinearLight => "linearlight",
            BlendMode::Glow => "glow",
            BlendMode::GrainMerge => "grainmerge",
            BlendMode::Multiply128 => "multiply128",
            BlendMode::Heat => "heat",
            BlendMode::Freeze => "freeze",
            BlendMode::Extremity => "extremity",
            BlendMode::SoftDifference => "softdifference",
            BlendMode::Geometric => "geometric",
            BlendMode::Harmonic => "harmonic",
            BlendMode::Bleach => "bleach",
            BlendMode::Stain => "stain",
            BlendMode::Interpolate => "interpolate",
            BlendMode::HardOverlay => "hardoverlay",
        }
    }

    /// Every variant, in the same order [`BlendMode::ffmpeg_name`] lists them — for a UI dropdown
    /// to iterate without hand-duplicating the list.
    pub const ALL: [BlendMode; 40] = [
        BlendMode::Normal,
        BlendMode::Addition,
        BlendMode::And,
        BlendMode::Average,
        BlendMode::Burn,
        BlendMode::Darken,
        BlendMode::Difference,
        BlendMode::GrainExtract,
        BlendMode::Divide,
        BlendMode::Dodge,
        BlendMode::Exclusion,
        BlendMode::HardLight,
        BlendMode::Lighten,
        BlendMode::Multiply,
        BlendMode::Negation,
        BlendMode::Or,
        BlendMode::Overlay,
        BlendMode::Phoenix,
        BlendMode::PinLight,
        BlendMode::Reflect,
        BlendMode::Screen,
        BlendMode::SoftLight,
        BlendMode::Subtract,
        BlendMode::VividLight,
        BlendMode::Xor,
        BlendMode::HardMix,
        BlendMode::LinearLight,
        BlendMode::Glow,
        BlendMode::GrainMerge,
        BlendMode::Multiply128,
        BlendMode::Heat,
        BlendMode::Freeze,
        BlendMode::Extremity,
        BlendMode::SoftDifference,
        BlendMode::Geometric,
        BlendMode::Harmonic,
        BlendMode::Bleach,
        BlendMode::Stain,
        BlendMode::Interpolate,
        BlendMode::HardOverlay,
    ];
}

/// Transition style for a block's incoming edge, per `request.md`'s Fase 4 "Efeitos visuais"
/// spec ("Transições entre clipes (fade, corte seco, slide, zoom)"). `HardCut` is the spec's
/// "corte seco" spelled out as an explicit choice, distinct from `None` meaning "no transition
/// configured yet" — both currently render identically (nothing renders either), but they mean
/// different things to the user's edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TransitionType {
    #[default]
    None,
    Fade,
    HardCut,
    Slide,
    Zoom,
    /// A cross-dissolve with the clip before this one: the clip overlaps the previous one by
    /// the transition duration and fades in over it (see `Track::overlap_lanes`).
    Dissolve,
}

impl TransitionType {
    /// Maps this transition to the integer code used in [`avbridge::ClipSegment::transition_in`]
    /// and the `ClipSegment.transition_in` C field. `None` and `HardCut` both produce `0`
    /// (no-op filter), since both mean an instant cut at the C level.
    pub fn to_export_code(self) -> u8 {
        match self {
            Self::None | Self::HardCut => 0,
            Self::Fade => 1,
            Self::Slide => 2,
            // A dissolve is a fade in over the clip below, resolved into a fade handle by
            // `render::segments`, not an entry effect of its own.
            Self::Dissolve => 0,
            Self::Zoom => 3,
        }
    }
}
