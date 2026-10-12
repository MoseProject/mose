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

use crate::keyframe::{self, Position};
use crate::timeline::clip::ClipInstance;
use crate::timeline::effects::TransitionType;
use crate::timeline::shape_clip::ShapeClip;
use crate::timeline::text_clip::TextClip;

/// Clips starting within this many seconds of an earlier clip's end do not overlap it.
const OVERLAP_EPSILON_SECS: f64 = 1e-3;
/// A clip counts as following another when the gap between them is at most this long.
const DISSOLVE_ADJACENT_SECS: f64 = 0.05;

/// What a [`Track`] carries. Determines how the timeline widget renders its clips
/// (thumbnails for video, waveforms for audio) and which asset kind can be dropped onto it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrackKind {
    Video,
    Audio,
    /// A text-overlay track: holds [`TextClip`]s rasterized into RGBA overlays on export.
    /// No media assets are placed here — only `text_clips`.
    Text,
    /// A shape-overlay track: holds [`ShapeClip`]s (rectangles/ellipses — per `request.md`'s
    /// Fase 4 "Efeitos visuais" umbrella, a graphic-element counterpart to text overlays). No
    /// media assets are placed here — only `shape_clips`.
    Shape,
}

impl TrackKind {
    /// One-letter prefix for auto-generated track names (`V1`, `A2`, ...).
    pub fn name_prefix(self) -> &'static str {
        match self {
            TrackKind::Video => "V",
            TrackKind::Audio => "A",
            TrackKind::Text => "T",
            TrackKind::Shape => "S",
        }
    }
}

/// What kind of audio source a [`Track`] carries — user-set metadata (a track header picker),
/// not inferred. `Video`/`Audio` tracks both carry real audio (a video capture's own embedded
/// game audio counts) and both can be tagged; `Text`/`Shape` tracks stay `Unspecified` since
/// they never carry audio at all. Exists so a feature that needs to reason about *which* track
/// is which audio source — D2 (`spec/architecture/differentiators.md`, highlight detection
/// needs to correlate simultaneous game-audio + mic spikes) and, later, multicam sync — has an
/// actual answer instead of guessing from track order or name. `#[default]` `Unspecified` so an
/// untagged/older-saved project's tracks are simply invisible to those features rather than
/// misclassified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioRole {
    #[default]
    Unspecified,
    GameAudio,
    Mic,
    Music,
}

impl AudioRole {
    /// Maps this role to the integer code used in [`avbridge::AudioSegment::duck_role`] and the
    /// `AudioSegment.duck_role` C field (P2 item 6, "Audio ducking") — `Mic` is the sidechain
    /// trigger, `Music` is what gets ducked under it, everything else mixes in unducked.
    pub fn to_duck_role_code(self) -> u8 {
        match self {
            Self::Unspecified | Self::GameAudio => 0,
            Self::Mic => 1,
            Self::Music => 2,
        }
    }
}

fn default_true() -> bool {
    true
}

/// One row of the timeline (e.g. `V1`, `A1`, `A2` in the mockup), holding an ordered list of
/// clips. Tracks don't overlap-check their own clips — that's an editing-time concern.
///
/// Text tracks (`kind == TrackKind::Text`) hold [`TextClip`]s in `text_clips` instead of
/// [`ClipInstance`]s in `clips` — `clips` is always empty for a text track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: u64,
    pub name: String,
    pub kind: TrackKind,
    pub clips: Vec<ClipInstance>,
    /// Text overlays on this track. Only populated when `kind == TrackKind::Text`; always
    /// empty for `Video`/`Audio` tracks. `#[serde(default)]` so projects saved before this
    /// field existed load without error.
    #[serde(default)]
    pub text_clips: Vec<TextClip>,
    /// Shape overlays on this track. Only populated when `kind == TrackKind::Shape`; always
    /// empty for other track kinds. `#[serde(default)]` so projects saved before this field
    /// existed load without error.
    #[serde(default)]
    pub shape_clips: Vec<ShapeClip>,
    /// Whether this track contributes to export and preview. Toggled from the timeline track
    /// header. Defaults to `true`; missing in project files saved before this field was added
    /// deserializes as `true` via the serde default so existing projects are unaffected.
    #[serde(default = "default_true")]
    pub visible: bool,
    /// Which audio source this track carries, if the user has said — see [`AudioRole`].
    /// `#[serde(default)]` so a project saved before this field existed loads with every track
    /// `Unspecified`, same as a never-tagged track in a new project.
    #[serde(default)]
    pub audio_role: AudioRole,
    /// Silences this track in preview and export without removing its clips. Independent of
    /// [`Self::visible`], which hides a video track's picture but keeps its sound.
    /// `#[serde(default)]` so older projects load with nothing muted.
    #[serde(default)]
    pub muted: bool,
    /// Solo: while any track is soloed, only soloed tracks are heard. See
    /// [`super::Timeline::track_is_audible`].
    #[serde(default)]
    pub solo: bool,
    /// Optional RGB color label for this track, per `matrix/competitor-parity.md`'s 2026-08-27
    /// update (`spec/ROADMAP.md` P4 item 27) — DaVinci Resolve's track color, called out by
    /// users as something Premiere still lacks. Purely cosmetic, same "at-a-glance
    /// organization" role as [`super::clip::ClipInstance::color_label`]. `None` = use the usual
    /// track-header coloring. `#[serde(default)]` so older saved projects load with no label.
    #[serde(default)]
    pub color_label: Option<[u8; 3]>,
    /// Whether this track refuses clip moves/trims from its header's lock toggle — matches
    /// `oca-editor-mock.html`'s `.tl-track-tool` lock icon. Distinct from `visible`: a locked
    /// track still renders/exports, it just can't be edited by accident while working on other
    /// tracks nearby. `#[serde(default)]` so older saved projects load with every track
    /// unlocked.
    #[serde(default)]
    pub locked: bool,
    /// This track's level and stereo balance, applied to every clip on it after the clips' own
    /// gain. Neutral by default. **Keep this the last field**: the project file is positional.
    #[serde(default)]
    pub mix: TrackMix,
}

