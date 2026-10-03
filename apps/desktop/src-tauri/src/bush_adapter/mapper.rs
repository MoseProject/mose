// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Maps the session timeline to an immutable Bush [`SceneSnapshot`].
//!
//! Every clip of every visible video/audio track becomes one contributor with its timeline
//! window, so Bush (not the adapter) decides which ones are active at the playhead. Properties
//! Bush has no exact equivalent for are approximated and reported in [`Mapped::approximations`]
//! so a parity run can say which look differences are expected rather than bugs.
//!
//! Intensity-to-parameter curves (blur, sharpen, pixelize, shake, chroma softness, deflicker,
//! stabilization) are provisional constants until side-by-side frames against the GStreamer
//! preview fix them (RFC gap 9).

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use avcore::keyframe::{evaluate_keyframes, Keyframe as CoreKeyframe, Position};
use avcore::timeline::{
    BlendMode as CoreBlend, ClipInstance, ColorFilter, MaskShape as CoreMask, Timeline, TrackKind,
    TransitionType,
};
use avcore::{MediaAsset, MediaKind};
use bush::{
    Animation, BlendMode, ChromaKey, Contributor, ContributorContent, ContributorId, CropRect,
    Easing, Fade, Keyframe, KeyframeTrack, LayerEffects, LayerMask, LayerTransform, Lut3D,
    MaskShape, MediaSource, Placement, SceneError, SceneSnapshot, SceneVersions, SourceRange,
    Version, MAX_BLUR_SIGMA,
};

/// Source span a frozen clip advances over its whole window; the decoder keeps serving the same
/// frame while the (near zero) speed walks through it.
const FROZEN_SOURCE_SPAN_SECS: f64 = 1e-3;
const BLUR_SIGMA_AT_FULL: f32 = 20.0;
const SHARPEN_AMOUNT_AT_FULL: f32 = 2.0;
const SHAKE_PIXELS_AT_FULL: f32 = 40.0;
const MAX_SHAKE_PIXELS: f32 = 200.0;
const CHROMA_SOFTNESS: f32 = 0.05;
const DEFLICKER_WEIGHT: f32 = 0.15;
const PIXELIZE_MIN_BLOCK: f32 = 2.0;
const PIXELIZE_BLOCK_RANGE: f32 = 46.0;
/// Ken-Burns zoom is sampled densely because crop width is `1 / zoom`, not linear in zoom.
const ZOOM_SAMPLES_PER_SEC: f64 = 15.0;
const MAX_ZOOM_SAMPLES: usize = 300;
const TRANSITION_ZOOM_START: f32 = 0.5;

/// What the mapper needs from the app: where a clip's media lives and parsed LUT tables.
pub trait SourceResolver {
    /// The file to decode for `asset` (its proxy when one exists), `None` if missing.
    fn source_path(&mut self, asset: &MediaAsset) -> Option<PathBuf>;
    /// The parsed `.cube` table at `path`, `None` if it cannot be read.
    fn lut(&mut self, path: &str) -> Option<Arc<Lut3D>>;
}

/// A property whose Bush rendering differs from the GStreamer preview or export.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Approximation {
    /// Core ramps speed linearly in source time, Bush in timeline time: the clip plays at its
    /// average speed so it still occupies the same timeline window.
    SpeedRamp,
    /// Bush applies one static gain; the first gain keyframe is used.
    GainKeyframes,
    /// A frozen clip is silent in Bush.
    FrozenAudioMuted,
    /// Ken-Burns zoom is a crop, so zooming out below 1x is held at 1x.
    ZoomOutHeld,
    /// Scale keyframes are ignored when crop keyframes already drive the crop window.
    ScaleWithCropKeyframes,
    /// A slide or zoom transition is skipped because position or scale keyframes own that axis.
    TransitionIgnored,
    /// Background-removal and privacy-blur mattes are not rendered.
    Matte,
}

pub struct Mapped {
    pub scene: SceneSnapshot,
    pub approximations: BTreeSet<Approximation>,
}

struct MappedClip {
    range: SourceRange,
    placement: Placement,
}

