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

use crate::keyframe::{self, Keyframe, Position};
use crate::timeline::TrackKind;

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
            Self::Zoom => 3,
        }
    }
}

/// One placed instance of a `MediaAsset` on the timeline. `source_in_secs`/`source_out_secs`
/// mark the trimmed range within the source asset; `start_secs` is its position on the track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipInstance {
    pub id: u64,
    pub asset_id: u64,
    pub start_secs: f64,
    pub source_in_secs: f64,
    pub source_out_secs: f64,
    /// `Some(sequence_id)` if this clip is a **compound clip** (nested sequence) — its content
    /// comes from rendering another [`crate::project::Sequence`] in the same
    /// [`crate::project::Project`], not `asset_id` (ignored when this is set; kept at whatever
    /// stale/placeholder value it had, same as other fields a clip kind doesn't use). Per
    /// `spec/ROADMAP.md`'s "nested sequences / compound clips" item — creates a new `Sequence`
    /// out of a selection ([`App::create_compound_clip_from_selection`] in `ui`), moves the
    /// selected clips into it, and drops this clip in their place. `source_in_secs`/
    /// `source_out_secs` trim into the *rendered* nested timeline (0-based, same convention as
    /// an ordinary asset), not the nested `Sequence`'s own internal timeline positions.
    /// [`crate::nested_sequence::materialize_nested_sequences`] resolves this recursively (a
    /// nested sequence's own clips may themselves be nested) to a temp rendered file before
    /// export/preview, cached and reused unless that sequence's own timeline content changes.
    /// `#[serde(default)]` so older saved projects load with every clip asset-backed, unchanged.
    #[serde(default)]
    pub nested_sequence_id: Option<u64>,
    /// `Some(group_id)` if this clip is a member of a composite block (per `request.md`'s
    /// Fase 3 "blocos compostos" spec) — every clip sharing the same id, always on the same
    /// track (composite blocks don't span tracks yet), moves/splits/deletes together as a
    /// unit (see `ui`'s `App::merge_into_composite` and the timeline panel's drag/delete
    /// handling). `#[serde(default)]` so a project saved before this field existed still
    /// loads, every clip in it just standalone (`None`).
    #[serde(default)]
    pub composite_id: Option<u64>,
    /// Optional RGB color label for this block, per `matrix/competitor-parity.md`'s 2026-08-27
    /// update (`spec/ROADMAP.md` P4 item 27) — a purely cosmetic at-a-glance organization aid
    /// (Premiere's clip labels, DaVinci's clip *and* track color), painted as the timeline
    /// block's fill color in place of its usual kind-based color when set. `None` = use the
    /// usual coloring. `#[serde(default)]` so older saved projects load with no label.
    #[serde(default)]
    pub color_label: Option<[u8; 3]>,
    /// Volume adjustment in decibels applied to this block's audio, independent of every other
    /// clip — per `request.md`'s Fase 4 "ganho de volume por bloco" spec. `0.0` is unity gain.
    /// Feeds the timeline waveform display (`ui`'s `draw_waveform`, scaled by
    /// [`ClipInstance::gain_linear`]) and is wired into export — resolved to
    /// `avbridge::ClipSegment::gain_db` and applied as a `volume=<gain>dB` audio filter stage
    /// (`timeline_export.c`/`timeline_export_multi.c`). Also wired into preview now
    /// (`crate::preview::build_audio_filter_bin`/`Preview::open_composited`'s own audio chain,
    /// both via GStreamer's `volume` element — a linear scale factor, so `gain_db` converts via
    /// `preview::gain_db_to_linear`, unlike avfilter's own `volume=<gain>dB` string option).
    /// `#[serde(default)]` so older saved projects load at unity gain.
    #[serde(default)]
    pub gain_db: f32,
    /// `true` if this block is frozen — holds a single still frame
    /// ([`ClipInstance::source_in_secs`]) for its whole displayed duration instead of playing
    /// through the trimmed source range, per `request.md`'s Fase 4 "Congelar" spec ("segura um
    /// quadro específico por uma duração configurável"). The held frame is picked by trimming
    /// `source_in_secs` to it; the hold duration is just the block's existing on-timeline
    /// length ([`ClipInstance::duration_secs`]), adjustable the same way as any other clip via
    /// the existing trim handles — freeze doesn't need its own duration field. Changes how the
    /// timeline draws the block (`ui`'s timeline panel shows a single repeated poster frame
    /// instead of a filmstrip, per-position thumbnails) and is now wired into export — resolved
    /// straight to [`avbridge::ClipSegment::frozen`] (`crate::render::resolve_timeline_segments`)
    /// rather than through [`ClipInstance::video_filter_chain`], since holding a frame needs the
    /// segment's own decode loop to synthesize duplicate frames, not a static avfilter string.
    /// Audio is unaffected — a frozen block's audio still plays across its full trimmed range.
    /// Still doesn't affect preview playback (`avcore::preview::Preview` always plays the real
    /// decoded source) — the same kind of preview gap as [`ClipInstance::gain_db`].
    /// `#[serde(default)]` so older saved projects load unfrozen.
    #[serde(default)]
    pub frozen: bool,
    /// Playback speed multiplier for this block — `2.0` plays twice as fast, `0.5` half speed,
    /// per `request.md`'s Fase 4 "Velocidade" spec. `1.0` is normal speed. Already factored into
    /// [`ClipInstance::duration_secs`] (the trimmed source range divided by speed, so the
    /// timeline block's own length reflects the sped-up/slowed-down result), shown as a badge
    /// on the timeline block (`ui`'s timeline panel), and wired into export: resolved to
    /// `avbridge::ClipSegment::speed_factor`, applied as `setpts=PTS/<speed>` on video and
    /// `atempo` on audio (`timeline_export.c`/`timeline_export_multi.c`). No live preview effect
    /// yet. `#[serde(default = ..)]` so older saved projects load at normal speed. When
    /// [`ClipInstance::speed_ramp_end_factor`] is `Some`, this field is instead the ramp's
    /// *start* speed — see that field's own doc comment.
    #[serde(default = "default_speed_factor")]
    pub speed_factor: f32,
    /// End speed of a smooth, continuous speed ramp across this clip's whole trimmed duration —
    /// `None` (the default) means plain constant `speed_factor`, unchanged. `Some(end)` means
    /// `speed_factor` is the ramp's *start* speed and this is its end speed, linearly
    /// interpolated in speed (not in output-time) over the clip's own trimmed source duration —
    /// per `spec/ROADMAP.md` P4 item 29's "smooth continuous curve" follow-up to the earlier
    /// stepped approximation. Export resolves this to a `setpts` expression that's the
    /// *integral* of `1/speed(t)` (a natural-log term, since speed is linear in `t`) rather than
    /// splitting the clip into discrete pieces — see
    /// [`crate::keyframe::smooth_speed_ramp_duration_secs`] for the matching duration formula
    /// [`ClipInstance::duration_secs`] uses, and
    /// `avbridge::ClipSegment::smooth_speed_ramp_end_factor`/`timeline_export.c`'s `setpts_str`
    /// construction for the export-side expression. `#[serde(default)]` so older saved projects
    /// load with no ramp (plain `speed_factor`, unchanged behavior).
    #[serde(default)]
    pub speed_ramp_end_factor: Option<f32>,
    /// Normalized crop rectangle within the source frame — `(crop_x, crop_y)` is the visible
    /// sub-rectangle's top-left corner, `(crop_w, crop_h)` its size, all fractions of the full
    /// frame (`0.0..=1.0`). Defaults to `(0.0, 0.0, 1.0, 1.0)` — the whole frame, uncropped —
    /// per `request.md`'s Fase 4 "Recorte (crop)" spec: reframing separate from the time-based
    /// split already covered in Fase 3. Shown as a badge on the timeline block (`ui`'s timeline
    /// panel, via [`ClipInstance::is_cropped`]) and wired into export ([`ClipInstance::
    /// video_filter_chain`]'s `crop=...` stage); no live preview effect yet. Independently
    /// clamped to `[0.0, 1.0]` when set (`ui`'s `App::set_selected_clip_crop`); a crop rect
    /// extending past the frame edge (`crop_x + crop_w > 1.0`) isn't rejected here — a known
    /// simplification, not verified against how `ffmpeg`'s own `crop` filter behaves on an
    /// out-of-bounds rectangle at render time. `#[serde(default = ..)]` so older saved projects
    /// load uncropped.
    #[serde(default)]
    pub crop_x: f32,
    #[serde(default)]
    pub crop_y: f32,
    #[serde(default = "default_crop_extent")]
    pub crop_w: f32,
    #[serde(default = "default_crop_extent")]
    pub crop_h: f32,
    /// General keyframe animation for this block's crop rectangle over time (a moving/resizing
    /// pan window), per the keyframe-expansion gap found while surveying what else the existing
    /// keyframe system could drive (`spec/ROADMAP.md` P4 item 33) — independent of
    /// [`ClipInstance::scale_keyframes`]'s Ken-Burns zoom (a single symmetric zoom factor about
    /// the frame center), this animates all four crop axes independently. Each field is
    /// independent: a non-empty list overrides that axis's own constant field above (same
    /// "keyframes win when present" relationship [`ClipInstance::gain_keyframes`] has with
    /// `gain_db`). Wired into export ([`crate::keyframe::crop_filter_expr`], a `geq`-based
    /// per-pixel approach — see that function's own doc comment for why, over `crop`+
    /// `eval=frame`), spliced into [`ClipInstance::keyframe_video_filter_chain`] alongside
    /// scale/rotation/opacity/color-balance rather than [`ClipInstance::video_filter_chain`]'s
    /// own static `crop` stage, which this field being non-empty on any axis suppresses instead
    /// of double-emitting (same pattern [`ClipInstance::brightness_keyframes`] established for
    /// the static `eq` stage). Not yet wired into live preview. `#[serde(default)]` so older
    /// saved projects load with no crop animation (using the constant fields as before).
    #[serde(default)]
    pub crop_x_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub crop_y_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub crop_w_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub crop_h_keyframes: Vec<Keyframe<f32>>,
    /// Layer mask shape ([`MaskShape::None`] by default — unmasked). Independent of the
    /// rectangular crop above; a block can be both cropped and masked. Wired into export
    /// ([`ClipInstance::video_filter_chain`]'s `geq`-based alpha stage) but, like
    /// [`ClipInstance::chroma_key_enabled`], only has a visible effect on a clip placed on an
    /// **overlay track** — a single/background track's final `format=yuv420p` conform drops
    /// the alpha plane it produces. Also wired into preview
    /// (`crate::preview::build_mask_shape_stage`'s `alphacombine` stage, fed a static
    /// `crate::overlay_render::render_mask_shape_gray8` buffer instead of a decoded file), same
    /// overlay-only gate. `#[serde(default)]` so older saved projects load unmasked.
    #[serde(default)]
    pub mask_shape: MaskShape,
    /// Corner radius for [`MaskShape::RoundedRect`], as a fraction (`0.0..=1.0`) of the block's
    /// shorter frame dimension — meaningless for the other shapes. `#[serde(default)]` so older
    /// saved projects load at `0.0` (square corners).
    #[serde(default)]
    pub mask_corner_radius: f32,
    /// `true` if this block's frame is mirrored horizontally, per `request.md`'s Fase 4
    /// "Efeitos visuais" spec ("Espelhar (flip horizontal)"). Shown as a badge on the timeline
    /// block (`ui`'s timeline panel) and wired into export ([`ClipInstance::video_filter_chain`]'s
    /// `hflip` stage); no live preview effect yet. `#[serde(default)]` so older saved projects
    /// load unflipped.
    #[serde(default)]
    pub flipped_h: bool,
    /// Color filter applied to this block ([`ColorFilter::None`] by default). Shown as a
    /// tinted timeline-block fill (`ui`'s timeline panel) and wired into export
    /// ([`ClipInstance::video_filter_chain`]'s `hue=s=0`/`colorchannelmixer` stage); no live
    /// preview effect yet. `#[serde(default)]` so older saved projects load unfiltered.
    #[serde(default)]
    pub color_filter: ColorFilter,
    /// Vignette strength for this block, `0.0..=1.0` (`0.0` is off) — per `request.md`'s Fase 4
    /// "Efeitos visuais" spec ("Vinheta"). Shown as a darkened border stroke around the timeline
    /// block, scaled by intensity (`ui`'s timeline panel), and wired into export
    /// ([`ClipInstance::video_filter_chain`]'s `vignette` stage); no live preview effect yet.
    /// `#[serde(default)]` so older saved projects load with no vignette.
    #[serde(default)]
    pub vignette_intensity: f32,
    /// Brightness adjustment for this block, `-1.0..=1.0` (`0.0` is unchanged) — per
    /// `request.md`'s Fase 4 "Efeitos visuais" spec ("Brilho, contraste e saturação"). Wired
    /// into export ([`ClipInstance::video_filter_chain`]'s `eq=brightness=...` stage, combined
    /// with [`ClipInstance::contrast`]/[`ClipInstance::saturation`] into one `eq` filter); no
    /// live preview effect yet. `#[serde(default)]` so older saved projects load unchanged.
    #[serde(default)]
    pub brightness: f32,
    /// Contrast multiplier for this block, `0.0..=2.0` (`1.0` is unchanged) — same spec and
    /// export wiring as [`ClipInstance::brightness`]. `#[serde(default = ..)]` so older saved
    /// projects load unchanged.
    #[serde(default = "default_unity_multiplier")]
    pub contrast: f32,
    /// Saturation multiplier for this block, `0.0..=2.0` (`1.0` is unchanged, `0.0` is
    /// grayscale) — same spec and export wiring as [`ClipInstance::brightness`].
    /// `#[serde(default = ..)]` so older saved projects load unchanged.
    #[serde(default = "default_unity_multiplier")]
    pub saturation: f32,
    /// General keyframe animation for this block's brightness/contrast/saturation over time, per
    /// the keyframe-expansion gap found while surveying what else the existing keyframe system
    /// could drive (`spec/ROADMAP.md` P4 item 32). Each field is independent: a non-empty list
    /// overrides that axis's own constant field above (same "keyframes win when present"
    /// relationship [`ClipInstance::gain_keyframes`] has with `gain_db`); an axis left empty
    /// keeps using its constant. Wired into export
    /// ([`crate::keyframe::color_balance_filter_expr`], spliced into
    /// [`ClipInstance::keyframe_video_filter_chain`] alongside scale/rotation/opacity rather
    /// than [`ClipInstance::video_filter_chain`]'s own static `eq` stage, which this field being
    /// non-empty on any axis suppresses instead of double-emitting) — a real, narrow ordering
    /// caveat: the animated `eq` stage runs at the *front* of the per-clip filter chain (with
    /// scale/rotation/opacity) rather than its usual position after crop/deflicker/
    /// stabilization, so a clip combining color-grading keyframes with any of those three sees
    /// its color grading applied to the pre-crop/pre-deflicker/pre-stabilization frame instead.
    /// Not yet wired into live preview. `#[serde(default)]` so older saved projects load with no
    /// color-grading animation (using the constant fields as before).
    #[serde(default)]
    pub brightness_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub contrast_keyframes: Vec<Keyframe<f32>>,
    #[serde(default)]
    pub saturation_keyframes: Vec<Keyframe<f32>>,
    /// Sharpen strength for this block, `0.0..=1.0` (`0.0` is off) — per `request.md`'s Fase 4
    /// "Efeitos visuais" spec ("Nitidez (sharpen)"). Wired into export
    /// ([`ClipInstance::video_filter_chain`]'s `unsharp` stage); no live preview effect yet.
    /// `#[serde(default)]` so older saved projects load unsharpened.
    #[serde(default)]
    pub sharpen: f32,
    /// `true` if chroma key (green-screen removal) is enabled for this block, per
    /// `request.md`'s Fase 4 "Efeitos visuais" spec ("Chroma key"). Wired into export
    /// ([`ClipInstance::video_filter_chain`]'s `colorkey` stage) but only has a visible effect
    /// on a clip placed on an **overlay track** — a single/background track's final
    /// `format=yuv420p` conform drops the alpha plane this produces (same caveat
    /// [`ClipInstance::mask_shape`] carries above; see CLAUDE.md's "Alpha/overlay-track
    /// caveat"). No live preview effect yet. `#[serde(default)]` so older saved projects load
    /// disabled.
    #[serde(default)]
    pub chroma_key_enabled: bool,
    /// The key color to remove, as `[r, g, b]` (`0..=255` each) — meaningless while
    /// [`ClipInstance::chroma_key_enabled`] is `false`. `#[serde(default = ..)]` so older saved
    /// projects load at the conventional chroma-green `#00FF00`.
    #[serde(default = "default_chroma_key_color")]
    pub chroma_key_color: [u8; 3],
    /// How close a pixel's color must be to [`ClipInstance::chroma_key_color`] to be keyed out,
    /// `0.0..=1.0` (`0.0` is exact-match-only, `1.0` keys everything) — meaningless while
    /// `chroma_key_enabled` is `false`. `#[serde(default = ..)]` so older saved projects load at
    /// a reasonable default tolerance.
    #[serde(default = "default_chroma_key_tolerance")]
    pub chroma_key_tolerance: f32,
    /// Blur strength for this block, `0.0..=1.0` (`0.0` is off) — per `request.md`'s Fase 4
    /// "Efeitos visuais" spec ("Blur"). Wired into export ([`ClipInstance::video_filter_chain`]'s
    /// `boxblur` stage); no live preview effect yet. `#[serde(default)]` so older saved projects
    /// load unblurred.
    #[serde(default)]
    pub blur_intensity: f32,
    /// Camera-shake strength for this block, `0.0..=1.0` (`0.0` is off) — per `request.md`'s
    /// Fase 4 "Efeitos visuais" spec ("Shake"), the deliberate counterpart of the eventual video
    /// stabilization feature. Wired into export and preview (unlike most of this struct's other
    /// effect fields) — see [`ClipInstance::video_filter_chain`]'s `crop`+`scale` oscillation
    /// stage for export and `build_video_filter_bin` for the preview element. `#[serde(default)]`
    /// so older saved projects load unshaken.
    #[serde(default)]
    pub shake_intensity: f32,
    /// Glitch strength for this block, `0.0..=1.0` (`0.0` is off) — per `request.md`'s Fase 4
    /// "Efeitos visuais" spec ("Glitch"). Wired into export ([`ClipInstance::video_filter_chain`]'s
    /// `noise` stage); no live preview effect yet, unlike [`ClipInstance::shake_intensity`].
    /// `#[serde(default)]` so older saved projects load unglitched.
    #[serde(default)]
    pub glitch_intensity: f32,
    /// Pixelize/mosaic-censor strength for this block, `0.0..=1.0` (`0.0` is off) — per
    /// `request.md`'s Fase 4 "Efeitos visuais" spec ("Pixelizar/censura (mosaico)"). Wired into
    /// both export ([`ClipInstance::video_filter_chain`]'s scale-down/scale-up stage) and
    /// preview (`build_video_filter_bin`). `#[serde(default)]` so older saved projects load
    /// unpixelized.
    #[serde(default)]
    pub pixelize_intensity: f32,
    /// Transition style for this block's incoming edge ([`TransitionType::None`] by default) —
    /// per `request.md`'s Fase 4 "Efeitos visuais" spec ("Transições entre clipes"). Models
    /// only the transition entering this clip, not a real cross-blend between two adjacent
    /// clips — that would need a relationship between this clip and the one before it, not a
    /// field on a single `ClipInstance`. A deliberately smaller first cut, same shape as the
    /// rest of this struct's effect fields. Wired into export — resolved to `avbridge::
    /// ClipSegment::transition_in`, rendered as a fade/slide/zoom applied over
    /// [`ClipInstance::transition_duration_secs`] at the start of the clip's own filter chain
    /// (`timeline_export.c`/`timeline_export_multi.c`); no live preview effect yet.
    /// `#[serde(default)]` so older saved projects load with no transition.
    #[serde(default)]
    pub transition_in: TransitionType,
    /// Duration in seconds of [`ClipInstance::transition_in`], meaningless while it's
    /// `TransitionType::None`. `#[serde(default = ..)]` so older saved projects load at a
    /// reasonable default duration.
    #[serde(default = "default_transition_duration")]
    pub transition_duration_secs: f32,
    /// General keyframe animation for this block's position (translate offset, normalized as a
    /// fraction of canvas width/height), per `features/request.md`'s Fase 4 "Keyframes" spec.
    /// Empty = no offset. Wired into export (`crate::keyframe::position_overlay_xy_expr`) —
    /// only has a visible effect on an overlay-track clip, since a single/background track has
    /// no compositing stage to translate into (same caveat as `chroma_key_enabled`'s alpha).
    /// Not yet wired into live preview. `#[serde(default)]` so older saved projects (or a
    /// project saved before this field existed) load with no position animation.
    #[serde(default)]
    pub position_keyframes: Vec<Keyframe<Position>>,
    /// General keyframe animation for this block's scale, per `features/request.md`'s Fase 4
    /// "Keyframes" spec — supersedes the old two-endpoint `zoom_start`/`zoom_end` Ken-Burns
    /// fields (a 2-keyframe list reproduces that same behavior as a degenerate case). Empty =
    /// no scaling (`1.0`). Wired into export (`crate::keyframe::scale_filter_expr`) and into
    /// live preview (`core::preview::build_video_filter_bin`'s `videocrop`+`videoscale`+
    /// `capsfilter` chain, re-evaluated per buffer off its own PTS). `#[serde(default)]` so
    /// older saved projects load unscaled — a project that had real `zoom_start`/`zoom_end`
    /// values loses that animation on load, since this field replaces rather than migrates it
    /// (no back-compat promised for this format).
    #[serde(default)]
    pub scale_keyframes: Vec<Keyframe<f32>>,
    /// General keyframe animation for this block's rotation, in degrees, per
    /// `features/request.md`'s Fase 4 "Keyframes" spec. Empty = no rotation (`0.0`). Wired into
    /// export (`crate::keyframe::rotation_filter_angle_expr`) and into live preview
    /// (`core::preview::build_video_filter_bin`'s `rotate` element, its `angle` property
    /// re-evaluated per buffer). `#[serde(default)]` so older saved projects load unrotated.
    #[serde(default)]
    pub rotation_keyframes: Vec<Keyframe<f32>>,
    /// General keyframe animation for this block's opacity, `0.0..=1.0`, per
    /// `features/request.md`'s Fase 4 "Keyframes" spec. Empty = fully opaque (`1.0`). Wired into
    /// export (`crate::keyframe::opacity_alpha_ramp_expr`) — only has a visible effect on an
    /// overlay-track clip at export time, same caveat as position above — and into live preview
    /// too (`core::preview::build_video_filter_bin`'s `alpha` element), where it's visible on
    /// any clip regardless of track, since the preview's fixed RGBA output already supports
    /// alpha blending directly rather than needing export's overlay-compositing stage.
    /// `#[serde(default)]` so older saved projects load fully opaque.
    #[serde(default)]
    pub opacity_keyframes: Vec<Keyframe<f32>>,
    /// General keyframe animation for this block's audio gain, in **dB** (same unit as
    /// [`ClipInstance::gain_db`]), per the keyframe-expansion gap found while surveying what
    /// else the existing keyframe system could drive (`spec/ROADMAP.md` P4 item 31). Empty =
    /// use the constant [`ClipInstance::gain_db`] unchanged (this field, when non-empty,
    /// overrides that constant rather than combining with it — same "one or the other, not
    /// both" relationship `scale_keyframes` has with the old `zoom_start`/`zoom_end`). Wired
    /// into export (`crate::keyframe::gain_filter_db_expr`, via `avbridge::AudioSegment::
    /// gain_keyframe_expr`) — FFmpeg's `volume` filter's `eval=frame` expression mode is a
    /// linear multiplier, not dB, so the expression wraps each interpolated dB value in
    /// `pow(10,X/20)`. Not yet wired into live preview — same "export first" shape several
    /// other keyframe fields on this struct started with. `#[serde(default)]` so older saved
    /// projects load with no gain animation (using the constant `gain_db` as before).
    #[serde(default)]
    pub gain_keyframes: Vec<Keyframe<f32>>,
    /// `true` to run this block's audio through CF-02's "Gameplay Voice" cleanup chain before
    /// mixing — `highpass=f=80,afftdn=nf=<noise_floor>,acompressor=threshold=<threshold>dB:
    /// ratio=<ratio>:attack=10:release=250:makeup=1.5,alimiter=limit=<ceiling>` (values from
    /// `scripts/Watch-Gameplay.ps1`'s own proven chain — `spec/architecture/
    /// competitive-feature-plan.md`'s CF-03), applied per clip in `avbridge::audio_mix`'s
    /// per-branch filter graph, ahead of the existing final `afftdn`/`loudnorm`/`alimiter`
    /// mastering pass every export already runs on the finished mix. Meant for `Mic`-role
    /// tracks, but this is a plain per-clip toggle regardless of role — the "Mic by default,
    /// explicit override elsewhere" gate is a `ui` selector concern, not a data constraint.
    /// Non-destructive/reversible like every other effect toggle on this struct. `#[serde(
    /// default)]` so older saved projects load with it off. Not yet wired into live preview —
    /// export only, same "export first" shape several other effect fields here started with.
    #[serde(default)]
    pub voice_cleanup_enabled: bool,
    /// `afftdn`'s `nf` (expected noise floor), in dB — more negative removes less noise floor
    /// (afftdn's own convention: `nf` is where it expects the *noise* to sit, not a cut amount).
    /// `#[serde(default = ..)]` matches the proven script default.
    #[serde(default = "default_voice_cleanup_noise_floor_db")]
    pub voice_cleanup_noise_floor_db: f32,
    /// `acompressor`'s `threshold`, in dB — audio above this level gets compressed.
    #[serde(default = "default_voice_cleanup_compressor_threshold_db")]
    pub voice_cleanup_compressor_threshold_db: f32,
    /// `acompressor`'s `ratio` — how strongly audio above the threshold is compressed (higher =
    /// stronger leveling).
    #[serde(default = "default_voice_cleanup_compressor_ratio")]
    pub voice_cleanup_compressor_ratio: f32,
    /// `alimiter`'s `limit`, linear (not dB) — matches this chain's own `alimiter` stage and the
    /// unrelated final-mastering `alimiter` stage's own convention (both `0.0..=1.0`).
    #[serde(default = "default_voice_cleanup_ceiling_linear")]
    pub voice_cleanup_ceiling_linear: f32,
    /// Path to a `.cube` 3D LUT file applied to this block's color grading, per `request.md`'s
    /// Fase 4 "Filtros de cor e LUTs" spec. Empty string = no LUT (the FFI-friendly analog of
    /// `Option<PathBuf>` this codebase already uses for other optional string fields, since a
    /// plain `String` round-trips through `.ocproj`'s MessagePack struct-map encoding without
    /// needing an `Option` variant on the wire). Wired into export via `video_filter_chain`'s
    /// `lut3d` stage; no equivalent GStreamer element exists on this dev machine's install (no
    /// `lut3d`/`gllut3d`/cube-file element turned up in a real `gst-inspect-1.0` listing), so
    /// preview has no LUT stage — the same "export only" gap several other effects here have,
    /// just for a different reason (missing element, not "not wired yet"). `#[serde(default)]`
    /// so older saved projects load with no LUT applied.
    #[serde(default)]
    pub lut_path: String,
    /// Layer footprint size, as a multiplier of this clip's own native decoded width/height —
    /// `1.0` (both axes) is native size, unchanged. Independent axes allow a deliberate
    /// non-uniform stretch, not just uniform scaling, per `request.md`'s Fase 4 "Transformação
    /// de camadas" spec ("largura, altura... ajustáveis"). A *different* concept from
    /// [`ClipInstance::scale_keyframes`]'s Ken-Burns zoom, which crops into and rescales back to
    /// the *same* frame size (a zoom-in-place on the content) — this instead genuinely resizes
    /// the frame buffer that gets composited, shrinking or growing the clip's on-canvas
    /// footprint. Wired into export via a `scale=iw*x:ih*y` avfilter stage appended after every
    /// other per-clip stage (`crate::render::resolve_clip_filters`) — only meaningful on an
    /// overlay track (track 1+ in `avbridge_encode_timeline_export_multi`): a single/background
    /// track's final canvas-size conform has no pad/fit step, so shrinking there would produce
    /// a mismatched-resolution frame rather than a smaller picture with visible canvas around
    /// it, the same overlay-only caveat `position_keyframes`/`opacity_keyframes` already have,
    /// just for a correctness reason instead of a compositing one. Not yet wired into live
    /// preview. `#[serde(default = ..)]` so older saved projects load at native size.
    #[serde(default = "default_unity_multiplier")]
    pub layer_scale_x: f32,
    #[serde(default = "default_unity_multiplier")]
    pub layer_scale_y: f32,
    /// `true` if temporal luminance-flicker removal is enabled for this block, per
    /// `request.md`'s Fase 4 "Efeitos visuais" spec ("Remoção de flicker") — common in
    /// screen/gameplay captures at certain refresh rates. Wired to export via `video_filter_chain`
    /// (`deflicker`); no equivalent stage exists in `core::preview`'s GStreamer
    /// `build_video_filter_bin`, but [`crate::apply_deflicker_to_rgba`] covers it as a CPU-side
    /// preview approximation instead (see `preview_effects`'s own doc comment) — the live
    /// preview and the export encode still use two different implementations of the same idea,
    /// not one shared code path. `#[serde(default)]` so older saved projects load with it off.
    #[serde(default)]
    pub deflicker_enabled: bool,
    /// Video stabilization strength for this block, `0.0..=1.0` (`0.0` is off) — per
    /// `request.md`'s Fase 4 "Efeitos visuais" spec ("Estabilização de vídeo"), the deliberate
    /// opposite of [`ClipInstance::shake_intensity`] (which adds tremido on purpose; this
    /// removes it from footage that already has it). Wired into export via `video_filter_chain`'s
    /// `deshake` stage — `libavfilter`'s built-in single-pass stabilizer, not the more capable
    /// two-pass `vidstabdetect`/`vidstabtransform` pair (`libvidstab`), which this project's
    /// pinned FFmpeg build doesn't have compiled in (confirmed via a real `ffmpeg -buildconf`,
    /// not assumed — it explicitly lists `--disable-libvidstab`). No GStreamer element for this
    /// exists on this dev machine's install either (confirmed via `gst-inspect-1.0`, same as
    /// `lut_path`'s caveat), so this is export-only, same shape as LUTs. `#[serde(default)]` so
    /// older saved projects load unstabilized.
    #[serde(default)]
    pub stabilization_intensity: f32,
    /// `true` once [`ClipInstance::background_removal_mask_path`] holds a matte generated for
    /// this exact clip — per `request.md`'s Fase 4 "Remoção de fundo por IA" spec. Not a plain
    /// style toggle like the effect fields above: the matte in `background_removal_mask_path`
    /// is generated per-clip (tied to this instance's own `source_in_secs`/`source_out_secs`
    /// range, see `crate::background_removal`), so this field is deliberately excluded from
    /// [`ClipFormatting`] — pasting it onto a different block would point that block at another
    /// clip's matte video. Wired into export (`crate::render::resolve_timeline_segments_multi`
    /// -> `avbridge::ClipSegment::mask_video_path` -> an `alphamerge` stage in
    /// `timeline_export_multi.c`), gated the same way `mask_shape`/`chroma_key`'s own alpha is:
    /// **only takes effect on an overlay track** (track 1+ in a multi-track export) — a
    /// single/background track's clips never composite, so their alpha (from this or any other
    /// source) is always discarded by the final `format=yuv420p` conform regardless. Also wired
    /// into preview (`crate::preview::build_composite_branch`'s `alphacombine` stage — the
    /// GStreamer counterpart to avfilter's `alphamerge`), same overlay-only gate.
    /// `#[serde(default)]` so older saved projects load with it off.
    #[serde(default)]
    pub background_removal_enabled: bool,
    /// Path to the grayscale-as-luma alpha-matte video `ui`'s "Gerar máscara" flow generates for
    /// this clip (`App::spawn_generate_matte_for_selected_clip`) — meaningless while
    /// [`ClipInstance::background_removal_enabled`] is `false`. Empty string = not yet
    /// generated. `#[serde(default)]` so older saved projects load with no matte.
    #[serde(default)]
    pub background_removal_mask_path: String,
    /// Compositing blend mode for this block ([`BlendMode::Normal`] by default — plain
    /// alpha-over, unchanged from before this field existed). Only meaningful for a clip on an
    /// overlay track (background/single-track clips never composite against anything).
    /// **Scope limit, not a bug**: while a non-`Normal` mode is set, this clip's whole layer
    /// blends against the canvas at full size — [`ClipInstance::position_keyframes`] (PIP-style
    /// repositioning) is not honored for that layer at the same time, since correctly combining
    /// arbitrary positioning with alpha-aware blend-mode math needs a materially more complex
    /// filter chain than either alone; that combination is a separate, not-yet-built follow-up.
    /// Wired into both export (`render.rs` feeds [`BlendMode::ffmpeg_name`] into a
    /// `blend=all_mode=...` avfilter stage, `avbridge`'s `timeline_export_multi.c`) and live
    /// preview (`preview.rs`'s `Preview::current_frame` composites a dedicated `appsink` branch
    /// onto the `compositor` output via [`crate::blend_mode::blend_channel`], the same formulas
    /// export's own `blend` filter uses). `#[serde(default)]` so older saved projects load with
    /// `Normal`.
    #[serde(default)]
    pub blend_mode: BlendMode,
    /// Fraction (`0.0..=1.0`, `x` then `y`) of this clip's own frame that rotation
    /// ([`ClipInstance::rotation_keyframes`]) and scale ([`ClipInstance::layer_scale_x`]/`_y`,
    /// [`ClipInstance::scale_keyframes`]) pivot around, instead of always the frame's own
    /// center. `(0.5, 0.5)` (the default — every clip before this field existed effectively
    /// used this) means "center," matching current behavior exactly; `(0.0, 0.0)` is the
    /// top-left corner, `(1.0, 1.0)` the bottom-right, etc. Wired into export
    /// (`avbridge::ClipSegment::anchor_x`/`anchor_y`, both `encode_timeline_export` and
    /// `encode_timeline_export_multi` — applies to every clip, not just overlay-track ones).
    /// **Scope limit, not a bug**: live preview doesn't reflect a non-default anchor yet —
    /// GStreamer's `rotate` element rotates around its own input frame's center with no anchor
    /// concept, and export's own trick (shifting the canvas-conforming `pad` stage's offset so
    /// the anchor lands at that buffer's center — verified against real `ffmpeg` output) has no
    /// equivalent GStreamer element to build on; a real preview implementation would need a
    /// GStreamer `videobox`-based restructuring, a separate follow-up. `#[serde(default)]`
    /// falling back to `f32`'s own `0.0` default would silently put the pivot at the top-left
    /// corner for a project saved before this field existed, not the actual old center-pivot
    /// behavior — `#[serde(default = "default_anchor")]` is required here, not a bare
    /// `#[serde(default)]`.
    #[serde(default = "default_anchor")]
    pub anchor_x: f32,
    #[serde(default = "default_anchor")]
    pub anchor_y: f32,
    /// A user-pinned auto-reframe anchor (a `0.0..=1.0` fraction of the source frame, the same
    /// convention [`crate::auto_reframe::compute_reframe_crop`]'s own `subject_center` parameter
    /// already uses), overriding face-detection entirely for this clip's next auto/dynamic
    /// reframe run — CF-04's own "optional user-provided seed point" gap. `None` (the default,
    /// and every clip saved before this field existed) keeps the exact old face-detection-only
    /// behavior. Deliberately *not* consumed automatically the moment it's set: a clip carrying
    /// a seed point still needs its "Reenquadrar automaticamente"/"Reenquadrar dinamicamente"
    /// button pressed, same as before — this only changes what that run anchors to, not when it
    /// runs. `#[serde(default)]` so an older saved project still loads.
    #[serde(default)]
    pub reframe_seed_point: Option<(f32, f32)>,
    /// CF-09 slice 4 (`spec/architecture/competitive-feature-plan.md`) — `true` once
    /// [`ClipInstance::privacy_blur_mask_path`] holds a matte generated for this exact clip
    /// instance (via `avcore::mask_propagation` propagating [`Self::privacy_blur_seed_vertices`]
    /// across the clip's own trimmed source range). Same "not a plain style toggle" reasoning
    /// [`Self::background_removal_enabled`]'s own doc comment gives — the matte is tied to this
    /// instance's exact `source_in_secs`/`source_out_secs` range, so this field (and the other
    /// `privacy_blur_*` fields below) is deliberately excluded from [`ClipFormatting`], same as
    /// the background-removal fields. `#[serde(default)]` so older saved projects load with it
    /// off.
    #[serde(default)]
    pub privacy_blur_enabled: bool,
    /// Path to the grayscale-as-luma matte video the properties panel's "Aplicar blur" flow
    /// generates for this clip (mirrors [`Self::background_removal_mask_path`]'s exact
    /// generation/caching shape, reusing the same `avcore::encode_matte_video`/mask-cache-dir
    /// machinery) — meaningless while [`Self::privacy_blur_enabled`] is `false`. Empty string =
    /// not yet generated. `#[serde(default)]` so older saved projects load with no matte.
    #[serde(default)]
    pub privacy_blur_mask_path: String,
    /// `gblur` sigma (blur intensity) `avbridge_apply_privacy_blur` is called with. `#[serde(
    /// default = "default_privacy_blur_sigma")]` rather than defaulting to `0.0`: a positive
    /// sigma is required (`avbridge::apply_privacy_blur` rejects `<= 0.0`), so an older saved
    /// project (or a freshly-added clip that hasn't touched this slider yet) still gets a usable
    /// value if blur is ever enabled without first visiting the slider.
    #[serde(default = "default_privacy_blur_sigma")]
    pub privacy_blur_sigma: f32,
    /// The seed polygon's vertices (`(x_frac, y_frac)`, fraction of frame size — the same
    /// convention [`crate::timeline::ShapeKind::Polygon`] and `avcore::mask_propagation::
    /// PropagatedMask::vertices` already use), drawn on the clip's own first trimmed frame —
    /// empty means "not yet seeded." Reset alongside [`Self::privacy_blur_enabled`]/
    /// [`Self::privacy_blur_mask_path`] whenever the clip's own trim range changes (a split): a
    /// rectangle traced against one specific frame's visual content is meaningless once that
    /// frame is no longer this clip's own first frame, unlike [`Self::reframe_seed_point`]'s
    /// purely spatial anchor.
    #[serde(default)]
    pub privacy_blur_seed_vertices: Vec<(f32, f32)>,
    /// The seed rectangle's own center, in the same `(x_frac, y_frac)` convention —
    /// [`avcore::mask_propagation::propagate_mask_by_translation`]'s own
    /// `initial_center_x_frac`/`_y_frac` tracking seed, kept alongside
    /// [`Self::privacy_blur_seed_vertices`] rather than re-derived from them (the vertices are a
    /// rectangle in this first cut, but the propagation API accepts an arbitrary polygon, so the
    /// tracked center isn't necessarily their centroid).
    #[serde(default)]
    pub privacy_blur_seed_center_x_frac: f32,
    #[serde(default)]
    pub privacy_blur_seed_center_y_frac: f32,
}