/// A track's mixer strip: fader, pan, tone and compressor. Neutral by default. **Fields are
/// positional in the project file**: a new one is a format change.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TrackMix {
    /// Level in dB, [`TrackMix::MIN_VOLUME_DB`] to [`TrackMix::MAX_VOLUME_DB`]; `0.0` leaves the
    /// clips as they are.
    pub volume_db: f32,
    /// Stereo balance, `-1.0` (left only) to `1.0` (right only); `0.0` is centred and untouched.
    pub pan: f32,
    /// Gain of the low shelf (120 Hz), the mid bell (1 kHz) and the high shelf (6 kHz), in dB.
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    /// High-pass and low-pass corners in Hz; `0.0` leaves that end open.
    pub highpass_hz: f32,
    pub lowpass_hz: f32,
    /// Compressor threshold in dB and ratio; a ratio of `1.0` is off.
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
}

impl Default for TrackMix {
    fn default() -> Self {
        Self {
            volume_db: 0.0,
            pan: 0.0,
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            highpass_hz: 0.0,
            lowpass_hz: 0.0,
            comp_threshold_db: -18.0,
            comp_ratio: 1.0,
        }
    }
}

impl TrackMix {
    pub const MIN_VOLUME_DB: f32 = -60.0;
    pub const MAX_VOLUME_DB: f32 = 12.0;
    pub const MAX_EQ_DB: f32 = 15.0;
    pub const MIN_HIGHPASS_HZ: f32 = 20.0;
    pub const MAX_HIGHPASS_HZ: f32 = 2_000.0;
    pub const MIN_LOWPASS_HZ: f32 = 500.0;
    pub const MAX_LOWPASS_HZ: f32 = 20_000.0;
    pub const MIN_COMP_THRESHOLD_DB: f32 = -60.0;
    pub const MAX_COMP_RATIO: f32 = 20.0;

    /// Whether the mix changes nothing (no gain, centred, flat tone, no compressor), so a track
    /// needs no mixing stage.
    pub fn is_neutral(&self) -> bool {
        self.volume_db == 0.0 && self.pan == 0.0 && self.strip_is_neutral()
    }

    /// Whether the tone and dynamics are untouched (the fader and pan may not be).
    pub fn strip_is_neutral(&self) -> bool {
        self.eq_low_db == 0.0
            && self.eq_mid_db == 0.0
            && self.eq_high_db == 0.0
            && self.highpass_hz == 0.0
            && self.lowpass_hz == 0.0
            && self.comp_ratio <= 1.0
    }

    /// The tone and dynamics as the export mix takes them.
    pub fn strip(&self) -> avbridge::AudioStrip {
        avbridge::AudioStrip {
            eq_low_db: self.eq_low_db,
            eq_mid_db: self.eq_mid_db,
            eq_high_db: self.eq_high_db,
            highpass_hz: self.highpass_hz,
            lowpass_hz: self.lowpass_hz,
            comp_threshold_db: self.comp_threshold_db,
            comp_ratio: self.comp_ratio,
        }
    }

    /// The same mix inside its limits; a value that is not a number becomes neutral.
    pub fn clamped(self) -> Self {
        let within = |value: f32, low: f32, high: f32, neutral: f32| {
            if value.is_finite() {
                value.clamp(low, high)
            } else {
                neutral
            }
        };
        // A corner frequency is off (0) or inside its useful range.
        let corner = |value: f32, low: f32, high: f32| {
            if !value.is_finite() || value <= 0.0 {
                0.0
            } else {
                value.clamp(low, high)
            }
        };
        Self {
            volume_db: within(
                self.volume_db,
                Self::MIN_VOLUME_DB,
                Self::MAX_VOLUME_DB,
                0.0,
            ),
            pan: within(self.pan, -1.0, 1.0, 0.0),
            eq_low_db: within(self.eq_low_db, -Self::MAX_EQ_DB, Self::MAX_EQ_DB, 0.0),
            eq_mid_db: within(self.eq_mid_db, -Self::MAX_EQ_DB, Self::MAX_EQ_DB, 0.0),
            eq_high_db: within(self.eq_high_db, -Self::MAX_EQ_DB, Self::MAX_EQ_DB, 0.0),
            highpass_hz: corner(
                self.highpass_hz,
                Self::MIN_HIGHPASS_HZ,
                Self::MAX_HIGHPASS_HZ,
            ),
            lowpass_hz: corner(self.lowpass_hz, Self::MIN_LOWPASS_HZ, Self::MAX_LOWPASS_HZ),
            comp_threshold_db: within(
                self.comp_threshold_db,
                Self::MIN_COMP_THRESHOLD_DB,
                0.0,
                -18.0,
            ),
            comp_ratio: within(self.comp_ratio, 1.0, Self::MAX_COMP_RATIO, 1.0),
        }
    }

    /// Linear multiplier of [`Self::volume_db`].
    pub fn linear_gain(&self) -> f32 {
        10f32.powf(self.volume_db / 20.0)
    }

    /// Left and right multipliers of the pan: the side the pan moves away from falls off as a
    /// quarter sine, the other stays at unity, so a centred track is left alone. The same law the
    /// export mix applies (`audio_mix.c`), so the preview matches it.
    pub fn pan_gains(&self) -> (f32, f32) {
        let pan = self.pan.clamp(-1.0, 1.0);
        let left = if pan > 0.0 {
            (pan * std::f32::consts::FRAC_PI_2).cos()
        } else {
            1.0
        };
        let right = if pan < 0.0 {
            (-pan * std::f32::consts::FRAC_PI_2).cos()
        } else {
            1.0
        };
        (left, right)
    }
}

