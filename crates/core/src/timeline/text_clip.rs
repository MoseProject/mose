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

/// Bundled font family used by a [`TextClip`]. Every family is shipped with oca under the
/// SIL Open Font License, so projects render identically even when the host has no fonts
/// installed. The actual font bytes are parsed lazily by [`crate::text_metrics`].
///
/// **Persisted in `.ocproj` by [`crate::font_catalog`]'s stable `family_id` slug** (e.g.
/// `"bebas-neue"`), not the variant name — FONT-01A's own deferred persisted-identity swap,
/// now done. [`Serialize`]/[`Deserialize`] are implemented manually below rather than derived,
/// since the mapping isn't the derive-default "variant name in, variant name out": reading also
/// accepts the *old* format (a bare variant name, e.g. `"BebasNeue"` — what every `.ocproj`
/// saved before this migration actually contains) for backward compatibility, and a slug or
/// name this build recognizes neither of resolves to [`Self::Unknown`] rather than silently
/// normalizing to [`Self::Lato`] — FONT-01's forwards-compatibility rule in full this time
/// ("an unknown future ID... keeps its serialized value... renders with the deterministic
/// fallback"): [`Self::family_id`] returns `Unknown`'s own preserved string, so a build that
/// doesn't recognize a family still renders it as Lato locally *and* writes the exact same
/// unrecognized slug back out on save, rather than losing the association the way the old
/// `#[serde(other)]`-onto-`Lato` unit-variant scheme did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TextFontFamily {
    /// Condensed display face suitable for titles.
    BebasNeue,
    /// High-contrast serif display face.
    PlayfairDisplay,
    /// Casual handwritten face.
    PatrickHand,
    /// Monospaced face.
    AnonymousPro,
    /// Heavy, high-contrast face intended for short-form captions.
    ArchivoBlack,
    // FONT-01B's 37-family expansion (`crate::font_catalog::CATALOG`) — added directly as plain
    // enum variants, the same shape the original six use, rather than the doc's own
    // `family_id`-as-persisted-type structural swap (still deferred, see this enum's own doc
    // comment above): every variant here still serializes as its own name string, and `Lato`'s
    // `#[serde(other)]` below already gives forward-compatible fallback for any name an older
    // build doesn't recognize -- the practical "these fonts are selectable and persist
    // correctly" outcome doesn't need the full swap, only the *"unknown id keeps its exact
    // string through a resave"* half still does. Grouped by `font_catalog::FontCategory` order,
    // matching `CATALOG`'s own layout.
    Inter,
    Montserrat,
    Roboto,
    OpenSans,
    Poppins,
    Nunito,
    SourceSans3,
    Barlow,
    Fredoka,
    Oswald,
    Anton,
    BarlowCondensed,
    LeagueSpartan,
    Teko,
    BlackOpsOne,
    RussoOne,
    Bangers,
    Merriweather,
    LibreBaskerville,
    Lora,
    Cinzel,
    Bitter,
    Caveat,
    Pacifico,
    DancingScript,
    ComicNeue,
    GloriaHallelujah,
    JetBrainsMono,
    RobotoMono,
    SpaceMono,
    NotoSansArabic,
    NotoNaskhArabic,
    NotoSansHebrew,
    NotoSansDevanagari,
    NotoSansBengali,
    NotoSansTamil,
    NotoSansThai,
    /// Modern sans-serif suitable for body text and general captions. Also the deterministic
    /// fallback for a font family this build doesn't recognize (see this enum's own doc comment).
    #[default]
    Lato,
    /// A `family_id` (or, reading an old save, a variant name) this build doesn't recognize —
    /// preserves the exact original string so a resave doesn't lose it, while rendering/
    /// behaving as [`Self::Lato`] everywhere else (see this enum's own doc comment). Never a
    /// member of [`Self::ALL`] — it's a runtime/persistence state, not a selectable option.
    Unknown(String),
}

impl TextFontFamily {
    pub const ALL: [Self; 43] = [
        Self::Lato,
        Self::BebasNeue,
        Self::PlayfairDisplay,
        Self::PatrickHand,
        Self::AnonymousPro,
        Self::ArchivoBlack,
        Self::Inter,
        Self::Montserrat,
        Self::Roboto,
        Self::OpenSans,
        Self::Poppins,
        Self::Nunito,
        Self::SourceSans3,
        Self::Barlow,
        Self::Fredoka,
        Self::Oswald,
        Self::Anton,
        Self::BarlowCondensed,
        Self::LeagueSpartan,
        Self::Teko,
        Self::BlackOpsOne,
        Self::RussoOne,
        Self::Bangers,
        Self::Merriweather,
        Self::LibreBaskerville,
        Self::Lora,
        Self::Cinzel,
        Self::Bitter,
        Self::Caveat,
        Self::Pacifico,
        Self::DancingScript,
        Self::ComicNeue,
        Self::GloriaHallelujah,
        Self::JetBrainsMono,
        Self::RobotoMono,
        Self::SpaceMono,
        Self::NotoSansArabic,
        Self::NotoNaskhArabic,
        Self::NotoSansHebrew,
        Self::NotoSansDevanagari,
        Self::NotoSansBengali,
        Self::NotoSansTamil,
        Self::NotoSansThai,
    ];

