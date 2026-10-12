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
//! vignette) are derived from the FFmpeg filter parameters the exporter uses (see the constants
//! below). Glitch (noise in the export, channel split and bands here) and stabilization (deshake
//! search radius in the export, smoothing weight here) are different algorithms with matching
//! intent, so they only share the 0..1 strength.

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::Arc;

use avcore::keyframe::{evaluate_keyframes, Keyframe as CoreKeyframe, Position};
use avcore::overlay_render::{render_shape_clip_rgba, render_text_segment_rgba};
use avcore::render::resolve_timeline_text_segments;
use avcore::timeline::{
    BlendMode as CoreBlend, ClipInstance, ColorFilter, MaskShape as CoreMask, Timeline, TrackKind,
    TransitionType,
};
use avcore::{MediaAsset, MediaKind};
use bush::{
    Animation, BlendMode, ChromaKey, Contributor, ContributorContent, ContributorId, CropRect,
    Easing, Fade, Keyframe, KeyframeTrack, LayerEffects, LayerMask, LayerTransform, Lut3D,
    MaskShape, MatteSource, MediaSource, PixelFormat, Placement, PrivacyBlur, RasterOverlay,
    SceneError, SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame, MAX_BLUR_SIGMA,
    MAX_GAIN_DB, MIN_GAIN_DB,
};

/// Source span a frozen clip advances over its whole window; the decoder keeps serving the same
/// frame while the (near zero) speed walks through it.
const FROZEN_SOURCE_SPAN_SECS: f64 = 1e-3;
/// Export blur is `avgblur=sizeX=sizeY=<10 * intensity>` (radius truncated to whole pixels).
const BLUR_RADIUS_AT_FULL: f32 = 10.0;
/// Export sharpen is `unsharp=5:5:<3 * sharpen>`; the 5x5 box matches `bush::SHARPEN_SIGMA`.
const SHARPEN_AMOUNT_AT_FULL: f32 = 3.0;
/// Export shake crops a margin of `0.08 * intensity` of each dimension and swings the crop
/// window by it, so the offset at full intensity is 8% of the canvas (averaged over both axes).
const SHAKE_FRACTION_AT_FULL: f32 = 0.08;
const MAX_SHAKE_PIXELS: f32 = 200.0;
/// Export chroma key is `colorkey=<tolerance>:0.1`.
const CHROMA_SOFTNESS: f32 = 0.1;
/// Export deflicker is a 5-frame mean, the equivalent exponential weight is `2 / (5 + 1)`.
const DEFLICKER_WEIGHT: f32 = 1.0 / 3.0;
/// Export pixelize block is `2 + 48 * intensity` pixels.
const PIXELIZE_MIN_BLOCK: f32 = 2.0;
const PIXELIZE_BLOCK_RANGE: f32 = 48.0;
/// Export vignette is `vignette=PI/4*<intensity>`; measured against FFmpeg on a flat grey frame,
/// the corner stays at 13% of the centre at intensity 1 and 68% at 0.5, which `0.87 * i^1.45`
/// of Bush's `1 - v * r^2` falloff reproduces at the corner.
const VIGNETTE_AT_FULL: f32 = 0.87;
const VIGNETTE_EXPONENT: f32 = 1.45;
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

/// A property whose Bush rendering differs from the export.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Approximation {
    /// A frozen clip is silent in Bush.
    FrozenAudioMuted,
    /// Ken-Burns zoom is a crop, so zooming out below 1x is held at 1x.
    ZoomOutHeld,
    /// Scale keyframes are ignored when crop keyframes already drive the crop window.
    ScaleWithCropKeyframes,
    /// A slide or zoom transition is skipped because position or scale keyframes own that axis.
    TransitionIgnored,
    /// Background removal or privacy blur is on but its matte file is missing, so the clip
    /// renders unmasked (the exporter degrades the same way).
    MatteMissing,
}