/// Builds the scene for the whole timeline. `canvas` is the preview canvas in pixels.
pub fn map_timeline(
    timeline: &Timeline,
    media: &[MediaAsset],
    canvas: (u32, u32),
    resolver: &mut dyn SourceResolver,
    versions: SceneVersions,
) -> Result<Mapped, SceneError> {
    let mut approximations = BTreeSet::new();
    let mut contributors = Vec::new();
    for (index, track) in timeline
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.visible)
    {
        let on_video_track = match track.kind {
            TrackKind::Video => true,
            TrackKind::Audio => false,
            TrackKind::Text | TrackKind::Shape => continue,
        };
        for clip in &track.clips {
            let Some(asset) = media
                .iter()
                .find(|asset| asset.id == clip.resolved_asset_id())
            else {
                continue;
            };
            let Some(path) = resolver.source_path(asset) else {
                continue;
            };
            let Some(mapped) = map_clip(clip, canvas, resolver, &mut approximations) else {
                continue;
            };
            contributors.push(Contributor {
                id: ContributorId(clip.id),
                layer: i32::try_from(index).unwrap_or(i32::MAX),
                visible: on_video_track && asset.kind == MediaKind::Video,
                audible: asset.has_audio && !clip.frozen,
                content: ContributorContent::Media(MediaSource {
                    path,
                    range: mapped.range,
                    version: Version(asset.id),
                }),
                placement: mapped.placement,
            });
        }
    }
    Ok(Mapped {
        scene: SceneSnapshot::new(versions, contributors)?,
        approximations,
    })
}