    /// Whether this bundled family has a distinct bold file — derived from
    /// [`crate::font_catalog::CATALOG`] directly (does this family's own entry declare a weight
    /// 700 face?) rather than a hand-maintained list, so a newly vendored family's bold support
    /// is never forgotten. Every FONT-01B variable-font family currently locks only its default
    /// (400) instance (see `font_catalog`'s own doc comment on why), so this returns `false` for
    /// all of them today — not a missing case, an accurate reflection of what's actually locked.
    pub fn supports_bold(&self) -> bool {
        crate::font_catalog::find_family(self.family_id())
            .is_some_and(|entry| entry.faces.iter().any(|face| face.weight == 700))
    }
}

impl Serialize for TextFontFamily {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.family_id())
    }
}

impl<'de> Deserialize<'de> for TextFontFamily {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        // New format: a font_catalog family_id slug (e.g. "bebas-neue").
        if let Some(family) = Self::from_family_id(&raw) {
            return Ok(family);
        }
        // Old format, for backward compatibility with every .ocproj saved before this
        // migration: a bare enum variant name (e.g. "BebasNeue") -- Debug's derived output for
        // a unit variant is exactly its name, so comparing against that (rather than a second
        // hand-maintained name table) can't drift from the real variant names.
        if let Some(family) = Self::ALL.into_iter().find(|f| format!("{f:?}") == raw) {
            return Ok(family);
        }
        // Neither -- an id this build genuinely doesn't recognize (a newer family, or a
        // corrupted/foreign value). Preserved verbatim rather than normalized to Lato, per this
        // enum's own doc comment.
        Ok(Self::Unknown(raw))
    }
}

/// Style within a bundled [`TextFontFamily`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextFontStyle {
    #[default]
    Regular,
    Bold,
}

/// Paragraph base direction for a [`TextClip`] (TEXT-01B,
/// `spec/architecture/complex-text-shaping.md`) — UAX #9's own P2/P3 auto-detection by default,
/// or an explicit override for text whose script alone doesn't disambiguate direction (e.g. a
/// caption that is only digits/punctuation, or mixed RTL/LTR text where the author wants the
/// *paragraph's* level pinned regardless of which script happens to lead). Does not force every
/// character's direction like a bidi *override* would — numbers and embedded opposite-script
/// runs still follow ordinary UAX #9 resolution within that pinned paragraph level, same as CSS's
/// `direction` property (as opposed to `unicode-bidi: bidi-override`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextDirection {
    #[default]
    Auto,
    Ltr,
    Rtl,
}

/// Horizontal text alignment for a [`TextClip`] (TEXT-01B, `spec/architecture/
/// complex-text-shaping.md`'s "semantic alignment" goal, scoped down to plain visual Left/
/// Center/Right for this slice — see that doc's own TEXT-01B step 1 note). `Auto` (the default)
/// preserves this clip's exact pre-existing behavior byte-for-byte: [`TextClip::pos_x`] is the
/// text block's *left* edge, wrapping into the remaining space to the canvas's right edge, and
/// per-line alignment within that box falls back to `cosmic-text`'s own direction-aware default
/// (left for an LTR-detected paragraph, right for RTL). Choosing `Left`/`Center`/`Right`
/// explicitly instead **redefines what `pos_x` anchors**: the text block's left edge, horizontal
/// center, or right edge respectively — matching how most caption/design tools bind a position
/// handle to the alignment currently selected, at the cost of `pos_x` meaning something different
/// depending on this field (a real, deliberate trade-off, not an oversight — the alternative of
/// always aligning within a fixed canvas-wide box was rejected because it would make `pos_x`
/// silently stop mattering the moment alignment left `Auto`/`Left`).
///
/// `Start`/`End` are the doc's originally-scoped-down "semantic alignment" (CSS logical
/// properties, not physical ones): the leading/trailing edge of the *resolved* paragraph
/// direction — [`crate::text_layout::resolve_paragraph_direction`], the same UAX #9 P2/P3
/// first-strong-character detection [`TextDirection::Auto`] uses, applied to [`TextDirection::
/// Ltr`]/[`TextDirection::Rtl`] as the pinned direction when not `Auto`. `Start` anchors `pos_x`
/// at the left edge for a resolved-LTR paragraph and the right edge for resolved-RTL (mirroring
/// `Right`'s edge, not `Left`'s pixel position); `End` is the opposite. Deliberately resolved once
/// per render, not persisted as a physical edge, so a clip's alignment stays semantically correct
/// even if `direction` or the text's own leading script changes later.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Auto,
    Left,
    Center,
    Right,
    Start,
    End,
}