/// Rasters Bush composites for the scene's overlay contributors, by contributor id.
pub type Overlays = Vec<(ContributorId, Arc<VideoFrame>)>;

pub struct Mapped {
    pub scene: SceneSnapshot,
    pub overlays: Overlays,
    pub approximations: BTreeSet<Approximation>,
}

/// Overlay ids live above any clip id so they cannot collide with media contributors.
const TEXT_OVERLAY_ID_BASE: u64 = 1 << 40;
const SHAPE_OVERLAY_ID_BASE: u64 = 1 << 41;
/// Overlays always paint above video; text above shapes.
const SHAPE_LAYER_BASE: i32 = 1_000;
const TEXT_LAYER_BASE: i32 = 2_000;

/// Full-canvas overlay rasters keyed by a hash of what they depict, so an edit that does not touch
/// a caption or shape does not re-rasterize it.
#[derive(Default)]
pub struct OverlayCache {
    frames: HashMap<u64, Arc<VideoFrame>>,
}

impl OverlayCache {
    fn raster(
        &mut self,
        key: u64,
        canvas: (u32, u32),
        used: &mut HashSet<u64>,
        render: impl FnOnce() -> Vec<u8>,
    ) -> Option<Arc<VideoFrame>> {
        used.insert(key);
        if let Some(frame) = self.frames.get(&key) {
            return Some(Arc::clone(frame));
        }
        let frame = Arc::new(
            VideoFrame::new(
                PixelFormat::Rgba8,
                canvas.0,
                canvas.1,
                canvas.0 as usize * 4,
                0.0,
                render(),
            )
            .ok()?,
        );
        self.frames.insert(key, Arc::clone(&frame));
        Some(frame)
    }
}

fn content_key(description: &str, canvas: (u32, u32)) -> u64 {
    let mut hasher = DefaultHasher::new();
    description.hash(&mut hasher);
    canvas.hash(&mut hasher);
    hasher.finish()
}

/// Text segments (one per caption, plus one per highlighted word) and shape clips as timed
/// raster contributors.
fn map_overlays(
    timeline: &Timeline,
    canvas: (u32, u32),
    cache: &mut OverlayCache,
) -> (Vec<Contributor>, Overlays) {
    let mut contributors = Vec::new();
    let mut overlays = Overlays::new();
    let mut used = HashSet::new();
    let mut push =
        |id: u64, layer: i32, start: f64, duration: f64, key: u64, frame: Arc<VideoFrame>| {
            let id = ContributorId(id);
            contributors.push(Contributor {
                id,
                layer,
                visible: true,
                audible: false,
                content: ContributorContent::RasterOverlay(RasterOverlay {
                    width: canvas.0,
                    height: canvas.1,
                    version: Version(key),
                }),
                placement: Placement {
                    timeline_start_secs: start,
                    duration_secs: Some(duration),
                    ..Placement::default()
                },
            });
            overlays.push((id, frame));
        };

    for (index, track) in timeline
        .tracks
        .iter()
        .enumerate()
        .filter(|(_, track)| track.visible && track.kind == TrackKind::Shape)
    {
        for clip in &track.shape_clips {
            if !clip.duration_secs.is_finite() || clip.duration_secs <= 0.0 {
                continue;
            }
            let key = content_key(&format!("{clip:?}"), canvas);
            let Some(frame) = cache.raster(key, canvas, &mut used, || {
                render_shape_clip_rgba(clip, canvas.0, canvas.1)
            }) else {
                continue;
            };
            push(
                SHAPE_OVERLAY_ID_BASE + clip.id,
                SHAPE_LAYER_BASE + i32::try_from(index).unwrap_or(0),
                clip.start_secs,
                clip.duration_secs,
                key,
                frame,
            );
        }
    }
    for (number, segment) in resolve_timeline_text_segments(timeline, canvas.0, canvas.1)
        .into_iter()
        .enumerate()
    {
        if !segment.duration_secs.is_finite() || segment.duration_secs <= 0.0 {
            continue;
        }
        let key = content_key(&format!("{segment:?}"), canvas);
        let Some(frame) = cache.raster(key, canvas, &mut used, || {
            render_text_segment_rgba(&segment, canvas.0, canvas.1)
        }) else {
            continue;
        };
        push(
            TEXT_OVERLAY_ID_BASE + number as u64,
            TEXT_LAYER_BASE + i32::try_from(number).unwrap_or(0),
            segment.start_secs,
            segment.duration_secs,
            key,
            frame,
        );
    }
    cache.frames.retain(|key, _| used.contains(key));
    (contributors, overlays)
}