fn default_privacy_blur_sigma() -> f32 {
    15.0
}

fn default_anchor() -> f32 {
    0.5
}

/// The rendering/display settings of a [`ClipInstance`] that can be copied onto a different
/// block without touching its structural fields (`id`, `asset_id`, start/trim, composite
/// membership). Used by `ui`'s "copiar formatação" feature (`Ctrl+Shift+C`/`V`) and by its
/// layer-template feature (`request.md`'s Fase 4 "Templates de grupo de camadas") — a saved
/// template is a named `Vec<(TrackKind, ClipFormatting)>` persisted to `ui`'s `PrefsState`,
/// hence `Serialize`/`Deserialize` here (every field type already round-trips through
/// `.ocproj`'s project persistence via `ClipInstance` itself, so this is no new surface).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClipFormatting {
    pub gain_db: f32,
    pub frozen: bool,
    pub speed_factor: f32,
    pub crop_x: f32,
    pub crop_y: f32,
    pub crop_w: f32,
    pub crop_h: f32,
    pub mask_shape: MaskShape,
    pub mask_corner_radius: f32,
    pub flipped_h: bool,
    pub color_filter: ColorFilter,
    pub vignette_intensity: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    pub sharpen: f32,
    pub chroma_key_enabled: bool,
    pub chroma_key_color: [u8; 3],
    pub chroma_key_tolerance: f32,
    pub blur_intensity: f32,
    pub shake_intensity: f32,
    pub glitch_intensity: f32,
    pub pixelize_intensity: f32,
    pub transition_in: TransitionType,
    pub transition_duration_secs: f32,
    pub position_keyframes: Vec<Keyframe<Position>>,
    pub scale_keyframes: Vec<Keyframe<f32>>,
    pub rotation_keyframes: Vec<Keyframe<f32>>,
    pub opacity_keyframes: Vec<Keyframe<f32>>,
    pub gain_keyframes: Vec<Keyframe<f32>>,
    pub voice_cleanup_enabled: bool,
    pub voice_cleanup_noise_floor_db: f32,
    pub voice_cleanup_compressor_threshold_db: f32,
    pub voice_cleanup_compressor_ratio: f32,
    pub voice_cleanup_ceiling_linear: f32,
    pub brightness_keyframes: Vec<Keyframe<f32>>,
    pub contrast_keyframes: Vec<Keyframe<f32>>,
    pub saturation_keyframes: Vec<Keyframe<f32>>,
    pub crop_x_keyframes: Vec<Keyframe<f32>>,
    pub crop_y_keyframes: Vec<Keyframe<f32>>,
    pub crop_w_keyframes: Vec<Keyframe<f32>>,
    pub crop_h_keyframes: Vec<Keyframe<f32>>,
    pub deflicker_enabled: bool,
    pub lut_path: String,
    pub layer_scale_x: f32,
    pub layer_scale_y: f32,
    pub stabilization_intensity: f32,
    pub blend_mode: BlendMode,
    pub anchor_x: f32,
    pub anchor_y: f32,
}

