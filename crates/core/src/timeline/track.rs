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
use crate::timeline::shape_clip::ShapeClip;
use crate::timeline::text_clip::TextClip;

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
        };
        clip.source_out_secs = split_source_secs;
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
        if new_start_secs < 0.0 {
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
        if new_start_secs < 0.0 {
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
    /// them). No-op (`false`, `next_clip_id` untouched) if `end_secs <= start_secs`.
    pub fn ripple_delete_range(
        &mut self,
        start_secs: f64,
        end_secs: f64,
        next_clip_id: &mut u64,
    ) -> bool {
        if end_secs <= start_secs {
            return false;
        }

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
        true
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