struct MappedClip {
    range: SourceRange,
    placement: Placement,
}

/// Builds the scene for the whole timeline. `canvas` is the preview canvas in pixels.
/// Scene layers reserved per track, so the lanes of one track never reach the next track's layers.
const LAYERS_PER_TRACK: usize = 1000;

pub fn map_timeline(
    timeline: &Timeline,
    media: &[MediaAsset],
    canvas: (u32, u32),
    resolver: &mut dyn SourceResolver,
    overlay_cache: &mut OverlayCache,
    versions: SceneVersions,
) -> Result<Mapped, SceneError> {
    let mut approximations = BTreeSet::new();
    let mut contributors = Vec::new();
    // Like the exporter, an alpha matte only takes effect above the first video track: the
    // background track never composites, so its alpha is discarded.
    let first_video_track = timeline
        .tracks
        .iter()
        .position(|track| track.kind == TrackKind::Video);
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
        let lanes = track.overlap_lanes();
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
            let overlay = first_video_track.is_some_and(|first| index != first);
            let Some(mut mapped) = map_clip(clip, canvas, overlay, resolver, &mut approximations)
            else {
                continue;
            };
            // The track's fader and pan, on top of the clip's own gain.
            mapped.placement.track_gain = track.mix.linear_gain();
            mapped.placement.pan = track.mix.pan;
            contributors.push(Contributor {
                id: ContributorId(clip.id),
                // A clip overlapping an earlier one (a cross-dissolve) sits one layer above the
                // clip it dissolves from; layers of different tracks stay far apart.
                layer: i32::try_from(
                    index * LAYERS_PER_TRACK + lanes.get(&clip.id).copied().unwrap_or(0),
                )
                .unwrap_or(i32::MAX),
                visible: on_video_track && asset.kind == MediaKind::Video,
                audible: asset.has_audio && !clip.frozen && timeline.track_is_audible(track),
                content: ContributorContent::Media(MediaSource {
                    path,
                    range: mapped.range,
                    version: Version(asset.id),
                }),
                placement: mapped.placement,
            });
        }
    }
    let (overlay_contributors, overlays) = map_overlays(timeline, canvas, overlay_cache);
    contributors.extend(overlay_contributors);
    Ok(Mapped {
        scene: SceneSnapshot::new(versions, contributors)?,
        overlays,
        approximations,
    })
}