/// A named, reusable group of layers (per `request.md`'s Fase 4 "Templates de grupo de
/// camadas" spec, e.g. "webcam recortada + fundo com blur + jogo centralizado") — each entry
/// pairs a [`TrackKind`] with the [`ClipFormatting`] to apply to whatever source clip the user
/// picks for that layer when the template is applied. Order matters (it's the layer stacking
/// order the template was saved with) but nothing else about the *source* clips (asset,
/// timing, track identity) is captured — those are supplied fresh each time the template is
/// applied, which is the whole point: reapply the same look to different footage without
/// reconfiguring effect-by-effect. Persisted in `ui`'s `PrefsState` (app-wide, not per-project,
/// since a template is meant to be reused across projects/shorts), not `.ocproj` — plain
/// top-level `Vec<LayerTemplate>` there, not nested in `Project`/`Sequence`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerTemplate {
    pub name: String,
    pub layers: Vec<(TrackKind, ClipFormatting)>,
}

fn default_speed_factor() -> f32 {
    1.0
}

fn default_transition_duration() -> f32 {
    0.5
}

fn default_crop_extent() -> f32 {
    1.0
}

fn default_unity_multiplier() -> f32 {
    1.0
}

fn default_chroma_key_color() -> [u8; 3] {
    [0, 255, 0]
}

