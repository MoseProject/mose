//! Immutable scene input for Bush's real-time renderer.
//!
//! Mose is responsible for resolving mutable project/timeline state into this compact, validated
//! representation. Bush sorts it once per scene revision and then reads it without reaching back
//! into UI or project state.

use std::collections::HashSet;
use std::path::PathBuf;

use std::sync::Arc;

use crate::{
    Animation, BlendMode, Fade, LayerEffects, LayerTransform, Lut3D, SceneVersions, Version,
};

/// Stable identity of one contributor inside a scene.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ContributorId(pub u64);

/// The source-time range of a media contributor in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceRange {
    pub in_secs: f64,
    pub out_secs: f64,
}

impl SourceRange {
    /// Validates a finite, non-empty source range.
    pub fn new(in_secs: f64, out_secs: f64) -> Result<Self, SceneError> {
        if !in_secs.is_finite() || !out_secs.is_finite() {
            return Err(SceneError::NonFiniteSourceRange);
        }
        if in_secs < 0.0 || out_secs <= in_secs {
            return Err(SceneError::InvalidSourceRange { in_secs, out_secs });
        }

        Ok(Self { in_secs, out_secs })
    }
}

/// A resolved local media source prepared by Mose.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaSource {
    pub path: PathBuf,
    pub range: SourceRange,
    /// Changes when Mose selects a different proxy/original source or decode-affecting format.
    pub version: Version,
}

/// A precomputed matte video (grayscale luma, white is inside) covering a clip's source range
/// from its first second; see [`Placement::alpha_matte`] and [`Placement::privacy_blur`].
#[derive(Clone, Debug, PartialEq)]
pub struct MatteSource {
    pub path: PathBuf,
    pub version: Version,
}

/// Blur of the composited canvas wherever a matte is set.
#[derive(Clone, Debug, PartialEq)]
pub struct PrivacyBlur {
    pub matte: MatteSource,
    /// Gaussian standard deviation in canvas pixels, `0 < sigma <= MAX_BLUR_SIGMA`.
    pub sigma: f32,
}

/// A pre-rasterized text, shape, or other overlay prepared outside Bush.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RasterOverlay {
    pub width: u32,
    pub height: u32,
    pub version: Version,
}

/// The renderable content associated with a scene contributor.
#[derive(Clone, Debug, PartialEq)]
pub enum ContributorContent {
    Media(MediaSource),
    RasterOverlay(RasterOverlay),
}

/// Where and when a contributor appears, and how it is composited.
///
/// For media, the timeline window is derived from the source range and `speed`
/// (`(out - in) / speed` seconds); `duration_secs` is the explicit window for raster overlays,
/// where `None` means visible for the whole scene.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub timeline_start_secs: f64,
    pub duration_secs: Option<f64>,
    /// Source seconds consumed per timeline second; `1.0` is normal speed.
    pub speed: f64,
    /// When set, speed ramps linearly from `speed` at the clip start to this value at its end
    /// (the timeline duration shortens or lengthens accordingly).
    pub speed_ramp_end: Option<f64>,
    pub x: i32,
    pub y: i32,
    pub opacity: f32,
    pub transform: LayerTransform,
    pub blend: BlendMode,
    pub effects: LayerEffects,
    /// Linear audio gain for audible contributors, `0.0..=4.0`.
    pub gain: f32,
    /// Keyframed overrides of opacity, position, scale and rotation, relative to the
    /// contributor's timeline start.
    pub animation: Animation,
    pub fade: Fade,
    /// Shared 3D colour lookup; cloning a placement shares the table.
    pub lut: Option<Arc<Lut3D>>,
    /// Temporal brightness smoothing weight (`0.0 < w <= 1.0`, lower is steadier); `None` off.
    pub deflicker: Option<f32>,
    /// Gaussian blur standard deviation in source pixels, 0.0..=MAX_BLUR_SIGMA (0 is off).
    pub blur_sigma: f32,
    /// Unsharp-mask amount, 0.0..=4.0 (0 is off).
    pub sharpen: f32,
    /// Glitch strength, 0.0..=1.0 (0 is off): channel split plus displaced bands.
    pub glitch: f32,
    /// Shake amplitude in canvas pixels, 0.0..=200.0 (0 is off): smooth positional jitter.
    pub shake: f32,
    /// Stabilization smoothing weight (0 < w <= 1, lower is steadier); `None` is off. The layer is
    /// counter-translated onto the smoothed camera path and zoomed by `STABILIZE_ZOOM` to hide
    /// the moving borders.
    pub stabilize: Option<f32>,
    /// Stretch the cropped layer to fill the canvas (times `transform.scale_*`), centred, with
    /// `x`/`y` as an offset from the centred position. Off keeps the decoded size and `x`/`y` as
    /// the top-left corner. The decoded size can be a proxy's, so only the engine knows it.
    pub fit_canvas: bool,
    /// Multiplies the layer's alpha by this matte (matte time is the clip-local source time).
    pub alpha_matte: Option<MatteSource>,
    /// Blurs the composited canvas through this matte, after every layer is composed (matte
    /// time is the clip-local timeline time stretched over the matte's length).
    pub privacy_blur: Option<PrivacyBlur>,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            timeline_start_secs: 0.0,
            duration_secs: None,
            speed: 1.0,
            speed_ramp_end: None,
            x: 0,
            y: 0,
            opacity: 1.0,
            transform: LayerTransform::default(),
            blend: BlendMode::Normal,
            effects: LayerEffects::default(),
            gain: 1.0,
            animation: Animation::default(),
            fade: Fade::default(),
            lut: None,
            deflicker: None,
            blur_sigma: 0.0,
            sharpen: 0.0,
            glitch: 0.0,
            shake: 0.0,
            stabilize: None,
            fit_canvas: false,
            alpha_matte: None,
            privacy_blur: None,
        }
    }
}