impl Track {
    /// The clip covering `at_secs` (timeline-relative), inclusive of `start_secs` — distinct
    /// from [`ClipInstance::contains`]'s strict-interior semantics (which exists for split
    /// safety, so splitting exactly on an edge doesn't produce a zero-length half). This one is
    /// for "what's playing right now" queries (preview), where the clip starting exactly at
    /// the playhead should count. `None` if nothing on this track covers `at_secs`.
    pub fn clip_at(&self, at_secs: f64) -> Option<&ClipInstance> {
        self.clips
            .iter()
            .find(|c| at_secs >= c.start_secs && at_secs < c.start_secs + c.duration_secs())
    }

    /// Splits the clip covering `at_secs` (timeline-relative) into two: the original keeps
    /// `id` and has its `source_out_secs` trimmed to the split point; a new clip starting at
    /// `at_secs`, with `new_clip_id` and the rest of the original's source range, is inserted
    /// right after it. No-op (`false`) if no clip on this track covers `at_secs`.
    pub fn split_clip_at(&mut self, at_secs: f64, new_clip_id: u64) -> bool {
        if !at_secs.is_finite() {
            return false;
        }
        let Some(index) = self.clips.iter().position(|c| c.contains(at_secs)) else {
            return false;
        };

        let clip = &mut self.clips[index];
        let source_duration = clip.source_out_secs - clip.source_in_secs;
        let timeline_elapsed = at_secs - clip.start_secs;
        // A smooth ramp's source time isn't linear in timeline time (that's the whole point of
        // the ramp), so the plain `elapsed * speed_factor` a constant-speed clip uses would land
        // the split at the wrong source frame — use the ramp's own inverse instead. This is
        // mathematically identical to the old formula in the non-ramped (overwhelmingly common)
        // case, not a behavior change there: keyframe::smooth_speed_ramp_source_secs_at's own
        // `v1 == v0` fallback is exactly `timeline_elapsed * speed_factor`.
        let split_source_secs = clip.source_in_secs
            + match clip.speed_ramp_end_factor {
                Some(end_speed) => keyframe::smooth_speed_ramp_source_secs_at(
                    timeline_elapsed,
                    source_duration,
                    clip.speed_factor,
                    end_speed,
                ),
                None => timeline_elapsed * clip.speed_factor as f64,
            };
        // Fraction of the *source* range consumed by the split — what keyframes (stored in
        // source-relative `0.0..=1.0`, see evaluate_keyframes' callers) actually need. Equal to
        // the old `(at_secs - start) / clip.duration_secs()` timeline-fraction in the
        // non-ramped case (both reduce to the same ratio when speed is constant), so this is a
        // safe drop-in even though it's phrased differently.
        let split_frac = if source_duration > 1e-9 {
            ((split_source_secs - clip.source_in_secs) / source_duration) as f32
        } else {
            0.0
        };
        // A ramp doesn't survive a split as one continuous curve — each half becomes its own
        // ramp instead, sharing the same speed-at-the-cut boundary so there's no audible/visual
        // jump right at the split (same "no jump at the cut" goal split_keyframes_at's own doc
        // comment describes for keyframes).
        let speed_at_split = clip.speed_ramp_end_factor.map(|end_speed| {
            let b = (end_speed as f64 - clip.speed_factor as f64) / source_duration.max(1e-9);
            (clip.speed_factor as f64 + b * (split_source_secs - clip.source_in_secs)) as f32
        });
        let (position_first, position_second) = keyframe::split_keyframes_at(
            &clip.position_keyframes,
            split_frac,
            Position { x: 0.0, y: 0.0 },
        );
        let (scale_first, scale_second) =
            keyframe::split_keyframes_at(&clip.scale_keyframes, split_frac, 1.0);
        let (rotation_first, rotation_second) =
            keyframe::split_keyframes_at(&clip.rotation_keyframes, split_frac, 0.0);
        let (opacity_first, opacity_second) =
            keyframe::split_keyframes_at(&clip.opacity_keyframes, split_frac, 1.0);
        let (gain_first, gain_second) =
            keyframe::split_keyframes_at(&clip.gain_keyframes, split_frac, 0.0);
        let (brightness_first, brightness_second) =
            keyframe::split_keyframes_at(&clip.brightness_keyframes, split_frac, 0.0);
        let (contrast_first, contrast_second) =
            keyframe::split_keyframes_at(&clip.contrast_keyframes, split_frac, 1.0);
        let (saturation_first, saturation_second) =
            keyframe::split_keyframes_at(&clip.saturation_keyframes, split_frac, 1.0);
        let (crop_x_first, crop_x_second) =
            keyframe::split_keyframes_at(&clip.crop_x_keyframes, split_frac, 0.0);
        let (crop_y_first, crop_y_second) =
            keyframe::split_keyframes_at(&clip.crop_y_keyframes, split_frac, 0.0);
        let (crop_w_first, crop_w_second) =
            keyframe::split_keyframes_at(&clip.crop_w_keyframes, split_frac, 1.0);
        let (crop_h_first, crop_h_second) =
            keyframe::split_keyframes_at(&clip.crop_h_keyframes, split_frac, 1.0);
        let second_half = ClipInstance {
            id: new_clip_id,
            asset_id: clip.asset_id,
            start_secs: at_secs,
            source_in_secs: split_source_secs,
            source_out_secs: clip.source_out_secs,
            // Splitting a composite member must not silently ungroup it from the rest of the
            // block.
            composite_id: clip.composite_id,
            color_label: clip.color_label,
            gain_db: clip.gain_db,
            frozen: clip.frozen,
            // Second half's own ramp starts at the split boundary's speed and rides out to the
            // original clip's end speed, unchanged.
            speed_factor: speed_at_split.unwrap_or(clip.speed_factor),
            speed_ramp_end_factor: clip.speed_ramp_end_factor,
            nested_sequence_id: clip.nested_sequence_id,
            crop_x: clip.crop_x,
            crop_y: clip.crop_y,
            crop_w: clip.crop_w,
            crop_h: clip.crop_h,
            mask_shape: clip.mask_shape,
            mask_corner_radius: clip.mask_corner_radius,
            flipped_h: clip.flipped_h,
            color_filter: clip.color_filter,
            vignette_intensity: clip.vignette_intensity,
            brightness: clip.brightness,
            contrast: clip.contrast,
            saturation: clip.saturation,
            sharpen: clip.sharpen,
            chroma_key_enabled: clip.chroma_key_enabled,
            chroma_key_color: clip.chroma_key_color,
            chroma_key_tolerance: clip.chroma_key_tolerance,
            blur_intensity: clip.blur_intensity,
            shake_intensity: clip.shake_intensity,
            glitch_intensity: clip.glitch_intensity,
            pixelize_intensity: clip.pixelize_intensity,
            // Arguably a freshly-split second half shouldn't inherit an "incoming transition"
            // meant for the original clip's start, but every other field here is propagated
            // unconditionally on split, so this stays consistent with that rather than special-
            // casing it.
            transition_in: clip.transition_in,
            transition_duration_secs: clip.transition_duration_secs,
            // The fade-in belongs to the first half, the fade-out to the second.
            fade_in_secs: 0.0,
            fade_out_secs: clip.fade_out_secs,
            position_keyframes: position_second,
            scale_keyframes: scale_second,
            rotation_keyframes: rotation_second,
            opacity_keyframes: opacity_second,
            gain_keyframes: gain_second,
            voice_cleanup_enabled: clip.voice_cleanup_enabled,
            voice_cleanup_noise_floor_db: clip.voice_cleanup_noise_floor_db,
            voice_cleanup_compressor_threshold_db: clip.voice_cleanup_compressor_threshold_db,
            voice_cleanup_compressor_ratio: clip.voice_cleanup_compressor_ratio,
            voice_cleanup_ceiling_linear: clip.voice_cleanup_ceiling_linear,
            brightness_keyframes: brightness_second,
            contrast_keyframes: contrast_second,
            saturation_keyframes: saturation_second,
            crop_x_keyframes: crop_x_second,
            crop_y_keyframes: crop_y_second,
            crop_w_keyframes: crop_w_second,
            crop_h_keyframes: crop_h_second,
            deflicker_enabled: clip.deflicker_enabled,
            lut_path: clip.lut_path.clone(),
            layer_scale_x: clip.layer_scale_x,
            layer_scale_y: clip.layer_scale_y,
            stabilization_intensity: clip.stabilization_intensity,
            // Not carried over: the matte at `clip.background_removal_mask_path` (if any) was
            // generated for the pre-split `source_in_secs..source_out_secs` range, which no
            // longer matches either half after the split — a stale matte pointing at the wrong
            // frame range, silently wrong. `false`/empty until re-generated for this half.
            background_removal_enabled: false,
            background_removal_mask_path: String::new(),
            blend_mode: clip.blend_mode,
            anchor_x: clip.anchor_x,
            anchor_y: clip.anchor_y,
            // Unlike the matte above, a reframe seed point is a spatial anchor in the frame,
            // not tied to a time range -- still valid for both halves after a split.
            reframe_seed_point: clip.reframe_seed_point,
            // Same staleness reasoning as background_removal_enabled/_mask_path above -- the
            // seed vertices were traced against the pre-split clip's own first frame, which is
            // no longer either half's own first frame after a split.
            privacy_blur_enabled: false,
            privacy_blur_mask_path: String::new(),
            privacy_blur_sigma: clip.privacy_blur_sigma,
            privacy_blur_seed_vertices: Vec::new(),
            privacy_blur_seed_center_x_frac: 0.0,
            privacy_blur_seed_center_y_frac: 0.0,
            grade: Default::default(),
        };
        clip.source_out_secs = split_source_secs;
        clip.fade_out_secs = 0.0;
        // First half's own ramp rides from the original start speed to the split boundary's
        // speed — mirrors the second half's own comment above.
        if let Some(speed_at_split) = speed_at_split {
            clip.speed_ramp_end_factor = Some(speed_at_split);
        }
        clip.position_keyframes = position_first;
        clip.scale_keyframes = scale_first;
        clip.rotation_keyframes = rotation_first;
        clip.opacity_keyframes = opacity_first;
        clip.gain_keyframes = gain_first;
        clip.brightness_keyframes = brightness_first;
        clip.contrast_keyframes = contrast_first;
        clip.saturation_keyframes = saturation_first;
        clip.crop_x_keyframes = crop_x_first;
        clip.crop_y_keyframes = crop_y_first;
        clip.crop_w_keyframes = crop_w_first;
        clip.crop_h_keyframes = crop_h_first;
        // Same staleness reasoning as the second half above — the original clip's own trimmed
        // range changed too.
        clip.background_removal_enabled = false;
        clip.background_removal_mask_path = String::new();
        clip.privacy_blur_enabled = false;
        clip.privacy_blur_mask_path = String::new();
        clip.privacy_blur_seed_vertices = Vec::new();
        clip.privacy_blur_seed_center_x_frac = 0.0;
        clip.privacy_blur_seed_center_y_frac = 0.0;

        self.clips.insert(index + 1, second_half);
        true
    }