fn default_chroma_key_tolerance() -> f32 {
    0.4
}

fn default_voice_cleanup_noise_floor_db() -> f32 {
    -30.0
}

fn default_voice_cleanup_compressor_threshold_db() -> f32 {
    -18.0
}

fn default_voice_cleanup_compressor_ratio() -> f32 {
    3.0
}

fn default_voice_cleanup_ceiling_linear() -> f32 {
    0.95
}

impl ClipInstance {
    /// A fresh, entirely-default clip at `start_secs`, trimmed to
    /// `source_in_secs..source_out_secs` of `asset_id` -- every effect neutral, unity gain and
    /// speed, no keyframes, voice cleanup off. Shared by every frontend that places a new asset
    /// on the timeline, so the ~70-field neutral literal lives in exactly one place; callers
    /// override individual fields with struct-update syntax (e.g. `voice_cleanup_enabled` for a
    /// clip landing on a `Mic` track).
    pub fn new(
        id: u64,
        asset_id: u64,
        start_secs: f64,
        source_in_secs: f64,
        source_out_secs: f64,
    ) -> Self {
        Self {
            id,
            asset_id,
            start_secs,
            source_in_secs,
            source_out_secs,
            composite_id: None,
            color_label: None,
            gain_db: 0.0,
            frozen: false,
            speed_factor: 1.0,
            speed_ramp_end_factor: None,
            nested_sequence_id: None,
            crop_x: 0.0,
            crop_y: 0.0,
            crop_w: 1.0,
            crop_h: 1.0,
            mask_shape: MaskShape::None,
            mask_corner_radius: 0.0,
            flipped_h: false,
            color_filter: ColorFilter::None,
            vignette_intensity: 0.0,
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            sharpen: 0.0,
            chroma_key_enabled: false,
            chroma_key_color: [0, 255, 0],
            chroma_key_tolerance: 0.4,
            blur_intensity: 0.0,
            shake_intensity: 0.0,
            glitch_intensity: 0.0,
            pixelize_intensity: 0.0,
            transition_in: TransitionType::None,
            transition_duration_secs: 0.5,
            position_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            opacity_keyframes: vec![],
            gain_keyframes: vec![],
            voice_cleanup_enabled: false,
            voice_cleanup_noise_floor_db: -30.0,
            voice_cleanup_compressor_threshold_db: -18.0,
            voice_cleanup_compressor_ratio: 3.0,
            voice_cleanup_ceiling_linear: 0.95,
            brightness_keyframes: vec![],
            contrast_keyframes: vec![],
            saturation_keyframes: vec![],
            crop_x_keyframes: vec![],
            crop_y_keyframes: vec![],
            crop_w_keyframes: vec![],
            crop_h_keyframes: vec![],
            deflicker_enabled: false,
            lut_path: String::new(),
            layer_scale_x: 1.0,
            layer_scale_y: 1.0,
            stabilization_intensity: 0.0,
            background_removal_enabled: false,
            background_removal_mask_path: String::new(),
            blend_mode: BlendMode::Normal,
            anchor_x: 0.5,
            anchor_y: 0.5,
            reframe_seed_point: None,
            privacy_blur_enabled: false,
            privacy_blur_mask_path: String::new(),
            privacy_blur_sigma: 15.0,
            privacy_blur_seed_vertices: Vec::new(),
            privacy_blur_seed_center_x_frac: 0.0,
            privacy_blur_seed_center_y_frac: 0.0,
        }
    }