fn map_clip(
    clip: &ClipInstance,
    canvas: (u32, u32),
    overlay: bool,
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
        (range, f64::from(clip.speed_factor.max(0.01)))
    };

    let (canvas_width, canvas_height) = (canvas.0 as f32, canvas.1 as f32);
    let mut animation = Animation::default();
    let mut placement = Placement {
        timeline_start_secs: clip.start_secs,
        speed,
        // Both sides ramp linearly in source time, so the window matches `duration_secs()`.
        speed_ramp_end: clip
            .speed_ramp_end_factor
            .filter(|_| !clip.frozen)
            .map(|end| f64::from(end.max(0.01))),
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
        vignette: VIGNETTE_AT_FULL
            * clip
                .vignette_intensity
                .clamp(0.0, 1.0)
                .powf(VIGNETTE_EXPONENT),
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
    if clip.background_removal_enabled && overlay {
        placement.alpha_matte = matte_source(&clip.background_removal_mask_path);
        if placement.alpha_matte.is_none() {
            approximations.insert(Approximation::MatteMissing);
        }
    }
    if clip.privacy_blur_enabled {
        placement.privacy_blur =
            matte_source(&clip.privacy_blur_mask_path).map(|matte| PrivacyBlur {
                matte,
                sigma: clip.privacy_blur_sigma.clamp(0.1, MAX_BLUR_SIGMA),
            });
        if placement.privacy_blur.is_none() {
            approximations.insert(Approximation::MatteMissing);
        }
    }
    placement.effects = effects;
    placement.blend = blend_of(clip.blend_mode);
    if !clip.lut_path.is_empty() {
        placement.lut = resolver.lut(&clip.lut_path);
    }

    // Filters that live on the placement itself.
    placement.blur_sigma = box_blur_sigma(clip.blur_intensity).min(MAX_BLUR_SIGMA);
    placement.sharpen = clip.sharpen.clamp(0.0, 1.0) * SHARPEN_AMOUNT_AT_FULL;
    placement.glitch = clip.glitch_intensity.clamp(0.0, 1.0);
    placement.shake = (clip.shake_intensity.clamp(0.0, 1.0)
        * SHAKE_FRACTION_AT_FULL
        * (canvas_width + canvas_height)
        / 2.0)
        .min(MAX_SHAKE_PIXELS);
    if clip.deflicker_enabled {
        placement.deflicker = Some(DEFLICKER_WEIGHT);
    }
    if clip.stabilization_intensity > 0.0 {
        placement.stabilize = Some((1.0 - 0.9 * clip.stabilization_intensity).clamp(0.05, 1.0));
    }

    // Audio gain.
    // Keyframes win over the static gain, like the exporter's gain expression.
    animation.gain_db = track_of(&clip.gain_keyframes, duration, |v: f32| {
        v.clamp(MIN_GAIN_DB, MAX_GAIN_DB)
    });
    placement.gain = clip.gain_linear().clamp(0.0, 4.0);

    apply_transition(
        clip,
        duration,
        canvas_width,
        &mut placement,
        &mut animation,
        approximations,
    );
    apply_fade_handles(clip, duration, &mut placement);
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

/// The clip's fade-in / fade-out handles. They share the placement fade with the Fade
/// transition, so the longer fade-in wins and the fade-out is added.
fn apply_fade_handles(clip: &ClipInstance, duration: f64, placement: &mut Placement) {
    let clamp = |secs: f32| f64::from(secs).clamp(0.0, duration);
    placement.fade.in_secs = placement.fade.in_secs.max(clamp(clip.fade_in_secs));
    placement.fade.out_secs = clamp(clip.fade_out_secs);
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
        TransitionType::Fade | TransitionType::Dissolve => {
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

/// The matte video at `path`, versioned by its size and modification time so regenerating a
/// matte in place invalidates frames cached from the old one; `None` when unset or missing.
fn matte_source(path: &str) -> Option<MatteSource> {
    if path.is_empty() {
        return None;
    }
    let metadata = std::fs::metadata(path).ok().filter(|meta| meta.is_file())?;
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|age| age.as_nanos())
        .hash(&mut hasher);
    Some(MatteSource {
        path: PathBuf::from(path),
        version: Version(hasher.finish()),
    })
}

/// Gaussian sigma with the variance of the exporter's box blur: a box of radius `R` (whole
/// pixels, `R = trunc(10 * intensity)`) spans `2R + 1` pixels, variance `R * (R + 1) / 3`.
fn box_blur_sigma(intensity: f32) -> f32 {
    let radius = (intensity.clamp(0.0, 1.0) * BLUR_RADIUS_AT_FULL).trunc();
    (radius * (radius + 1.0) / 3.0).sqrt()
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
