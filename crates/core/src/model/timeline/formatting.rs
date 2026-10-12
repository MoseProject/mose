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

use crate::keyframe::{Keyframe, Position};
use crate::timeline::TrackKind;

use super::clip::ClipInstance;
use super::effects::{BlendMode, ColorFilter, MaskShape, TransitionType};

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
    /// The colour grade; `#[serde(default)]` so saved layer templates without one load neutral.
    #[serde(default)]
    pub grade: super::grade::Grade,
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

impl ClipInstance {
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
            grade: self.grade,
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
        self.grade = f.grade;
        self.layer_scale_x = f.layer_scale_x;
        self.layer_scale_y = f.layer_scale_y;
        self.stabilization_intensity = f.stabilization_intensity;
        self.blend_mode = f.blend_mode;
        self.anchor_x = f.anchor_x;
        self.anchor_y = f.anchor_y;
    }
}