    /// How long this instance plays for, i.e. its trimmed length — not the source asset's
    /// full duration. Uses [`keyframe::smooth_speed_ramp_duration_secs`]'s log-based integral
    /// when [`ClipInstance::speed_ramp_end_factor`] is `Some` (a smooth ramp doesn't compress
    /// time by a plain constant divisor), plain division otherwise.
    pub fn duration_secs(&self) -> f64 {
        let source_duration = self.source_out_secs - self.source_in_secs;
        match self.speed_ramp_end_factor {
            Some(end_speed) => keyframe::smooth_speed_ramp_duration_secs(
                source_duration,
                self.speed_factor,
                end_speed,
            ),
            None => source_duration / self.speed_factor as f64,
        }
    }

    /// Linear amplitude multiplier for [`ClipInstance::gain_db`] — e.g. `+6.0` dB roughly
    /// doubles amplitude, `-6.0` dB roughly halves it. `0.0` dB gives `1.0` (unity).
    pub fn gain_linear(&self) -> f32 {
        10f32.powf(self.gain_db / 20.0)
    }

    /// `true` if the crop rect isn't the full, uncropped frame.
    pub fn is_cropped(&self) -> bool {
        self.crop_x != 0.0 || self.crop_y != 0.0 || self.crop_w != 1.0 || self.crop_h != 1.0
    }