impl Placement {
    /// Top-left corner and transform to composite a `frame_width` x `frame_height` source with:
    /// unchanged unless `fit_canvas`, otherwise the cropped region stretched to the canvas.
    pub(crate) fn layout_on(
        &self,
        canvas_width: u32,
        canvas_height: u32,
        frame_width: u32,
        frame_height: u32,
    ) -> (i32, i32, LayerTransform) {
        if !self.fit_canvas {
            return (self.x, self.y, self.transform);
        }
        let crop = self.transform.crop;
        let cropped_width = (frame_width as f32 * crop.width).max(1.0);
        let cropped_height = (frame_height as f32 * crop.height).max(1.0);
        let mut transform = self.transform;
        transform.scale_x = self.transform.scale_x * canvas_width as f32 / cropped_width;
        transform.scale_y = self.transform.scale_y * canvas_height as f32 / cropped_height;
        let drawn_width = self.transform.scale_x * canvas_width as f32;
        let drawn_height = self.transform.scale_y * canvas_height as f32;
        let x = ((canvas_width as f32 - drawn_width) / 2.0).round() as i32 + self.x;
        let y = ((canvas_height as f32 - drawn_height) / 2.0).round() as i32 + self.y;
        (x, y, transform)
    }

    fn validate(&self) -> Result<(), SceneError> {
        let finite = self.timeline_start_secs.is_finite() && self.speed.is_finite();
        let window_ok = self
            .duration_secs
            .is_none_or(|duration| duration.is_finite() && duration > 0.0);
        if !finite
            || self.timeline_start_secs < 0.0
            || self.speed <= 0.0
            || self
                .speed_ramp_end
                .is_some_and(|end| !end.is_finite() || end <= 0.0)
            || !window_ok
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !self.transform.is_valid()
            || !self.effects.is_valid()
            || !(0.0..=4.0).contains(&self.gain)
            || !self.animation.is_valid()
            || !self.fade.is_valid()
            || !(0.0..=crate::MAX_BLUR_SIGMA).contains(&self.blur_sigma)
            || !(0.0..=4.0).contains(&self.sharpen)
            || !(0.0..=1.0).contains(&self.glitch)
            || !(0.0..=200.0).contains(&self.shake)
            || self
                .stabilize
                .is_some_and(|weight| !(weight > 0.0 && weight <= 1.0))
            || self
                .deflicker
                .is_some_and(|weight| !(weight > 0.0 && weight <= 1.0))
            || self
                .alpha_matte
                .as_ref()
                .is_some_and(|matte| matte.path.as_os_str().is_empty())
            || self.privacy_blur.as_ref().is_some_and(|blur| {
                blur.matte.path.as_os_str().is_empty()
                    || !(blur.sigma > 0.0 && blur.sigma <= crate::MAX_BLUR_SIGMA)
            })
        {
            return Err(SceneError::InvalidPlacement);
        }
        Ok(())
    }
}

/// One video, audio, or overlay contributor at a scene layer.
#[derive(Clone, Debug, PartialEq)]
pub struct Contributor {
    pub id: ContributorId,
    /// Larger values paint later, above lower values.
    pub layer: i32,
    pub visible: bool,
    pub audible: bool,
    pub content: ContributorContent,
    pub placement: Placement,
}

/// A validated, layer-sorted snapshot submitted to Bush.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSnapshot {
    pub versions: SceneVersions,
    contributors: Vec<Contributor>,
}