    /// Mutable access to the clip with this id, if it's on this track.
    pub fn clip_mut(&mut self, clip_id: u64) -> Option<&mut ClipInstance> {
        self.clips.iter_mut().find(|c| c.id == clip_id)
    }

    /// Mutable access to the text clip with this id, if it's on this track. Mirrors
    /// [`Track::clip_mut`] for [`TextClip`], since text overlays get the same
    /// drag-to-move/drag-to-trim treatment as ordinary clips on the timeline strip.
    pub fn text_clip_mut(&mut self, text_clip_id: u64) -> Option<&mut TextClip> {
        self.text_clips.iter_mut().find(|t| t.id == text_clip_id)
    }

    /// Mutable access to the shape clip with this id, if it's on this track. Mirrors
    /// [`Track::clip_mut`] for [`ShapeClip`].
    pub fn shape_clip_mut(&mut self, shape_clip_id: u64) -> Option<&mut ShapeClip> {
        self.shape_clips.iter_mut().find(|s| s.id == shape_clip_id)
    }

    /// Repositions the clip with `clip_id` to `new_start_secs` on this same track — what
    /// dragging a clip's body (not one of its edges) does. Doesn't check for overlap with
    /// neighboring clips (matches this struct's existing no-overlap-checking policy, see
    /// above) — dragging one clip onto another just lets them overlap for now. No-op
    /// (`false`) if the clip isn't on this track or `new_start_secs` is negative.
    pub fn move_clip(&mut self, clip_id: u64, new_start_secs: f64) -> bool {
        if !new_start_secs.is_finite() || new_start_secs < 0.0 {
            return false;
        }
        let Some(clip) = self.clip_mut(clip_id) else {
            return false;
        };
        clip.start_secs = new_start_secs;
        true
    }