    /// `true` if this is a compound clip (nested sequence) — see
    /// [`ClipInstance::nested_sequence_id`]'s own doc comment.
    pub fn is_nested_sequence(&self) -> bool {
        self.nested_sequence_id.is_some()
    }

    /// The [`crate::media::MediaAsset::id`] this clip actually resolves to: `asset_id` for an
    /// ordinary clip, or [`crate::nested_sequence::synthetic_asset_id`] of
    /// [`ClipInstance::nested_sequence_id`] for a compound clip (whose `asset_id` field is left
    /// at whatever it was when the clip was wrapped — see that field's own doc comment). Every
    /// asset-id lookup for a clip must go through this method, never read `asset_id` directly,
    /// so a compound clip resolves against the synthetic asset
    /// [`crate::nested_sequence::materialize_nested_sequences`] merges into a `media_library`.
    pub fn resolved_asset_id(&self) -> u64 {
        self.nested_sequence_id
            .map(crate::nested_sequence::synthetic_asset_id)
            .unwrap_or(self.asset_id)
    }

    /// `true` if a layer mask ([`ClipInstance::mask_shape`]) is applied.
    pub fn is_masked(&self) -> bool {
        self.mask_shape != MaskShape::None
    }

    /// `true` if a color filter ([`ClipInstance::color_filter`]) is applied.
    pub fn is_color_filtered(&self) -> bool {
        self.color_filter != ColorFilter::None
    }

    /// `true` if a non-default compositing blend mode ([`ClipInstance::blend_mode`]) is set.
    pub fn has_blend_mode(&self) -> bool {
        self.blend_mode != BlendMode::Normal
    }

    /// `true` if a non-center rotation/scale pivot ([`ClipInstance::anchor_x`]/`anchor_y`) is
    /// set.
    pub fn has_anchor(&self) -> bool {
        (self.anchor_x - 0.5).abs() > 1e-4 || (self.anchor_y - 0.5).abs() > 1e-4
    }

    /// `true` if a 3D LUT ([`ClipInstance::lut_path`]) is applied.
    pub fn has_lut(&self) -> bool {
        !self.lut_path.is_empty()
    }

    /// `true` if this block's layer footprint ([`ClipInstance::layer_scale_x`]/`_y`) differs
    /// from native size on either axis.
    pub fn has_layer_scale(&self) -> bool {
        (self.layer_scale_x - 1.0).abs() > 1e-4 || (self.layer_scale_y - 1.0).abs() > 1e-4
    }

    /// `true` if [`ClipInstance::stabilization_intensity`] is above zero.
    pub fn has_stabilization(&self) -> bool {
        self.stabilization_intensity > 0.0
    }

    /// `true` if [`ClipInstance::vignette_intensity`] is above zero.
    pub fn has_vignette(&self) -> bool {
        self.vignette_intensity > 0.0
    }

    /// `true` if chroma key ([`ClipInstance::chroma_key_enabled`]) is on.
    pub fn is_chroma_keyed(&self) -> bool {
        self.chroma_key_enabled
    }

    /// `true` if a transition ([`ClipInstance::transition_in`]) is configured on this block's
    /// incoming edge.
    pub fn has_transition(&self) -> bool {
        self.transition_in != TransitionType::None
    }

    /// `true` if this block has any position keyframes.
    pub fn has_position_keyframes(&self) -> bool {
        !self.position_keyframes.is_empty()
    }

    /// `true` if this block has any scale keyframes (the general-keyframe replacement for the
    /// old `is_zoomed`).
    pub fn has_scale_keyframes(&self) -> bool {
        !self.scale_keyframes.is_empty()
    }

    /// `true` if this block has any rotation keyframes.
    pub fn has_rotation_keyframes(&self) -> bool {
        !self.rotation_keyframes.is_empty()
    }

    /// `true` if this block has any opacity keyframes.
    pub fn has_opacity_keyframes(&self) -> bool {
        !self.opacity_keyframes.is_empty()
    }

    /// `true` if this block has any audio gain keyframes (overriding the constant `gain_db`).
    pub fn has_gain_keyframes(&self) -> bool {
        !self.gain_keyframes.is_empty()
    }

    /// `true` if this block has color-grading keyframes on any of brightness/contrast/
    /// saturation — gates whether [`Self::video_filter_chain`]'s static `eq` stage should defer
    /// to [`Self::keyframe_video_filter_chain`]'s animated one instead.
    pub fn has_color_keyframes(&self) -> bool {
        !self.brightness_keyframes.is_empty()
            || !self.contrast_keyframes.is_empty()
            || !self.saturation_keyframes.is_empty()
    }

    /// `true` if this block has crop/pan keyframes on any of x/y/width/height — gates whether
    /// [`Self::video_filter_chain`]'s static `crop` stage should defer to
    /// [`Self::keyframe_video_filter_chain`]'s animated one instead.
    pub fn has_crop_keyframes(&self) -> bool {
        !self.crop_x_keyframes.is_empty()
            || !self.crop_y_keyframes.is_empty()
            || !self.crop_w_keyframes.is_empty()
            || !self.crop_h_keyframes.is_empty()
    }

    /// Builds this clip's crop/scale/rotation/opacity/color-balance keyframe avfilter fragment,
    /// spliced into the per-clip chain before [`ClipInstance::video_filter_chain`]'s own stages
    /// — the same position the old `zoom` stage used to occupy. `None` if none of the five are
    /// animated. Position keyframes aren't part of this — they apply to the *overlay*
    /// compositing stage, not a per-clip filter (see [`keyframe::position_overlay_xy_expr`] and
    /// `crate::render`). Crop keyframes run first (mirroring the static `crop` stage's own
    /// traditional "runs before every other effect" position in `video_filter_chain`, so a
    /// crop/pan animation reframes the source before scale/rotate/color-balance operate on it).
    /// Color-balance keyframes running here at all (rather than in their usual post-crop/
    /// deflicker/stabilization spot in `video_filter_chain`) is a real, narrow ordering caveat —
    /// see [`ClipInstance::brightness_keyframes`]'s doc comment.
    pub fn keyframe_video_filter_chain(
        &self,
        fps_num: u32,
        fps_den: u32,
        timeline_duration_secs: f64,
    ) -> Option<String> {
        let mut stages = Vec::new();
        if self.has_crop_keyframes() {
            if let Some(crop) = keyframe::crop_filter_expr(
                &self.crop_x_keyframes,
                &self.crop_y_keyframes,
                &self.crop_w_keyframes,
                &self.crop_h_keyframes,
                self.crop_x,
                self.crop_y,
                self.crop_w,
                self.crop_h,
                fps_num,
                fps_den,
                timeline_duration_secs,
            ) {
                stages.push(crop);
            }
        }
        if let Some(scale) = keyframe::scale_filter_expr(
            &self.scale_keyframes,
            fps_num,
            fps_den,
            timeline_duration_secs,
        ) {
            stages.push(scale);
        }
        if let Some(angle_expr) =
            keyframe::rotation_filter_angle_expr(&self.rotation_keyframes, timeline_duration_secs)
        {
            stages.push(format!(
                "rotate=angle='{angle_expr}':ow=rotw('{angle_expr}'):oh=roth('{angle_expr}')"
            ));
        }
        if let Some(alpha_expr) = keyframe::opacity_alpha_ramp_expr(
            &self.opacity_keyframes,
            fps_num,
            fps_den,
            timeline_duration_secs,
        ) {
            stages.push(format!(
                "format=yuva420p,geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='alpha(X,Y)*({alpha_expr})'"
            ));
        }
        if self.has_color_keyframes() {
            if let Some(eq) = keyframe::color_balance_filter_expr(
                &self.brightness_keyframes,
                &self.contrast_keyframes,
                &self.saturation_keyframes,
                self.brightness,
                self.contrast,
                self.saturation,
                timeline_duration_secs,
            ) {
                stages.push(eq);
            }
        }
        if stages.is_empty() {
            None
        } else {
            Some(stages.join(","))
        }
    }