/// One placed text overlay on a [`super::Track`] whose [`super::TrackKind`] is
/// [`super::TrackKind::Text`]. Rasterized with the selected bundled font into an RGBA image, then
/// composited in a native post-processing pass after the main timeline encode — see
/// `avbridge::apply_text_overlays`.
///
/// Previewed via [`crate::overlay_render::render_text_clip_rgba`] and
/// the preview's timed raster overlays (one per caption segment, so a highlighted word is its
/// own segment and nothing is re-rasterized during playback). Preview and export filter the same
/// complete-caption `fontdue` layout by UTF-8 byte range, so automatic wrapping, explicit
/// newlines, font geometry, and background geometry remain identical.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TextClip {
    pub id: u64,
    /// Start time on the timeline, in seconds.
    pub start_secs: f64,
    /// How long the text stays visible, in seconds.
    pub duration_secs: f64,
    /// The text string to render.
    pub text: String,
    /// Font size in points.
    pub font_size: f32,
    /// Bundled font family. Defaults to Lato for projects saved before font selection existed.
    #[serde(default)]
    pub font_family: TextFontFamily,
    /// Font style. Families without a distinct bold file normalize this to `Regular` at render
    /// time, while keeping the persisted value harmless and forwards-compatible.
    #[serde(default)]
    pub font_style: TextFontStyle,
    /// TEXT-01C (`spec/architecture/complex-text-shaping.md`): an explicit OpenType `wght` axis
    /// value (typically `100..=900`) overriding `font_style`'s plain Regular/Bold choice for a
    /// [`FontSourceKind::Variable`](crate::font_catalog::FontSourceKind::Variable) family — every
    /// FONT-01B variable family locks one upstream variable TTF spanning a real weight range, not
    /// just its two named Regular/Bold instances. `None` (the default, and every project saved
    /// before this field existed) keeps the exact old `font_style`-only behavior; a static family
    /// has no `wght` axis at all, so this is harmless-but-meaningless there (cosmic-text's own
    /// font matching simply never finds an axis to apply it to). `#[serde(default)]` so an older
    /// saved project still loads.
    #[serde(default)]
    pub font_weight: Option<u16>,
    /// RGBA color: `[r, g, b, a]`, each 0–255. Alpha 255 = fully opaque.
    pub color_rgba: [u8; 4],
    /// RGBA color behind the complete laid-out text block. Alpha 0 disables the background.
    #[serde(default)]
    pub background_rgba: [u8; 4],
    /// Space in pixels between glyph bounds and the background edge.
    #[serde(default = "default_text_background_padding")]
    pub background_padding: f32,
    /// Rounded-corner radius in pixels for the background.
    #[serde(default = "default_text_background_corner_radius")]
    pub background_corner_radius: f32,
    /// Horizontal anchor as a 0.0–1.0 fraction of the canvas width (0.0 = left edge).
    pub pos_x: f32,
    /// Vertical anchor as a 0.0–1.0 fraction of the canvas height (0.0 = top edge).
    pub pos_y: f32,
    /// General keyframe animation for `pos_x`/`pos_y` over this clip's own on-timeline
    /// duration — the position/scale/rotation slice `spec/ROADMAP.md` P4 item 34 explicitly
    /// called out as not yet done, since `TextClip`'s export path pre-rasterizes a full-canvas
    /// RGBA PNG per segment (`crate::overlay_render::render_text_segment_rgba`) rather than
    /// `ShapeClip`'s self-contained `geq` expression, so the same "position is just a `geq`
    /// per-pixel formula" trick doesn't apply here. Ships instead as a pixel-offset `overlay`
    /// stage in `avbridge::apply_text_overlays`: the raster still bakes one constant anchor
    /// (the first keyframe's value when keyframes are present, `pos_x`/`pos_y` otherwise — same
    /// "keyframes win when present" convention `ShapeClip`'s own keyframe fields use) exactly
    /// as before, and `keyframe::text_position_offset_expr` builds the delta from that anchor as
    /// an `overlay=x=<expr>:y=<expr>` fragment instead of the always-`x=0:y=0` this filter graph
    /// used to have — no sprite-cropping restructuring needed, and an unkeyframed `TextClip`
    /// gets the exact same PNG/filter graph it always had. Each axis independently overrides its
    /// own constant when non-empty, same as `ShapeClip::center_x_keyframes`/`center_y_keyframes`
    /// (two separate `f32` lists, not one `Position`-typed list, for the same reason: this is an
    /// absolute `0.0..=1.0` anchor fraction, not `ClipInstance::position_keyframes`' `-1.0..=1.0`
    /// pan-offset convention). `#[serde(default)]` so an older saved project loads with no
    /// position animation. Export-only, like `opacity_keyframes` — no live preview effect
    /// (`render_text_clip_rgba` always draws at the plain `pos_x`/`pos_y` anchor).
    #[serde(default)]
    pub pos_x_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub pos_y_keyframes: Vec<Keyframe<f32>>,
    /// General keyframe animation for this block's overall size, as a multiplier (`1.0` =
    /// unscaled) — the last piece of P4 item 34's `TextClip` scope, same non-preview,
    /// export-only shape `pos_x_keyframes` has. Unlike position, this needed no raster-baking
    /// change at all: `keyframe::text_scale_sample_exprs` builds a `geq` inverse-sample remap
    /// around the raster's own baked position anchor (`pos_x`/`pos_y`, or `pos_x_keyframes`/
    /// `pos_y_keyframes`' first value when present) — an inverse zoom, not a literal `scale`
    /// filter with `eval=frame`, which reliably corrupted the heap in a real export elsewhere in
    /// this codebase when its output frame size varied per frame (see `CLAUDE.md`). Clamped to
    /// the same `0.1..=20.0` bounds [`crate::timeline::ClipInstance::scale_keyframes`] uses.
    /// `#[serde(default)]` so older saved projects load unscaled.
    #[serde(default)]
    pub scale_keyframes: Vec<Keyframe<f32>>,
    /// General keyframe animation for this block's own clockwise rotation, in degrees — the last
    /// piece of P4 item 34's `TextClip` scope. Same treatment as `scale_keyframes`: no
    /// raster-baking change needed, `keyframe::text_rotation_sample_exprs` builds a `geq`
    /// inverse-sample remap around the same raster-baked position anchor. Scale and rotation
    /// compose independently around that shared anchor with no interaction term needed (an
    /// isotropic scale and a rotation around the same center commute). `#[serde(default)]` so
    /// older saved projects load unrotated.
    #[serde(default)]
    pub rotation_keyframes: Vec<Keyframe<f32>>,
    /// Per-word timestamps within this clip's own text, `start_secs`/`end_secs` relative to
    /// this clip's *own* start (not the timeline) — per `request.md`'s Fase 4 "Legenda com
    /// destaque de palavra (estilo shorts)". Populated when this clip was generated from
    /// [`crate::transcribe::transcribe`]'s word-level output (`ui`'s "Transcrever" flow); empty
    /// for a manually-typed text block, which has no per-word timing to highlight against.
    /// `#[serde(default)]` so older saved projects load with no word highlighting.
    #[serde(default)]
    pub words: Vec<WordTiming>,
    /// Whether [`TextClip::words`] should be rendered as in-place word highlighting
    /// (`resolve_text_segments` emits one highlighted overlay per word, on top of the base
    /// text) rather than as plain static text. Meaningless while `words` is empty.
    /// `#[serde(default)]` so older saved projects load with highlighting off.
    #[serde(default)]
    pub highlight_enabled: bool,
    /// Color a word is drawn in while it's the one being spoken, `[r, g, b, a]` — meaningless
    /// while `highlight_enabled` is `false`. `#[serde(default = ..)]` so older saved projects
    /// load at a reasonable default (bright yellow, matching the popular shorts-caption look)
    /// rather than an invisible/transparent black.
    #[serde(default = "default_highlight_color")]
    pub highlight_color_rgba: [u8; 4],
    /// General opacity fade over this clip's own on-timeline duration — the first slice of P4
    /// item 34's `TextClip` scope, per `spec/ROADMAP.md` (position, scale, and rotation followed
    /// later, see `pos_x_keyframes`/`pos_y_keyframes`/`scale_keyframes`/`rotation_keyframes`).
    /// The raster already carries a real alpha channel (transparent background around the text/
    /// background box), so a fade is just an alpha *multiplier* applied to the existing pixels in
    /// `avbridge::apply_text_overlays`'s filter graph (`keyframe::text_opacity_alpha_expr`) —
    /// zero changes to the Rust-side rasterization or highlight-layout code this doc comment's
    /// sibling fields depend on. Empty means "no fade, same static visibility window as
    /// before" — the exact same filter graph an unanimated `TextClip` always had.
    /// `#[serde(default)]` so older saved projects load with no fade.
    #[serde(default)]
    pub opacity_keyframes: Vec<Keyframe<f32>>,
    /// Explicit paragraph base-direction override — see [`TextDirection`]. `#[serde(default)]`
    /// so older saved projects load as `Auto` (UAX #9 auto-detection), the exact behavior this
    /// clip already had before `TextDirection` existed.
    #[serde(default)]
    pub direction: TextDirection,
    /// Optional BCP-47-ish language tag (e.g. `"pt-BR"`, `"ar"`), persisted for a future shaping
    /// pass (TEXT-01C's international-fallback face selection) to consume — not yet read by
    /// [`crate::text_layout`] or [`crate::overlay_render`], since `cosmic-text` 0.19's `Attrs`
    /// has no language field to feed it into. A real, deliberately deferred gap, not silently
    /// dropped: kept here now so a project authored with this metadata doesn't need a second
    /// migration once shaping does consume it. `#[serde(default)]` so older saved projects load
    /// with no language hint.
    #[serde(default)]
    pub language: Option<String>,
    /// Horizontal text alignment — see [`TextAlign`]. `#[serde(default)]` so older saved projects
    /// load as `Auto`, the exact behavior this clip already had before `TextAlign` existed
    /// (`pos_x` as a plain left edge, direction-aware per-line fallback).
    #[serde(default)]
    pub text_align: TextAlign,
}