    /// The id of the clip immediately before `clip_id` on this track (the one with the
    /// greatest `start_secs` that's still less than `clip_id`'s own) — `None` if `clip_id`
    /// isn't on this track or is already the earliest one. Shared by the roll/slide edits
    /// below to find which neighbor a shared edge/absorbed gap belongs to.
    pub fn previous_clip_id(&self, clip_id: u64) -> Option<u64> {
        let this_start = self.clips.iter().find(|c| c.id == clip_id)?.start_secs;
        self.clips
            .iter()
            .filter(|c| c.id != clip_id && c.start_secs < this_start)
            .max_by(|a, b| a.start_secs.total_cmp(&b.start_secs))
            .map(|c| c.id)
    }

    /// The id of the clip immediately after `clip_id` on this track, by the mirror-image rule
    /// [`Self::previous_clip_id`] uses.
    pub fn next_clip_id(&self, clip_id: u64) -> Option<u64> {
        let this_start = self.clips.iter().find(|c| c.id == clip_id)?.start_secs;
        self.clips
            .iter()
            .filter(|c| c.id != clip_id && c.start_secs > this_start)
            .min_by(|a, b| a.start_secs.total_cmp(&b.start_secs))
            .map(|c| c.id)
    }

    /// Trims `clip_id`'s start to `new_start_secs`, then shifts every clip whose `start_secs`
    /// is past `clip_id`'s own (pre-trim) start by the same delta — Premiere/DaVinci/FCP's
    /// "Ripple" tool (`ROADMAP.md` P2 item 11): no gap is left behind, later clips slide to
    /// fill it. No-op (`false`) if the clip isn't on this track or the underlying
    /// [`ClipInstance::trim_start`] refuses the edit — nothing is shifted in that case.
    pub fn ripple_trim_start(
        &mut self,
        clip_id: u64,
        new_start_secs: f64,
        min_duration_secs: f64,
    ) -> bool {
        let Some(index) = self.clips.iter().position(|c| c.id == clip_id) else {
            return false;
        };
        let old_start_secs = self.clips[index].start_secs;
        if !self.clips[index].trim_start(new_start_secs, min_duration_secs) {
            return false;
        }
        let delta = new_start_secs - old_start_secs;
        for clip in &mut self.clips {
            if clip.id != clip_id && clip.start_secs > old_start_secs {
                clip.start_secs = (clip.start_secs + delta).max(0.0);
            }
        }
        true
    }

    /// [`Self::ripple_trim_start`]'s mirror for the clip's *end* edge: trims `clip_id`'s end to
    /// `new_end_secs`, then shifts every clip whose `start_secs` is past `clip_id`'s own by the
    /// resulting duration delta.
    pub fn ripple_trim_end(
        &mut self,
        clip_id: u64,
        new_end_secs: f64,
        min_duration_secs: f64,
        max_source_out_secs: Option<f64>,
    ) -> bool {
        let Some(index) = self.clips.iter().position(|c| c.id == clip_id) else {
            return false;
        };
        let this_start = self.clips[index].start_secs;
        let old_end_secs = this_start + self.clips[index].duration_secs();
        if !self.clips[index].trim_end(new_end_secs, min_duration_secs, max_source_out_secs) {
            return false;
        }
        let delta = new_end_secs - old_end_secs;
        for clip in &mut self.clips {
            if clip.id != clip_id && clip.start_secs > this_start {
                clip.start_secs = (clip.start_secs + delta).max(0.0);
            }
        }
        true
    }

    /// Moves the cut point between `clip_id` and its immediate next neighbor
    /// ([`Self::next_clip_id`]) to `new_boundary_secs` — Premiere/DaVinci/FCP's "Roll" tool
    /// (`ROADMAP.md` P2 item 11): `clip_id`'s end and the neighbor's start move together, so
    /// the pair's combined timeline span (and every other clip's position) stays unchanged.
    /// No-op (`false`) if `clip_id` has no next neighbor on this track, or the edit would
    /// violate either clip's own trim bounds — applied atomically: a rejection on either side
    /// leaves both clips exactly as they were, never a half-rolled pair.
    pub fn roll_edit(
        &mut self,
        clip_id: u64,
        new_boundary_secs: f64,
        min_duration_secs: f64,
        this_max_source_out_secs: Option<f64>,
    ) -> bool {
        let Some(index) = self.clips.iter().position(|c| c.id == clip_id) else {
            return false;
        };
        let Some(next_id) = self.next_clip_id(clip_id) else {
            return false;
        };
        let next_index = self
            .clips
            .iter()
            .position(|c| c.id == next_id)
            .expect("next_clip_id only ever returns an id present on this track");

        let this_snapshot = self.clips[index].clone();
        let next_snapshot = self.clips[next_index].clone();
        let next_old_start = self.clips[next_index].start_secs;

        let this_ok = self.clips[index].trim_end(
            new_boundary_secs,
            min_duration_secs,
            this_max_source_out_secs,
        );
        let next_delta = new_boundary_secs - next_old_start;
        let next_new_start = next_old_start + next_delta;
        // trim_start never needs a max-source-out bound: it only moves source_in toward
        // source_out (which stays fixed), never past it.
        let next_ok =
            this_ok && self.clips[next_index].trim_start(next_new_start, min_duration_secs);

        if !next_ok {
            self.clips[index] = this_snapshot;
            self.clips[next_index] = next_snapshot;
            return false;
        }
        true
    }