    /// Builds this clip's avfilter chain description for `core::render::render_timeline_export`
    /// — the subset of effect fields expressible as a static per-clip video filter (see
    /// `features/request.md`'s Fase 4 "Efeitos visuais" list): crop, deflicker, brightness/
    /// contrast/saturation, the black-and-white/sepia color filter, chroma key, mask shape,
    /// blur, sharpen, pixelize, shake, glitch, vignette, and horizontal flip. `gain_db` is audio, not
    /// video, and isn't part of this chain. `speed_factor` is handled in the native bridge;
    /// general keyframes are compiled separately by [`ClipInstance::keyframe_video_filter_chain`].
    /// `mask_shape`'s alpha only survives to the rendered
    /// output on an overlay track — see the caveat on its stage below. `transition_in` needs a
    /// materially different mechanism (cross-clip blending) and isn't covered here yet.
    /// `frozen` also needs a different mechanism (frame duplication) but is covered elsewhere —
    /// see [`ClipInstance::frozen`]'s doc.
    ///
    /// Returns `""` (no-op) if none of the covered effects deviate from neutral. Filters are
    /// comma-joined in a fixed order — crop first (so later filters see the cropped frame,
    /// not un-cropped coordinates), flip last (so it doesn't mirror crop/vignette geometry) —
    /// which is a judgment call, not something derivable from the field values themselves.
    pub fn video_filter_chain(&self) -> String {
        let mut stages = Vec::new();

        if self.is_cropped() && !self.has_crop_keyframes() {
            stages.push(format!(
                "crop=iw*{}:ih*{}:iw*{}:ih*{}",
                self.crop_w, self.crop_h, self.crop_x, self.crop_y
            ));
        }
        if self.deflicker_enabled {
            // Arithmetic-mean mode over a 5-frame temporal window (both FFmpeg's own
            // defaults) — smooths out frame-to-frame luminance variation from screen/gameplay
            // capture at certain refresh rates, before any other stage reshapes that luminance.
            stages.push("deflicker=mode=am:size=5".to_string());
        }
        if self.has_stabilization() {
            // rx/ry (search radius in pixels, deshake's valid range 0..64) scale with
            // intensity rather than a fixed radius — a small radius only corrects gentle
            // handheld wobble, a large one can also absorb bigger jolts, at the cost of more
            // aggressive cropping into the frame at the edges (deshake's own trade-off, not
            // something this stage compensates for). Runs right after deflicker, before any
            // color/stylistic stage reshapes the pixel data motion estimation reads.
            let radius = (4.0 + self.stabilization_intensity.clamp(0.0, 1.0) * 60.0).round() as i32;
            stages.push(format!("deshake=rx={radius}:ry={radius}:edge=mirror"));
        }
        if !self.has_color_keyframes()
            && (self.brightness != 0.0 || self.contrast != 1.0 || self.saturation != 1.0)
        {
            stages.push(format!(
                "eq=brightness={}:contrast={}:saturation={}",
                self.brightness, self.contrast, self.saturation
            ));
        }
        match self.color_filter {
            ColorFilter::None => {}
            ColorFilter::BlackAndWhite => stages.push("hue=s=0".to_string()),
            ColorFilter::Sepia => stages.push(
                "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131:0".to_string(),
            ),
        }
        if self.has_lut() {
            // Forward slashes even on Windows sidesteps avfilter's own backslash-escaping rules
            // inside a quoted option value (ffmpeg accepts `/`-separated paths on any platform);
            // a literal single quote in the path (the one character `'...'` quoting can't pass
            // through unescaped) is escaped avfilter-style.
            let escaped = self.lut_path.replace('\\', "/").replace('\'', "'\\''");
            stages.push(format!("lut3d=file='{escaped}'"));
        }
        if self.is_chroma_keyed() {
            let [r, g, b] = self.chroma_key_color;
            stages.push(format!(
                "colorkey=0x{r:02x}{g:02x}{b:02x}:{:.3}:0.1",
                self.chroma_key_tolerance
            ));
        }
        match self.mask_shape {
            MaskShape::None => {}
            MaskShape::Circle | MaskShape::RoundedRect => {
                // Promotes to an alpha-having pixel format, then geq's per-pixel expression
                // clips it to the mask shape by zeroing alpha() outside it — multiplying by
                // (rather than overwriting) the existing alpha(X,Y) so a chroma-keyed clip's
                // own transparency composes with the mask instead of being clobbered by it.
                // Note the same caveat as chroma_key's below: the final `format=yuv420p`
                // conform on a single (non-overlay) track drops this alpha again — the mask
                // only has a visible effect on a clip placed on an overlay track (see
                // `build_overlay_vfilter` in timeline_export_multi.c, which has no such conform between
                // its two per-track chains and the `overlay` filter that composites them).
                //
                // avfilter's filtergraph-level parser only treats a comma as a stage separator
                // outside quotes — every comma below sits inside the geq option's own `'...'`
                // quoting, so `pow`/`min`/`max`/`lte`'s comma-separated arguments are safe
                // (verified against a real ffmpeg build, not just read off the docs).
                let minwh = "min(W,H)";
                let alpha_expr = match self.mask_shape {
                    MaskShape::Circle => {
                        format!("alpha(X,Y)*lte(pow(X-W/2,2)+pow(Y-H/2,2),pow({minwh}/2,2))")
                    }
                    MaskShape::RoundedRect => {
                        // Rounded-rect signed-distance field: shrink the half-extents by the
                        // corner radius, measure how far outside that inner rect (X,Y) falls,
                        // then subtract the radius back out — <=0 is inside the rounded shape.
                        let radius = format!(
                            "min({:.4}*{minwh},{minwh}/2)",
                            self.mask_corner_radius.clamp(0.0, 1.0)
                        );
                        format!(
                            "alpha(X,Y)*lte(sqrt(pow(max(abs(X-W/2)-(W/2-{radius}),0),2)+pow(max(abs(Y-H/2)-(H/2-{radius}),0),2))-{radius},0)"
                        )
                    }
                    MaskShape::None => unreachable!("outer match already excludes None"),
                };
                stages.push(format!(
                    "format=yuva420p,geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='{alpha_expr}'"
                ));
            }
        }
        if self.blur_intensity > 0.0 {
            stages.push(format!("boxblur={:.2}", self.blur_intensity * 10.0));
        }
        if self.sharpen > 0.0 {
            stages.push(format!("unsharp=5:5:{:.2}:5:5:0.0", self.sharpen * 3.0));
        }
        if self.pixelize_intensity > 0.0 {
            // Scale down to block_size-pixel grid then back up — nearest-neighbor gives the
            // hard mosaic look. block ranges from 2px (subtle) to 50px (heavy censorship).
            let block = (2.0 + self.pixelize_intensity * 48.0).round() as u32;
            stages.push(format!(
                "scale=iw/{b}:ih/{b}:flags=neighbor,scale=iw*{b}:ih*{b}:flags=neighbor",
                b = block
            ));
        }
        if self.shake_intensity > 0.0 {
            // Crop away a margin on all sides (giving room to "shake" into), oscillate the crop
            // origin with two independent sinusoids, then scale back to the original frame size.
            // margin at full intensity: 8% of each dimension per side.
            let margin = self.shake_intensity * 0.08_f32;
            let keep = 1.0_f32 - 2.0 * margin;
            let scale_back = 1.0 / keep;
            stages.push(format!(
                "crop=iw*{keep:.4}:ih*{keep:.4}:iw*{m:.4}*(1+sin(n*0.31)):ih*{m:.4}*(1+cos(n*0.23)),scale=iw*{sb:.4}:ih*{sb:.4}",
                m = margin,
                sb = scale_back,
            ));
        }
        if self.glitch_intensity > 0.0 {
            // Temporal luma + chroma noise approximates digital glitch corruption.
            let ls = (self.glitch_intensity * 60.0).round() as u32;
            let cs = (self.glitch_intensity * 25.0).round() as u32;
            stages.push(format!(
                "noise=c0s={ls}:c0f=t:c1s={cs}:c1f=t:c2s={cs}:c2f=t"
            ));
        }
        if self.has_vignette() {
            stages.push(format!("vignette=PI/4*{:.3}", self.vignette_intensity));
        }
        if self.flipped_h {
            stages.push("hflip".to_string());
        }

        stages.join(",")
    }