impl TextClip {
    /// The manually inserted default: white 48 pt Lato near the lower-left corner, lasting 3
    /// seconds from `start_secs`. Every shell's "add text" goes through this; `text` is passed in
    /// because the placeholder content is localized by the caller.
    pub fn new(id: u64, start_secs: f64, text: String) -> Self {
        Self {
            id,
            start_secs,
            duration_secs: 3.0,
            text,
            font_size: 48.0,
            font_family: TextFontFamily::default(),
            font_style: TextFontStyle::default(),
            font_weight: None,
            color_rgba: [255, 255, 255, 255],
            background_rgba: [0, 0, 0, 0],
            background_padding: default_text_background_padding(),
            background_corner_radius: default_text_background_corner_radius(),
            pos_x: 0.1,
            pos_y: 0.85,
            pos_x_keyframes: Vec::new(),
            pos_y_keyframes: Vec::new(),
            scale_keyframes: Vec::new(),
            rotation_keyframes: Vec::new(),
            words: Vec::new(),
            highlight_enabled: false,
            highlight_color_rgba: default_highlight_color(),
            opacity_keyframes: Vec::new(),
            direction: TextDirection::default(),
            language: None,
            text_align: TextAlign::default(),
        }
    }

    /// Switches the font family, dropping settings the new family can't honor: Bold falls back
    /// to Regular when the family has no bold face, and an explicit `wght` value is cleared for a
    /// static (non-variable) family. Returns whether anything changed.
    pub fn set_font_family(&mut self, family: TextFontFamily) -> bool {
        let before = (self.font_family.clone(), self.font_style, self.font_weight);
        let is_variable =
            crate::font_catalog::find_family(family.family_id()).is_some_and(|entry| {
                entry.source_kind == crate::font_catalog::FontSourceKind::Variable
            });
        if !family.supports_bold() {
            self.font_style = TextFontStyle::Regular;
        }
        if !is_variable {
            self.font_weight = None;
        }
        self.font_family = family;
        before != (self.font_family.clone(), self.font_style, self.font_weight)
    }
}

fn default_text_background_padding() -> f32 {
    8.0
}

fn default_text_background_corner_radius() -> f32 {
    8.0
}

/// One word within a [`TextClip`]'s [`TextClip::words`] — see
/// [`crate::transcribe::TranscribeWord`], which this mirrors (kept as a separate type since
/// `core::transcribe`'s output is a transient transcription result, not project-saved state).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WordTiming {
    pub text: String,
    /// Relative to the owning [`TextClip::start_secs`], not the timeline.
    pub start_secs: f64,
    /// Relative to the owning [`TextClip::start_secs`], not the timeline.
    pub end_secs: f64,
}

fn default_highlight_color() -> [u8; 4] {
    [255, 220, 0, 255]
}