    /// Moves `clip_id` to `new_start_secs` on this track, keeping its own duration/source
    /// range unchanged — Premiere/DaVinci/FCP's "Slide" tool (`ROADMAP.md` P2 item 11): the
    /// immediate previous and next clips ([`Self::previous_clip_id`]/[`Self::next_clip_id`],
    /// resolved against `clip_id`'s *pre-move* position) absorb the movement by adjusting their
    /// own out/in points to meet `clip_id`'s new position, so nothing else on the track shifts.
    /// No-op (`false`) if `new_start_secs` is negative, `clip_id` isn't on this track, or
    /// either affected neighbor's own trim bounds would be violated — applied atomically, same
    /// as [`Self::roll_edit`]. A neighbor that doesn't exist (`clip_id` is first/last on the
    /// track) simply isn't adjusted on that side.
    pub fn slide_clip(
        &mut self,
        clip_id: u64,
        new_start_secs: f64,
        min_duration_secs: f64,
        prev_max_source_out_secs: Option<f64>,
    ) -> bool {
        if !new_start_secs.is_finite() || new_start_secs < 0.0 {
            return false;
        }
        let Some(index) = self.clips.iter().position(|c| c.id == clip_id) else {
            return false;
        };
        let this_duration = self.clips[index].duration_secs();
        let new_end_secs = new_start_secs + this_duration;

        let prev_index = self
            .previous_clip_id(clip_id)
            .and_then(|id| self.clips.iter().position(|c| c.id == id));
        let next_index = self
            .next_clip_id(clip_id)
            .and_then(|id| self.clips.iter().position(|c| c.id == id));

        let snapshots: Vec<(usize, ClipInstance)> = [Some(index), prev_index, next_index]
            .into_iter()
            .flatten()
            .map(|i| (i, self.clips[i].clone()))
            .collect();

        let prev_ok = match prev_index {
            Some(i) => {
                self.clips[i].trim_end(new_start_secs, min_duration_secs, prev_max_source_out_secs)
            }
            None => true,
        };
        // next's trim_start never needs a max-source-out bound, same reasoning as roll_edit.
        let next_ok = prev_ok
            && match next_index {
                Some(i) => self.clips[i].trim_start(new_end_secs, min_duration_secs),
                None => true,
            };

        if !next_ok {
            for (i, snapshot) in snapshots {
                self.clips[i] = snapshot;
            }
            return false;
        }
        self.clips[index].start_secs = new_start_secs;
        true
    }