fn map_clip(
    clip: &ClipInstance,
    canvas: (u32, u32),
    resolver: &mut dyn SourceResolver,
    approximations: &mut BTreeSet<Approximation>,
) -> Option<MappedClip> {
    let duration = clip.duration_secs();
    if !duration.is_finite() || duration <= 0.0 {
        return None;
    }
    let (range, speed) = if clip.frozen {
        approximations.insert(Approximation::FrozenAudioMuted);
        (
            SourceRange::new(
                clip.source_in_secs,
                clip.source_in_secs + FROZEN_SOURCE_SPAN_SECS,
            )
            .ok()?,
            FROZEN_SOURCE_SPAN_SECS / duration,
        )
    } else {
        let range = SourceRange::new(clip.source_in_secs, clip.source_out_secs).ok()?;
        let speed = if clip.speed_ramp_end_factor.is_some() {
            approximations.insert(Approximation::SpeedRamp);
            (clip.source_out_secs - clip.source_in_secs) / duration
        } else {
            f64::from(clip.speed_factor.max(0.01))
        };
        (range, speed)
    };

    let (canvas_width, canvas_height) = (canvas.0 as f32, canvas.1 as f32);
    let mut animation = Animation::default();
    let mut placement = Placement {
        timeline_start_secs: clip.start_secs,
        speed,
        fit_canvas: true,
        ..Placement::default()
    };

    // Opacity, position, rotation.
    animation.opacity = track_of(&clip.opacity_keyframes, duration, |v: f32| {
        v.clamp(0.0, 1.0)
    });
    animation.x = track_of(&clip.position_keyframes, duration, |p: Position| {
        p.x * canvas_width
    });
    animation.y = track_of(&clip.position_keyframes, duration, |p: Position| {
        p.y * canvas_height
    });
    animation.rotation_deg = track_of(&clip.rotation_keyframes, duration, |v: f32| v);

    // Crop window, static then animated.
    let crop_width = clip.crop_w.clamp(0.01, 1.0);
    let crop_height = clip.crop_h.clamp(0.01, 1.0);
    let crop = CropRect {
        x: clip.crop_x.clamp(0.0, 1.0 - crop_width),
        y: clip.crop_y.clamp(0.0, 1.0 - crop_height),
        width: crop_width,
        height: crop_height,
    };
    animation.crop_x = track_of(&clip.crop_x_keyframes, duration, |v: f32| v.clamp(0.0, 1.0));
    animation.crop_y = track_of(&clip.crop_y_keyframes, duration, |v: f32| v.clamp(0.0, 1.0));
    animation.crop_width = track_of(&clip.crop_w_keyframes, duration, |v: f32| {
        v.clamp(0.01, 1.0)
    });
    animation.crop_height = track_of(&clip.crop_h_keyframes, duration, |v: f32| {
        v.clamp(0.01, 1.0)
    });
    if !clip.scale_keyframes.is_empty() {
        if animation.crop_x.is_some()
            || animation.crop_y.is_some()
            || animation.crop_width.is_some()
            || animation.crop_height.is_some()
        {
            approximations.insert(Approximation::ScaleWithCropKeyframes);
        } else {
            zoom_into_crop(clip, crop, duration, &mut animation, approximations);
        }
    }

    placement.transform = LayerTransform {
        scale_x: clip.layer_scale_x.max(0.01),
        scale_y: clip.layer_scale_y.max(0.01),
        rotation_deg: 0.0,
        crop,
        flip_x: clip.flipped_h,
        flip_y: false,
    };

    // Colour.
    let mut effects = LayerEffects {
        brightness: clip.brightness.clamp(-1.0, 1.0),
        contrast: clip.contrast.clamp(0.0, 4.0),
        saturation: clip.saturation.clamp(0.0, 4.0),
        vignette: clip.vignette_intensity.clamp(0.0, 1.0),
        ..LayerEffects::default()
    };
    animation.brightness = track_of(&clip.brightness_keyframes, duration, |v: f32| {
        v.clamp(-1.0, 1.0)
    });
    animation.contrast = track_of(&clip.contrast_keyframes, duration, |v: f32| {
        v.clamp(0.0, 4.0)
    });
    animation.saturation = track_of(&clip.saturation_keyframes, duration, |v: f32| {
        v.clamp(0.0, 4.0)
    });
    match clip.color_filter {
        ColorFilter::None => {}
        ColorFilter::BlackAndWhite => {
            effects.saturation = 0.0;
            animation.saturation = None;
        }
        ColorFilter::Sepia => effects.sepia = 1.0,
    }
    effects.pixelize_block = pixelize_block(clip.pixelize_intensity);
    if clip.chroma_key_enabled {
        effects.chroma_key = Some(ChromaKey {
            color: clip.chroma_key_color,
            tolerance: clip.chroma_key_tolerance.clamp(0.0, 1.0),
            softness: CHROMA_SOFTNESS,
        });
    }
    effects.mask = match clip.mask_shape {
        CoreMask::None => None,
        CoreMask::Circle => Some(LayerMask {
            shape: MaskShape::Ellipse,
            corner_radius: 0.0,
            feather: 0.0,
            invert: false,
        }),
        CoreMask::RoundedRect => Some(LayerMask {
            shape: MaskShape::Rectangle,
            corner_radius: clip.mask_corner_radius.clamp(0.0, 1.0),
            feather: 0.0,
            invert: false,
        }),
    };
    if clip.background_removal_enabled || clip.privacy_blur_enabled {
        approximations.insert(Approximation::Matte);
    }
    placement.effects = effects;
    placement.blend = blend_of(clip.blend_mode);
    if !clip.lut_path.is_empty() {
        placement.lut = resolver.lut(&clip.lut_path);
    }

    // Filters that live on the placement itself.
    placement.blur_sigma =
        (clip.blur_intensity.clamp(0.0, 1.0) * BLUR_SIGMA_AT_FULL).min(MAX_BLUR_SIGMA);
    placement.sharpen = clip.sharpen.clamp(0.0, 1.0) * SHARPEN_AMOUNT_AT_FULL;
    placement.glitch = clip.glitch_intensity.clamp(0.0, 1.0);
    placement.shake =
        (clip.shake_intensity.clamp(0.0, 1.0) * SHAKE_PIXELS_AT_FULL).min(MAX_SHAKE_PIXELS);
    if clip.deflicker_enabled {
        placement.deflicker = Some(DEFLICKER_WEIGHT);
    }
    if clip.stabilization_intensity > 0.0 {
        placement.stabilize = Some((1.0 - 0.9 * clip.stabilization_intensity).clamp(0.05, 1.0));
    }

    // Audio gain.
    placement.gain = match clip.gain_keyframes.first() {
        Some(first) => {
            approximations.insert(Approximation::GainKeyframes);
            10f32.powf(first.value / 20.0)
        }
        None => clip.gain_linear(),
    }
    .clamp(0.0, 4.0);

    apply_transition(
        clip,
        duration,
        canvas_width,
        &mut placement,
        &mut animation,
        approximations,
    );
    placement.animation = animation;
    Some(MappedClip { range, placement })
}

/// Evaluates core's piecewise-linear keyframes (fractions of the clip's timeline duration) as a
/// Bush linear track in seconds from the clip start. `None` when there are no usable keyframes.
fn track_of<T: Copy>(
    keyframes: &[CoreKeyframe<T>],
    duration: f64,
    value: impl Fn(T) -> f32,
) -> Option<KeyframeTrack> {
    let mut mapped: Vec<Keyframe> = Vec::with_capacity(keyframes.len());
    for keyframe in keyframes {
        let time_secs = f64::from(keyframe.time_fraction.clamp(0.0, 1.0)) * duration;
        let value = value(keyframe.value);
        if !value.is_finite()
            || mapped
                .last()
                .is_some_and(|last| time_secs <= last.time_secs)
        {
            continue;
        }
        mapped.push(Keyframe {
            time_secs,
            value,
            easing: Easing::Linear,
        });
    }
    KeyframeTrack::new(mapped).ok()
}