impl SceneSnapshot {
    /// Validates contributors and sorts them once into deterministic painter order.
    pub fn new(
        versions: SceneVersions,
        mut contributors: Vec<Contributor>,
    ) -> Result<Self, SceneError> {
        let mut ids = HashSet::with_capacity(contributors.len());
        for contributor in &contributors {
            if !ids.insert(contributor.id) {
                return Err(SceneError::DuplicateContributor(contributor.id));
            }
            validate_content(&contributor.content)?;
            contributor.placement.validate()?;
        }

        contributors.sort_unstable_by_key(|contributor| (contributor.layer, contributor.id));
        Ok(Self {
            versions,
            contributors,
        })
    }

    /// Contributors in deterministic painter order.
    pub fn contributors(&self) -> &[Contributor] {
        &self.contributors
    }

    /// Visible contributors in deterministic painter order.
    pub fn visible_contributors(&self) -> impl Iterator<Item = &Contributor> {
        self.contributors
            .iter()
            .filter(|contributor| contributor.visible)
    }

    /// Audible contributors in deterministic layer order.
    pub fn audible_contributors(&self) -> impl Iterator<Item = &Contributor> {
        self.contributors
            .iter()
            .filter(|contributor| contributor.audible)
    }
}

impl Contributor {
    /// Timeline length of this contributor; `None` for an overlay visible for the whole scene.
    pub fn timeline_duration_secs(&self) -> Option<f64> {
        match &self.content {
            ContributorContent::Media(source) => {
                let length = source.range.out_secs - source.range.in_secs;
                Some(match self.placement.speed_ramp_end {
                    Some(end) => ramp_duration_secs(length, self.placement.speed, end),
                    None => length / self.placement.speed,
                })
            }
            ContributorContent::RasterOverlay(_) => self.placement.duration_secs,
        }
    }

    /// The placement with keyframes and fades evaluated at `playhead_secs`. Static values are
    /// used for any property without a track; the result is always a valid, unanimated layer
    /// description for the compositor.
    pub fn placement_at(&self, playhead_secs: f64) -> Placement {
        let local = playhead_secs - self.placement.timeline_start_secs;
        let mut resolved = self.placement.clone();
        let animation = &self.placement.animation;
        if let Some(track) = &animation.opacity {
            resolved.opacity = track.value_at(local);
        }
        if let Some(track) = &animation.x {
            resolved.x = track.value_at(local).round() as i32;
        }
        if let Some(track) = &animation.y {
            resolved.y = track.value_at(local).round() as i32;
        }
        if let Some(track) = &animation.scale_x {
            resolved.transform.scale_x = track.value_at(local);
        }
        if let Some(track) = &animation.scale_y {
            resolved.transform.scale_y = track.value_at(local);
        }
        if let Some(track) = &animation.rotation_deg {
            resolved.transform.rotation_deg = track.value_at(local);
        }
        if let Some(track) = &animation.brightness {
            resolved.effects.brightness = track.value_at(local);
        }
        if let Some(track) = &animation.contrast {
            resolved.effects.contrast = track.value_at(local);
        }
        if let Some(track) = &animation.saturation {
            resolved.effects.saturation = track.value_at(local);
        }
        {
            // An animated crop window must stay a valid sub-rectangle at every instant.
            let crop = &mut resolved.transform.crop;
            if let Some(track) = &animation.crop_width {
                crop.width = track.value_at(local).clamp(0.01, 1.0);
            }
            if let Some(track) = &animation.crop_height {
                crop.height = track.value_at(local).clamp(0.01, 1.0);
            }
            if let Some(track) = &animation.crop_x {
                crop.x = track.value_at(local);
            }
            if let Some(track) = &animation.crop_y {
                crop.y = track.value_at(local);
            }
            crop.x = crop.x.clamp(0.0, 1.0 - crop.width);
            crop.y = crop.y.clamp(0.0, 1.0 - crop.height);
        }
        if resolved.shake > 0.0 {
            resolved.x += crate::shake_offset(resolved.shake, playhead_secs, 1).round() as i32;
            resolved.y += crate::shake_offset(resolved.shake, playhead_secs, 2).round() as i32;
        }
        resolved.opacity *= self
            .placement
            .fade
            .factor_at(local, self.timeline_duration_secs());
        resolved
    }

    /// Whether the contributor is on the timeline at `playhead_secs` (end exclusive).
    pub fn is_active_at(&self, playhead_secs: f64) -> bool {
        let start = self.placement.timeline_start_secs;
        if playhead_secs < start {
            return false;
        }
        match self.timeline_duration_secs() {
            Some(duration) => playhead_secs < start + duration,
            None => true,
        }
    }

    /// Source seconds consumed `local_secs` after the contributor starts (speed ramp included).
    pub fn source_offset_at(&self, local_secs: f64) -> f64 {
        let start_speed = self.placement.speed;
        match (self.placement.speed_ramp_end, self.source_length_secs()) {
            (Some(end), Some(length)) => ramp_source_offset(local_secs, length, start_speed, end),
            _ => start_speed * local_secs,
        }
    }

