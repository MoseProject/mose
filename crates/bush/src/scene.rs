//! Immutable scene input for Bush's real-time renderer.
//!
//! Mose is responsible for resolving mutable project/timeline state into this compact, validated
//! representation. Bush sorts it once per scene revision and then reads it without reaching back
//! into UI or project state.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::{BlendMode, LayerEffects, LayerTransform, SceneVersions, Version};

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
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub timeline_start_secs: f64,
    pub duration_secs: Option<f64>,
    /// Source seconds consumed per timeline second; `1.0` is normal speed.
    pub speed: f64,
    pub x: i32,
    pub y: i32,
    pub opacity: f32,
    pub transform: LayerTransform,
    pub blend: BlendMode,
    pub effects: LayerEffects,
}

impl Default for Placement {
    fn default() -> Self {
        Self {
            timeline_start_secs: 0.0,
            duration_secs: None,
            speed: 1.0,
            x: 0,
            y: 0,
            opacity: 1.0,
            transform: LayerTransform::default(),
            blend: BlendMode::Normal,
            effects: LayerEffects::default(),
        }
    }
}

impl Placement {
    fn validate(&self) -> Result<(), SceneError> {
        let finite = self.timeline_start_secs.is_finite() && self.speed.is_finite();
        let window_ok = self
            .duration_secs
            .is_none_or(|duration| duration.is_finite() && duration > 0.0);
        if !finite
            || self.timeline_start_secs < 0.0
            || self.speed <= 0.0
            || !window_ok
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
            || !self.transform.is_valid()
            || !self.effects.is_valid()
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
                Some((source.range.out_secs - source.range.in_secs) / self.placement.speed)
            }
            ContributorContent::RasterOverlay(_) => self.placement.duration_secs,
        }
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

    /// Source time presented at `playhead_secs` for a media contributor active then.
    pub fn source_time_at(&self, playhead_secs: f64) -> Option<f64> {
        let ContributorContent::Media(source) = &self.content else {
            return None;
        };
        if !self.is_active_at(playhead_secs) {
            return None;
        }
        let local = (playhead_secs - self.placement.timeline_start_secs) * self.placement.speed;
        Some((source.range.in_secs + local).min(source.range.out_secs))
    }
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