    /// True if `at_secs` (timeline-relative) falls strictly inside this clip's placed range.
    /// Boundary-exact positions return `false` — splitting exactly on an edge would just
    /// produce a zero-length half.
    pub fn contains(&self, at_secs: f64) -> bool {
        at_secs > self.start_secs && at_secs < self.start_secs + self.duration_secs()
    }

    /// Drags the clip's left edge to `new_start_secs`, keeping its end point
    /// (`source_out_secs`) fixed — `source_in_secs` shifts by the same delta as `start_secs`,
    /// since trimming the start plays a later point in the source. No-op (`false`) if that
    /// would put `new_start_secs` or the resulting `source_in_secs` below zero, or shrink the
    /// clip below `min_duration_secs`.
    pub fn trim_start(&mut self, new_start_secs: f64, min_duration_secs: f64) -> bool {
        let timeline_delta = new_start_secs - self.start_secs;
        let new_source_in_secs = self.source_in_secs + timeline_delta * self.speed_factor as f64;
        let new_source_duration = self.source_out_secs - new_source_in_secs;
        let new_timeline_duration = new_source_duration / self.speed_factor as f64;
        if new_start_secs < 0.0
            || new_source_in_secs < 0.0
            || new_timeline_duration < min_duration_secs
        {
            return false;
        }
        self.start_secs = new_start_secs;
        self.source_in_secs = new_source_in_secs;
        true
    }

    /// Snapshots every rendering/display field as a [`ClipFormatting`] value — used by
    /// `ui`'s "copiar formatação" feature to copy settings that can be pasted onto a
    /// different block without duplicating the clip itself.
    pub fn formatting(&self) -> ClipFormatting {
        ClipFormatting {
            gain_db: self.gain_db,
            frozen: self.frozen,
            speed_factor: self.speed_factor,
            crop_x: self.crop_x,
            crop_y: self.crop_y,
            crop_w: self.crop_w,
            crop_h: self.crop_h,
            mask_shape: self.mask_shape,
            mask_corner_radius: self.mask_corner_radius,
            flipped_h: self.flipped_h,
            color_filter: self.color_filter,
            vignette_intensity: self.vignette_intensity,
            brightness: self.brightness,
            contrast: self.contrast,
            saturation: self.saturation,
            sharpen: self.sharpen,
            chroma_key_enabled: self.chroma_key_enabled,
            chroma_key_color: self.chroma_key_color,
            chroma_key_tolerance: self.chroma_key_tolerance,
            blur_intensity: self.blur_intensity,
            shake_intensity: self.shake_intensity,
            glitch_intensity: self.glitch_intensity,
            pixelize_intensity: self.pixelize_intensity,
            transition_in: self.transition_in,
            transition_duration_secs: self.transition_duration_secs,
            position_keyframes: self.position_keyframes.clone(),
            scale_keyframes: self.scale_keyframes.clone(),
            rotation_keyframes: self.rotation_keyframes.clone(),
            opacity_keyframes: self.opacity_keyframes.clone(),
            gain_keyframes: self.gain_keyframes.clone(),
            voice_cleanup_enabled: self.voice_cleanup_enabled,
            voice_cleanup_noise_floor_db: self.voice_cleanup_noise_floor_db,
            voice_cleanup_compressor_threshold_db: self.voice_cleanup_compressor_threshold_db,
            voice_cleanup_compressor_ratio: self.voice_cleanup_compressor_ratio,
            voice_cleanup_ceiling_linear: self.voice_cleanup_ceiling_linear,
            brightness_keyframes: self.brightness_keyframes.clone(),
            contrast_keyframes: self.contrast_keyframes.clone(),
            saturation_keyframes: self.saturation_keyframes.clone(),
            crop_x_keyframes: self.crop_x_keyframes.clone(),
            crop_y_keyframes: self.crop_y_keyframes.clone(),
            crop_w_keyframes: self.crop_w_keyframes.clone(),
            crop_h_keyframes: self.crop_h_keyframes.clone(),
            deflicker_enabled: self.deflicker_enabled,
            lut_path: self.lut_path.clone(),
            layer_scale_x: self.layer_scale_x,
            layer_scale_y: self.layer_scale_y,
            stabilization_intensity: self.stabilization_intensity,
            blend_mode: self.blend_mode,
            anchor_x: self.anchor_x,
            anchor_y: self.anchor_y,
        }
    }

    /// Applies every field from `f` onto this clip, leaving structural fields (`id`,
    /// `asset_id`, `start_secs`, `source_in_secs`/`source_out_secs`, `composite_id`)
    /// unchanged — the "colar formatação" counterpart to [`Self::formatting`].
    pub fn apply_formatting(&mut self, f: &ClipFormatting) {
        self.gain_db = f.gain_db;
        self.frozen = f.frozen;
        self.speed_factor = f.speed_factor;
        self.crop_x = f.crop_x;
        self.crop_y = f.crop_y;
        self.crop_w = f.crop_w;
        self.crop_h = f.crop_h;
        self.mask_shape = f.mask_shape;
        self.mask_corner_radius = f.mask_corner_radius;
        self.flipped_h = f.flipped_h;
        self.color_filter = f.color_filter;
        self.vignette_intensity = f.vignette_intensity;
        self.brightness = f.brightness;
        self.contrast = f.contrast;
        self.saturation = f.saturation;
        self.sharpen = f.sharpen;
        self.chroma_key_enabled = f.chroma_key_enabled;
        self.chroma_key_color = f.chroma_key_color;
        self.chroma_key_tolerance = f.chroma_key_tolerance;
        self.blur_intensity = f.blur_intensity;
        self.shake_intensity = f.shake_intensity;
        self.glitch_intensity = f.glitch_intensity;
        self.pixelize_intensity = f.pixelize_intensity;
        self.transition_in = f.transition_in;
        self.transition_duration_secs = f.transition_duration_secs;
        self.position_keyframes = f.position_keyframes.clone();
        self.scale_keyframes = f.scale_keyframes.clone();
        self.rotation_keyframes = f.rotation_keyframes.clone();
        self.opacity_keyframes = f.opacity_keyframes.clone();
        self.gain_keyframes = f.gain_keyframes.clone();
        self.voice_cleanup_enabled = f.voice_cleanup_enabled;
        self.voice_cleanup_noise_floor_db = f.voice_cleanup_noise_floor_db;
        self.voice_cleanup_compressor_threshold_db = f.voice_cleanup_compressor_threshold_db;
        self.voice_cleanup_compressor_ratio = f.voice_cleanup_compressor_ratio;
        self.voice_cleanup_ceiling_linear = f.voice_cleanup_ceiling_linear;
        self.brightness_keyframes = f.brightness_keyframes.clone();
        self.contrast_keyframes = f.contrast_keyframes.clone();
        self.saturation_keyframes = f.saturation_keyframes.clone();
        self.crop_x_keyframes = f.crop_x_keyframes.clone();
        self.crop_y_keyframes = f.crop_y_keyframes.clone();
        self.crop_w_keyframes = f.crop_w_keyframes.clone();
        self.crop_h_keyframes = f.crop_h_keyframes.clone();
        self.deflicker_enabled = f.deflicker_enabled;
        self.lut_path = f.lut_path.clone();
        self.layer_scale_x = f.layer_scale_x;
        self.layer_scale_y = f.layer_scale_y;
        self.stabilization_intensity = f.stabilization_intensity;
        self.blend_mode = f.blend_mode;
        self.anchor_x = f.anchor_x;
        self.anchor_y = f.anchor_y;
    }

    /// Drags the clip's right edge to `new_end_secs` (timeline-relative), keeping `start_secs`
    /// and `source_in_secs` fixed. No-op (`false`) if that would shrink the clip below
    /// `min_duration_secs`, or (when `max_source_out_secs` is known — the source asset's own
    /// duration) push `source_out_secs` past the end of the actual source media.
    pub fn trim_end(
        &mut self,
        new_end_secs: f64,
        min_duration_secs: f64,
        max_source_out_secs: Option<f64>,
    ) -> bool {
        let new_timeline_duration = new_end_secs - self.start_secs;
        let new_source_out_secs =
            self.source_in_secs + new_timeline_duration * self.speed_factor as f64;
        if new_timeline_duration < min_duration_secs {
            return false;
        }
        if let Some(max) = max_source_out_secs {
            if new_source_out_secs > max {
                return false;
            }
        }
        self.source_out_secs = new_source_out_secs;
        true
    }

    /// Shifts which part of the source media this clip shows by `delta_secs`, without moving
    /// it on the timeline or changing its duration — Premiere/DaVinci/FCP's "Slip" tool
    /// (`ROADMAP.md` P2 item 11). `source_in_secs` and `source_out_secs` move together. No-op
    /// (`false`) if that would push `source_in_secs` below zero, or (when
    /// `max_source_out_secs` is known — the source asset's own duration) `source_out_secs`
    /// past the end of the actual source media.
    pub fn slip(&mut self, delta_secs: f64, max_source_out_secs: Option<f64>) -> bool {
        let new_source_in_secs = self.source_in_secs + delta_secs;
        let new_source_out_secs = self.source_out_secs + delta_secs;
        if new_source_in_secs < 0.0 {
            return false;
        }
        if let Some(max) = max_source_out_secs {
            if new_source_out_secs > max {
                return false;
            }
        }
        self.source_in_secs = new_source_in_secs;
        self.source_out_secs = new_source_out_secs;
        true
    }
}