    /// Clip-local source seconds at `playhead_secs`: the time inside an alpha matte video.
    pub fn alpha_matte_time_at(&self, playhead_secs: f64) -> Option<f64> {
        self.placement.alpha_matte.as_ref()?;
        let ContributorContent::Media(source) = &self.content else {
            return None;
        };
        self.source_time_at(playhead_secs)
            .map(|time| time - source.range.in_secs)
    }

    /// Time inside a privacy-blur matte video: the matte spans the clip's whole timeline window
    /// evenly, whatever its speed.
    pub fn privacy_matte_time_at(&self, playhead_secs: f64) -> Option<f64> {
        self.placement.privacy_blur.as_ref()?;
        if !self.is_active_at(playhead_secs) {
            return None;
        }
        let length = self.source_length_secs()?;
        let duration = self.timeline_duration_secs()?;
        let local = playhead_secs - self.placement.timeline_start_secs;
        Some((local / duration).clamp(0.0, 1.0) * length)
    }

    /// Trimmed source length of a media contributor.
    fn source_length_secs(&self) -> Option<f64> {
        match &self.content {
            ContributorContent::Media(source) => Some(source.range.out_secs - source.range.in_secs),
            ContributorContent::RasterOverlay(_) => None,
        }
    }

    /// Instantaneous playback speed `local_secs` after the contributor starts.
    pub fn speed_at(&self, local_secs: f64) -> f64 {
        match (self.placement.speed_ramp_end, self.source_length_secs()) {
            (Some(end), Some(length)) => {
                ramp_speed_at(local_secs, length, self.placement.speed, end)
            }
            _ => self.placement.speed,
        }
    }

    /// Source time presented at `playhead_secs` for a media contributor active then.
    pub fn source_time_at(&self, playhead_secs: f64) -> Option<f64> {
        let ContributorContent::Media(source) = &self.content else {
            return None;
        };
        if !self.is_active_at(playhead_secs) {
            return None;
        }
        let local = self.source_offset_at(playhead_secs - self.placement.timeline_start_secs);
        Some((source.range.in_secs + local).min(source.range.out_secs))
    }
}

/// Speed ramps are linear in *source* time (speed `v0` at the first source second, `v1` at the
/// last), the same definition the exporter uses, so a clip's timeline duration is
/// `length / (v1 - v0) * ln(v1 / v0)` and its speed grows exponentially in timeline time.
const RAMP_EQUAL_SPEEDS: f64 = 1e-9;

fn ramp_duration_secs(length: f64, v0: f64, v1: f64) -> f64 {
    if (v1 - v0).abs() < RAMP_EQUAL_SPEEDS {
        length / v0
    } else {
        length / (v1 - v0) * (v1 / v0).ln()
    }
}

/// Instantaneous speed `local` timeline seconds into a ramp over `length` source seconds.
fn ramp_speed_at(local: f64, length: f64, v0: f64, v1: f64) -> f64 {
    if (v1 - v0).abs() < RAMP_EQUAL_SPEEDS {
        return v0;
    }
    let local = local.clamp(0.0, ramp_duration_secs(length, v0, v1));
    v0 * ((v1 - v0) * local / length).exp()
}

/// Source seconds consumed `local` timeline seconds into the ramp.
fn ramp_source_offset(local: f64, length: f64, v0: f64, v1: f64) -> f64 {
    if (v1 - v0).abs() < RAMP_EQUAL_SPEEDS {
        return v0 * local;
    }
    let local = local.clamp(0.0, ramp_duration_secs(length, v0, v1));
    length * v0 * (((v1 - v0) * local / length).exp() - 1.0) / (v1 - v0)
}

fn validate_content(content: &ContributorContent) -> Result<(), SceneError> {
    match content {
        ContributorContent::Media(source) => {
            if source.path.as_os_str().is_empty() {
                return Err(SceneError::EmptyMediaPath);
            }
            SourceRange::new(source.range.in_secs, source.range.out_secs).map(|_| ())
        }
        ContributorContent::RasterOverlay(overlay) => {
            if overlay.width == 0 || overlay.height == 0 {
                return Err(SceneError::EmptyRasterOverlay);
            }
            Ok(())
        }
    }
}

/// Invalid data rejected before it can reach decoders, renderers, or native boundaries.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneError {
    NonFiniteSourceRange,
    InvalidSourceRange { in_secs: f64, out_secs: f64 },
    EmptyMediaPath,
    EmptyRasterOverlay,
    DuplicateContributor(ContributorId),
    InvalidPlacement,
}