    /// The lane of every clip, by id: the clips of a track normally follow one another (lane 0),
    /// but a clip that starts before an earlier clip has ended (a cross-dissolve overlap) goes to
    /// the first lane that is free, so each lane is a plain sequence. Export and preview render
    /// each lane as its own layer above the lane before it.
    pub fn overlap_lanes(&self) -> std::collections::HashMap<u64, usize> {
        let mut order: Vec<&ClipInstance> = self.clips.iter().collect();
        order.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs).then(a.id.cmp(&b.id)));
        let mut lane_ends: Vec<f64> = Vec::new();
        let mut lanes = std::collections::HashMap::with_capacity(order.len());
        for clip in order {
            let lane = lane_ends
                .iter()
                .position(|&end| end <= clip.start_secs + OVERLAP_EPSILON_SECS)
                .unwrap_or(lane_ends.len());
            let end = clip.start_secs + clip.duration_secs();
            if lane == lane_ends.len() {
                lane_ends.push(end);
            } else {
                lane_ends[lane] = end;
            }
            lanes.insert(clip.id, lane);
        }
        lanes
    }

    /// How many seconds `clip_id` starts inside the clips before it (`0.0` when it follows them
    /// or the id is unknown) -- the span of a cross-dissolve.
    pub fn overlap_with_earlier_secs(&self, clip_id: u64) -> f64 {
        let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) else {
            return 0.0;
        };
        self.clips
            .iter()
            .filter(|other| other.id != clip_id && other.start_secs < clip.start_secs)
            .map(|other| other.start_secs + other.duration_secs() - clip.start_secs)
            .fold(0.0, f64::max)
    }

    /// The clips of this track cut into the pieces the exporter composites, each with its layer:
    /// a clip starting before an earlier clip has ended is split where that clip ends. The part
    /// that overlaps (the head) goes to an overlay lane (1 and up) and carries the clip's
    /// dissolve as a fade-in; the part after (the tail) continues the base lane (0), so the base
    /// lane stays a plain sequence covering the whole timeline. A clip inside an earlier one is
    /// all head. The split is `split_clip_at`'s, so source ranges, speed ramps and keyframes follow it.
    pub fn lane_pieces(&self) -> Vec<(ClipInstance, usize)> {
        let mut order: Vec<&ClipInstance> = self.clips.iter().collect();
        order.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs).then(a.id.cmp(&b.id)));
        let mut pieces = Vec::with_capacity(order.len());
        let mut covered_until = f64::NEG_INFINITY;
        // End of the last head on each overlay lane (index 0 is lane 1).
        let mut overlay_ends: Vec<f64> = Vec::new();
        for clip in order {
            let end = clip.start_secs + clip.duration_secs();
            if clip.start_secs >= covered_until - OVERLAP_EPSILON_SECS {
                pieces.push((clip.clone(), 0));
                covered_until = covered_until.max(end);
                continue;
            }
            let head_end = end.min(covered_until);
            let has_tail = end > covered_until + OVERLAP_EPSILON_SECS;
            // `split_clip_at` does the whole split (source range, speed ramp, keyframes rescaled
            // over each half), so a keyframed clip animates the same across both pieces.
            let (mut head, tail) = if has_tail {
                let mut scratch = Track {
                    clips: vec![clip.clone()],
                    text_clips: Vec::new(),
                    shape_clips: Vec::new(),
                    ..self.clone()
                };
                if scratch.split_clip_at(head_end, u64::MAX) {
                    let tail = scratch.clips.pop().map(|mut tail| {
                        tail.id = clip.id;
                        tail
                    });
                    (scratch.clips.remove(0), tail)
                } else {
                    (clip.clone(), None)
                }
            } else {
                (clip.clone(), None)
            };
            head.fade_in_secs = clip.fade_in_secs.max(self.dissolve_in_secs(clip) as f32);
            head.transition_in = TransitionType::None;
            let lane = overlay_ends
                .iter()
                .position(|&lane_end| lane_end <= clip.start_secs + OVERLAP_EPSILON_SECS)
                .unwrap_or(overlay_ends.len());
            if lane == overlay_ends.len() {
                overlay_ends.push(head_end);
            } else {
                overlay_ends[lane] = head_end;
            }
            pieces.push((head, lane + 1));
            if let Some(mut tail) = tail {
                tail.fade_in_secs = 0.0;
                tail.transition_in = TransitionType::None;
                pieces.push((tail, 0));
                covered_until = end;
            }
        }
        pieces
    }

    /// The fade-in a [`TransitionType::Dissolve`] gives `clip`: its transition duration, cut to
    /// the span it really overlaps the clips before it (the whole duration when it overlaps
    /// nothing, where it acts as a plain fade-in). `0.0` for any other transition.
    pub fn dissolve_in_secs(&self, clip: &ClipInstance) -> f64 {
        if clip.transition_in != TransitionType::Dissolve {
            return 0.0;
        }
        let duration = f64::from(clip.transition_duration_secs).max(0.0);
        let overlap = self.overlap_with_earlier_secs(clip.id);
        if overlap > OVERLAP_EPSILON_SECS {
            duration.min(overlap)
        } else {
            duration
        }
    }

    /// Whether `next` carries on from the end of `prev` without a jump: the same asset, touching
    /// on the timeline, the source resuming where it stopped at the same plain speed. A split
    /// leaves two such clips, and their join needs no fade.
    fn continues(prev: &ClipInstance, next: &ClipInstance) -> bool {
        const TOLERANCE_SECS: f64 = 1e-3;
        prev.resolved_asset_id() == next.resolved_asset_id()
            && prev.nested_sequence_id == next.nested_sequence_id
            && prev.speed_ramp_end_factor.is_none()
            && next.speed_ramp_end_factor.is_none()
            && (prev.speed_factor - next.speed_factor).abs() < 1e-6
            && (prev.start_secs + prev.duration_secs() - next.start_secs).abs() < TOLERANCE_SECS
            && (prev.source_out_secs - next.source_in_secs).abs() < TOLERANCE_SECS
    }

    /// Whether the audio of `clip` meets another clip's at `at_secs` (its start or end) with a
    /// jump in the sound: the clips touch but do not continue each other.
    fn audio_cut_at(&self, clip: &ClipInstance, at_start: bool) -> bool {
        const TOLERANCE_SECS: f64 = 1e-3;
        self.clips.iter().any(|other| {
            if other.id == clip.id {
                return false;
            }
            if at_start {
                (other.start_secs + other.duration_secs() - clip.start_secs).abs() < TOLERANCE_SECS
                    && !Self::continues(other, clip)
            } else {
                (clip.start_secs + clip.duration_secs() - other.start_secs).abs() < TOLERANCE_SECS
                    && !Self::continues(clip, other)
            }
        })
    }

    /// Whether `clip`'s sound is cut hard against a neighbour on either side, which the mix
    /// smooths with a short fade ([`Self::AUDIO_DECLICK_SECS`]).
    pub fn audio_has_hard_cut(&self, clip: &ClipInstance) -> bool {
        self.audio_cut_at(clip, true) || self.audio_cut_at(clip, false)
    }

    /// Length of the short fade that keeps a hard cut between two clips from clicking.
    pub const AUDIO_DECLICK_SECS: f32 = 0.010;

    /// The fade-in of `clip`'s audio: its own fade handle, a dissolve into it, or the declick
    /// where it is cut hard against the clip before it, whichever is longest.
    pub fn audio_fade_in_secs(&self, clip: &ClipInstance) -> f32 {
        let declick = if self.audio_cut_at(clip, true) {
            Self::AUDIO_DECLICK_SECS
        } else {
            0.0
        };
        clip.fade_in_secs
            .max(self.dissolve_in_secs(clip) as f32)
            .max(declick)
    }

    /// The fade-out of `clip`'s audio, by the same rule as [`Self::audio_fade_in_secs`].
    pub fn audio_fade_out_secs(&self, clip: &ClipInstance) -> f32 {
        let declick = if self.audio_cut_at(clip, false) {
            Self::AUDIO_DECLICK_SECS
        } else {
            0.0
        };
        clip.fade_out_secs
            .max(self.dissolve_out_secs(clip) as f32)
            .max(declick)
    }

    /// The fade-out of `clip`'s audio under a later clip's dissolve: how long the next dissolving
    /// clip overlaps its tail (cut to that clip's transition duration). `0.0` when none does.
    pub fn dissolve_out_secs(&self, clip: &ClipInstance) -> f64 {
        let end = clip.start_secs + clip.duration_secs();
        self.clips
            .iter()
            .filter(|other| {
                other.id != clip.id
                    && other.transition_in == TransitionType::Dissolve
                    && other.start_secs >= clip.start_secs
                    && other.start_secs < end - OVERLAP_EPSILON_SECS
            })
            .map(|other| (end - other.start_secs).min(f64::from(other.transition_duration_secs)))
            .fold(0.0, f64::max)
    }

    /// Turns the cut before `clip_id` into an overlap of up to `duration_secs` (never more than
    /// half of either clip): the clip and every clip after it on this track move earlier, so the
    /// previous clip's tail and this clip's head play together. `false` (nothing changes) when no
    /// clip ends right where this one starts.
    pub fn apply_dissolve_overlap(&mut self, clip_id: u64, duration_secs: f64) -> bool {
        let Some(clip) = self.clips.iter().find(|c| c.id == clip_id) else {
            return false;
        };
        let (start, own_duration) = (clip.start_secs, clip.duration_secs());
        let adjacent_previous = self
            .clips
            .iter()
            .filter(|other| other.id != clip_id && other.start_secs < start)
            .map(|other| {
                (
                    other.start_secs + other.duration_secs(),
                    other.duration_secs(),
                )
            })
            .filter(|(end, _)| (end - start).abs() <= DISSOLVE_ADJACENT_SECS)
            .max_by(|a, b| a.0.total_cmp(&b.0));
        let Some((_, previous_duration)) = adjacent_previous else {
            return false;
        };
        let shift = duration_secs
            .min(previous_duration / 2.0)
            .min(own_duration / 2.0);
        if !shift.is_finite() || shift <= OVERLAP_EPSILON_SECS {
            return false;
        }
        for later in self
            .clips
            .iter_mut()
            .filter(|c| c.start_secs >= start - OVERLAP_EPSILON_SECS)
        {
            later.start_secs -= shift;
        }
        true
    }

    /// Removes the timeline range `[start_secs, end_secs)` from this track and ripples every
    /// later clip left to close the gap — "ripple delete," what D1's silence-gap review
    /// (`spec/architecture/differentiators.md`) applies to each accepted
    /// [`crate::silence_detection::SilenceGap`]. Any clip straddling either boundary is split
    /// first via [`Self::split_clip_at`] (each split, if performed, consumes one id from
    /// `next_clip_id` and increments it — a no-op split at an exact boundary leaves it
    /// untouched), then every clip now falling fully inside the range is dropped, and every
    /// clip starting at or after `end_secs` shifts left by the removed span. Only `self.clips`
    /// is affected, matching [`Self::ripple_trim_start`]/[`Self::ripple_trim_end`]'s existing
    /// scope (text/shape overlay tracks are never video/audio tracks, so this never applies to
    /// them). No-op (`false`, `next_clip_id` untouched) if `end_secs <= start_secs`; also `false` when the range held nothing to remove or move.
    pub fn ripple_delete_range(
        &mut self,
        start_secs: f64,
        end_secs: f64,
        next_clip_id: &mut u64,
    ) -> bool {
        if !start_secs.is_finite() || !end_secs.is_finite() || end_secs <= start_secs {
            return false;
        }

        let before = self.clips.clone();
        if self.split_clip_at(start_secs, *next_clip_id) {
            *next_clip_id += 1;
        }
        if self.split_clip_at(end_secs, *next_clip_id) {
            *next_clip_id += 1;
        }

        const EPSILON: f64 = 1e-6;
        let removed_span = end_secs - start_secs;
        self.clips.retain(|c| {
            let c_end = c.start_secs + c.duration_secs();
            !(c.start_secs >= start_secs - EPSILON && c_end <= end_secs + EPSILON)
        });
        for clip in &mut self.clips {
            if clip.start_secs >= end_secs - EPSILON {
                clip.start_secs = (clip.start_secs - removed_span).max(0.0);
            }
        }
        // A range with no clip in it and none after it changes nothing: no edit to report.
        self.clips != before
    }

    /// Removes the timeline range `[start_secs, end_secs)` from this track and leaves the gap
    /// open: the "lift" counterpart of [`Self::ripple_delete_range`]. Clips straddling a boundary
    /// are split the same way (consuming ids from `next_clip_id`), clips fully inside the range
    /// are dropped, and nothing moves. No-op (`false`) if `end_secs <= start_secs`.
    pub fn lift_range(&mut self, start_secs: f64, end_secs: f64, next_clip_id: &mut u64) -> bool {
        if !start_secs.is_finite() || !end_secs.is_finite() || end_secs <= start_secs {
            return false;
        }
        let before = self.clips.clone();
        if self.split_clip_at(start_secs, *next_clip_id) {
            *next_clip_id += 1;
        }
        if self.split_clip_at(end_secs, *next_clip_id) {
            *next_clip_id += 1;
        }
        const EPSILON: f64 = 1e-6;
        self.clips.retain(|c| {
            let c_end = c.start_secs + c.duration_secs();
            !(c.start_secs >= start_secs - EPSILON && c_end <= end_secs + EPSILON)
        });
        self.clips != before
    }

    /// The position, in seconds, where this track's last clip ends. `0.0` for an empty track —
    /// the natural "append here" position for a clip added to this track. Accounts for
    /// [`ClipInstance`]s, [`TextClip`]s, and [`ShapeClip`]s so every track kind reports its own
    /// length correctly.
    pub fn duration_secs(&self) -> f64 {
        let clips_end = self
            .clips
            .iter()
            .map(|c| c.start_secs + c.duration_secs())
            .fold(0.0, f64::max);
        let text_end = self
            .text_clips
            .iter()
            .map(|t| t.start_secs + t.duration_secs)
            .fold(0.0, f64::max);
        let shape_end = self
            .shape_clips
            .iter()
            .map(|s| s.start_secs + s.duration_secs)
            .fold(0.0, f64::max);
        clips_end.max(text_end).max(shape_end)
    }
}