/// Ken-Burns zoom as an animated crop window centred on the base crop, so it stays inside the
/// layer's own footprint (an overlay does not spill over what is behind it).
fn zoom_into_crop(
    clip: &ClipInstance,
    base: CropRect,
    duration: f64,
    animation: &mut Animation,
    approximations: &mut BTreeSet<Approximation>,
) {
    let samples = ((duration * ZOOM_SAMPLES_PER_SEC).ceil() as usize).clamp(2, MAX_ZOOM_SAMPLES);
    let (mut xs, mut ys, mut widths, mut heights) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for step in 0..=samples {
        let fraction = step as f32 / samples as f32;
        let mut zoom = evaluate_keyframes(&clip.scale_keyframes, fraction, 1.0);
        if !zoom.is_finite() || zoom < 1.0 {
            approximations.insert(Approximation::ZoomOutHeld);
            zoom = 1.0;
        }
        let (width, height) = (base.width / zoom, base.height / zoom);
        let time_secs = f64::from(fraction) * duration;
        let key = |value: f32| Keyframe {
            time_secs,
            value,
            easing: Easing::Linear,
        };
        xs.push(key(base.x + (base.width - width) / 2.0));
        ys.push(key(base.y + (base.height - height) / 2.0));
        widths.push(key(width));
        heights.push(key(height));
    }
    animation.crop_x = KeyframeTrack::new(xs).ok();
    animation.crop_y = KeyframeTrack::new(ys).ok();
    animation.crop_width = KeyframeTrack::new(widths).ok();
    animation.crop_height = KeyframeTrack::new(heights).ok();
}

/// Incoming transition: fade becomes the placement fade, slide and zoom become short animations
/// unless keyframes already own that axis.
fn apply_transition(
    clip: &ClipInstance,
    duration: f64,
    canvas_width: f32,
    placement: &mut Placement,
    animation: &mut Animation,
    approximations: &mut BTreeSet<Approximation>,
) {
    let length = f64::from(clip.transition_duration_secs).min(duration);
    if clip.transition_in == TransitionType::None
        || clip.transition_in == TransitionType::HardCut
        || !length.is_finite()
        || length <= 0.0
    {
        return;
    }
    match clip.transition_in {
        TransitionType::Fade => {
            placement.fade = Fade {
                in_secs: length,
                out_secs: 0.0,
            };
        }
        TransitionType::Slide => {
            match Animation::slide(-canvas_width, 0.0, length, Easing::EaseOut) {
                Some(slide) if animation.x.is_none() => animation.x = slide.x,
                _ => {
                    approximations.insert(Approximation::TransitionIgnored);
                }
            }
        }
        TransitionType::Zoom => {
            let layer_scale_is_unity = (placement.transform.scale_x - 1.0).abs() < 1e-6
                && (placement.transform.scale_y - 1.0).abs() < 1e-6;
            match Animation::zoom(TRANSITION_ZOOM_START, 1.0, length, Easing::EaseOut) {
                Some(zoom) if layer_scale_is_unity => {
                    animation.scale_x = zoom.scale_x;
                    animation.scale_y = zoom.scale_y;
                }
                _ => {
                    approximations.insert(Approximation::TransitionIgnored);
                }
            }
        }
        TransitionType::None | TransitionType::HardCut => {}
    }
}

fn pixelize_block(intensity: f32) -> u32 {
    let intensity = intensity.clamp(0.0, 1.0);
    if intensity <= 0.0 {
        0
    } else {
        (PIXELIZE_MIN_BLOCK + intensity * PIXELIZE_BLOCK_RANGE).round() as u32
    }
}

/// Both enums list the FFmpeg `blend` filter's modes, so the FFmpeg name is the shared key.
fn blend_of(mode: CoreBlend) -> BlendMode {
    BlendMode::ALL
        .iter()
        .copied()
        .find(|candidate| candidate.ffmpeg_name() == mode.ffmpeg_name())
        .unwrap_or(BlendMode::Normal)
}

#[cfg(test)]
#[path = "mapper_test.rs"]
mod tests;
