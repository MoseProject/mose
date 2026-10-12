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

//! CF-05: OpenTimelineIO interchange boundary
//! ([`spec/architecture/competitive-feature-plan.md`](../../../../spec/architecture/competitive-feature-plan.md)'s
//! own "Outcome: let Oca specialize in gameplay automation while Premiere, Resolve, or other
//! tools can perform final finishing").
//!
//! This module is CF-05's own slice 1 ("Create an `avcore::interchange` boundary independent of
//! UI and render code") plus the structural half of slice 2 (mapping [`crate::timeline::Timeline`]
//! into rational-time editorial concepts: track order, clip source ranges, gaps, markers,
//! transitions, speed, and media references). The types below are Oca's own schema-neutral
//! intermediate representation, not OTIO's own — [`otio_json`] serializes them to a real,
//! verified OpenTimelineIO `.otio` JSON document (write direction only so far; see that
//! submodule's own doc comment for exactly what's verified and why real `.otio` *import* is a
//! separate, later slice).
//!
//! [`RationalTime`]/[`TimeRange`] mirror the *concept* every editorial interchange format uses
//! (a time value counted in units of `1/rate` seconds), not any single format's exact field
//! names. [`sequence_to_interchange`] uses a fixed high-precision rate
//! ([`INTERCHANGE_TIME_RATE`], microsecond resolution) rather than each clip's own native frame
//! rate — a deliberate placeholder, not a claim of frame-accurate interchange; real OTIO export
//! should use each clip's probed [`crate::media::MediaAsset::fps`] instead, once that
//! serialization slice lands.

pub mod otio_json;

use std::path::{Path, PathBuf};

use crate::project::{Project, Sequence};
use crate::timeline::{
    BlendMode, ClipInstance, ColorFilter, Marker, MarkerKind, MaskShape, Timeline, Track,
    TrackKind, TransitionType,
};

/// The fixed time rate [`sequence_to_interchange`] expresses every [`RationalTime`] in — see this
/// module's own doc comment for why this isn't yet each clip's native frame rate.
pub const INTERCHANGE_TIME_RATE: f64 = 1_000_000.0;

/// A time value counted in units of `1/rate` seconds — the concept every editorial interchange
/// format represents time with, not any one format's own field names (see this module's doc
/// comment). `to_seconds()` is always `value / rate`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RationalTime {
    pub value: f64,
    pub rate: f64,
}

impl RationalTime {
    /// `rate` must be positive — every caller in this module passes [`INTERCHANGE_TIME_RATE`].
    pub fn from_seconds(seconds: f64, rate: f64) -> Self {
        Self {
            value: seconds * rate,
            rate,
        }
    }

    pub fn to_seconds(self) -> f64 {
        self.value / self.rate
    }
}

/// A half-open span of time: `[start_time, start_time + duration)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TimeRange {
    pub start_time: RationalTime,
    pub duration: RationalTime,
}

impl TimeRange {
    fn from_seconds(start_secs: f64, duration_secs: f64, rate: f64) -> Self {
        Self {
            start_time: RationalTime::from_seconds(start_secs, rate),
            duration: RationalTime::from_seconds(duration_secs, rate),
        }
    }
}

/// Where a clip's media actually comes from. [`Self::Missing`] is the doc's own acceptance
/// criterion ("Missing media produces offline references rather than dropping clips") — a clip
/// whose [`ClipInstance::asset_id`] no longer resolves in the project's media library still gets
/// an interchange clip, just with no resolvable path, rather than being silently omitted.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaReference {
    External { target_url: String },
    Missing,
}

/// Which [`TrackKind`]s this slice's editorial subset covers — `Text`/`Shape` overlay tracks
/// carry no media clips at all and aren't part of an editorial cut in the sense CF-05 targets,
/// so [`sequence_to_interchange`] omits them (reported via
/// [`interchange_compatibility_report`]) rather than inventing an OTIO-side representation for
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterchangeTrackKind {
    Video,
    Audio,
}

impl InterchangeTrackKind {
    fn from_track_kind(kind: TrackKind) -> Option<Self> {
        match kind {
            TrackKind::Video => Some(Self::Video),
            TrackKind::Audio => Some(Self::Audio),
            TrackKind::Text | TrackKind::Shape => None,
        }
    }
}

/// Carried on [`InterchangeClip::transition_in`] rather than modeled as its own track item — a
/// real OTIO `Transition` object consumes time from both neighboring clips via `in_offset`/
/// `out_offset`, which needs the same real-schema verification this module's own doc comment
/// says slice 1 defers. Kept here as clip-attached metadata so the information survives into
/// the interchange model at all; [`interchange_compatibility_report`] reports every non-`None`
/// occurrence as approximated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterchangeTransitionKind {
    Fade,
    HardCut,
    Slide,
    Zoom,
    Dissolve,
}

impl InterchangeTransitionKind {
    fn from_transition_type(kind: TransitionType) -> Option<Self> {
        match kind {
            TransitionType::None => None,
            TransitionType::Fade => Some(Self::Fade),
            TransitionType::HardCut => Some(Self::HardCut),
            TransitionType::Slide => Some(Self::Slide),
            TransitionType::Zoom => Some(Self::Zoom),
            TransitionType::Dissolve => Some(Self::Dissolve),
        }
    }

    fn to_transition_type(self) -> TransitionType {
        match self {
            Self::Fade => TransitionType::Fade,
            Self::HardCut => TransitionType::HardCut,
            Self::Slide => TransitionType::Slide,
            Self::Zoom => TransitionType::Zoom,
            Self::Dissolve => TransitionType::Dissolve,
        }
    }
}

/// One editorial clip — [`ClipInstance::start_secs`]/[`ClipInstance::source_in_secs`]/
/// [`ClipInstance::source_out_secs`] (speed-adjusted) mapped onto [`TimeRange`], plus the
/// resolved [`MediaReference`] and the doc's other required carried fields (speed, transition).
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeClip {
    pub source_id: u64,
    pub source_range: TimeRange,
    pub media_reference: MediaReference,
    pub speed_factor: f32,
    pub transition_in: Option<InterchangeTransitionKind>,
}

/// One item placed on an [`InterchangeTrack`], in on-timeline order.
#[derive(Debug, Clone, PartialEq)]
pub enum InterchangeTrackItem {
    Clip(InterchangeClip),
    /// Empty space between clips — most editorial interchange formats require a track's
    /// children to be contiguous, unlike [`crate::timeline::Track::clips`]' own sparse
    /// `start_secs`-addressed model, so [`sequence_to_interchange`] makes every gap explicit.
    Gap(TimeRange),
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeTrack {
    pub name: String,
    pub kind: InterchangeTrackKind,
    pub items: Vec<InterchangeTrackItem>,
}

/// One [`crate::timeline::Marker`], carried at its own point-in-time (zero-duration
/// [`TimeRange`]) — every interchange format's own marker concept the doc calls out.
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeMarker {
    pub name: String,
    pub marked_range: TimeRange,
    pub kind: MarkerKind,
}

/// The full result of [`sequence_to_interchange`] — one [`Sequence`]'s editorial subset, in a
/// schema-neutral form ready for a later slice to serialize to a real OTIO JSON schema version.
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeTimeline {
    pub name: String,
    pub tracks: Vec<InterchangeTrack>,
    pub markers: Vec<InterchangeMarker>,
}

/// Resolves `asset_id` against `project`'s media library into a [`MediaReference`] — external
/// when the asset still exists, [`MediaReference::Missing`] otherwise (a deleted asset, or a
/// compound clip's [`ClipInstance::nested_sequence_id`], which has no [`crate::media::MediaAsset`]
/// of its own).
fn resolve_media_reference(project: &Project, asset_id: u64) -> MediaReference {
    match project
        .media_library
        .iter()
        .find(|asset| asset.id == asset_id)
    {
        Some(asset) => MediaReference::External {
            target_url: asset.source_path.to_string_lossy().into_owned(),
        },
        None => MediaReference::Missing,
    }
}

fn clip_to_interchange(clip: &ClipInstance, project: &Project) -> InterchangeClip {
    InterchangeClip {
        source_id: clip.id,
        source_range: TimeRange::from_seconds(
            clip.source_in_secs,
            clip.source_out_secs - clip.source_in_secs,
            INTERCHANGE_TIME_RATE,
        ),
        media_reference: resolve_media_reference(project, clip.asset_id),
        speed_factor: clip.speed_factor,
        transition_in: InterchangeTransitionKind::from_transition_type(clip.transition_in),
    }
}

/// Builds one [`InterchangeTrack`]'s ordered items from `track`'s clips, inserting an explicit
/// [`InterchangeTrackItem::Gap`] wherever there's on-timeline space before/between them. Assumes
/// `track.clips` are non-overlapping (a real, pre-existing invariant every clip-placement code
/// path in this app already maintains) but does not assume they're pre-sorted by
/// [`ClipInstance::start_secs`].
fn track_items(track: &Track, project: &Project) -> Vec<InterchangeTrackItem> {
    let mut clips: Vec<&ClipInstance> = track.clips.iter().collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

    let mut items = Vec::with_capacity(clips.len() * 2);
    let mut cursor_secs = 0.0f64;
    for clip in clips {
        let gap_secs = clip.start_secs - cursor_secs;
        if gap_secs > 0.0 {
            items.push(InterchangeTrackItem::Gap(TimeRange::from_seconds(
                cursor_secs,
                gap_secs,
                INTERCHANGE_TIME_RATE,
            )));
        }
        items.push(InterchangeTrackItem::Clip(clip_to_interchange(
            clip, project,
        )));
        cursor_secs = clip.start_secs + clip.duration_secs();
    }
    items
}

/// Maps `sequence`'s [`Timeline`] into [`InterchangeTimeline`] — see this module's own doc
/// comment for what's carried (video/audio track order, clip source ranges, gaps, markers,
/// transition kind, speed) versus deferred (real OTIO serialization, transition offset
/// semantics, every non-editorial visual/audio effect — see
/// [`interchange_compatibility_report`] for the latter).
pub fn sequence_to_interchange(sequence: &Sequence, project: &Project) -> InterchangeTimeline {
    let timeline: &Timeline = &sequence.timeline;
    let tracks = timeline
        .tracks
        .iter()
        .filter_map(|track| {
            let kind = InterchangeTrackKind::from_track_kind(track.kind)?;
            Some(InterchangeTrack {
                name: track.name.clone(),
                kind,
                items: track_items(track, project),
            })
        })
        .collect();

    let markers = timeline
        .markers
        .iter()
        .map(|marker| InterchangeMarker {
            name: marker.label.clone(),
            marked_range: TimeRange::from_seconds(marker.position_secs, 0.0, INTERCHANGE_TIME_RATE),
            kind: marker.kind,
        })
        .collect();

    InterchangeTimeline {
        name: sequence.name.clone(),
        tracks,
        markers,
    }
}

/// Resolves a [`MediaReference`] back to a project's own [`crate::media::MediaAsset::id`] by
/// matching `target_url` against [`crate::media::MediaAsset::source_path`] — the reverse of
/// [`resolve_media_reference`]. Tries an exact path-string match first (the common case: this
/// project already carries the asset at the exact path the exporting machine recorded); when
/// that fails and `media_root` is given, falls back to [`resolve_under_media_root`] — the doc's
/// own "imported paths are normalized and cannot escape an explicitly selected media root"
/// acceptance criterion, for the cross-machine/cross-OS case where `target_url` is a foreign
/// absolute path (e.g. exported on Windows, imported on Linux) but the same file, under its own
/// name, already exists in this project's media library beneath a root the user explicitly
/// picked. `None` for [`MediaReference::Missing`] or a path that resolves to nothing in either
/// scheme (offline media, same as a real NLE's own relink-required case).
fn resolve_asset_id(
    project: &Project,
    media_reference: &MediaReference,
    media_root: Option<&Path>,
) -> Option<u64> {
    let MediaReference::External { target_url } = media_reference else {
        return None;
    };
    if let Some(asset) = project
        .media_library
        .iter()
        .find(|asset| asset.source_path.to_string_lossy() == *target_url)
    {
        return Some(asset.id);
    }
    let root = media_root?;
    let safe_path = resolve_under_media_root(root, target_url)?;
    project
        .media_library
        .iter()
        .find(|asset| asset.source_path.canonicalize().ok().as_deref() == Some(&safe_path))
        .map(|asset| asset.id)
}

/// Resolves `target_url`'s own file name against `media_root`, guaranteeing the result can never
/// escape that root — the literal contract behind CF-05's "imported paths are normalized and
/// cannot escape an explicitly selected media root" acceptance criterion. `target_url` is
/// untrusted: it comes from a `.otio` document that may have been authored on a different
/// machine, OS, or by a different tool entirely, so it is never joined onto `media_root` or
/// opened directly — only its final path segment is used, joined under `media_root`, then both
/// sides are canonicalized (resolving any symlink) and the candidate is required to still start
/// with the canonicalized root.
///
/// The final segment is extracted by splitting on both `/` and `\` explicitly, never via
/// `Path::file_name()` — that method only recognizes the *host's own* separator convention, so a
/// Windows-style `\`-separated `target_url` parsed on a Linux host (where `\` isn't a path
/// separator at all) would come back as one giant unsplit "file name" that simply fails to exist
/// under `media_root`, silently defeating the very cross-machine relink case this function exists
/// for (caught by running this against a real filesystem, not just type-checking it: a `.otio`
/// authored on Windows and relinked on Linux is exactly this feature's motivating scenario).
/// A resulting segment that is empty (a trailing separator), `.`, or `..` is rejected outright —
/// `media_root.join("..")` would climb to the parent directory despite containing no separator
/// of its own, so this check runs before candidate construction, not only after canonicalizing.
///
/// This defeats every escape vector a raw join would admit: `../../etc/passwd`-style traversal
/// (neutralized before the candidate is even built), an absolute path substituted wholesale for
/// `media_root`'s own prefix (only the bare final segment ever contributes), and a symlink
/// placed inside `media_root` that itself points outside it (only `canonicalize()`, which
/// resolves symlinks, can catch this — the starts_with check runs on the resolved target, not
/// the pre-resolution join). Returns `None` — never a partially-validated path — when
/// `target_url` has no usable final segment, the candidate doesn't exist, isn't a plain file, or
/// resolves outside `media_root`.
fn resolve_under_media_root(media_root: &Path, target_url: &str) -> Option<PathBuf> {
    let file_name = target_url.rsplit(['/', '\\']).next()?;
    if file_name.is_empty() || file_name == "." || file_name == ".." {
        return None;
    }
    let candidate = media_root.join(file_name);
    if !candidate.is_file() {
        return None;
    }
    let canonical_root = media_root.canonicalize().ok()?;
    let canonical_candidate = candidate.canonicalize().ok()?;
    canonical_candidate
        .starts_with(&canonical_root)
        .then_some(canonical_candidate)
}

/// A freshly reconstructed [`ClipInstance`] carrying only what [`InterchangeClip`] itself
/// carries — every other field (crop, color grading, masks, all other effects and keyframe
/// animation) at its own untouched default, since none of them round-trip through this slice's
/// interchange model yet (see [`interchange_compatibility_report`]).
fn clip_instance_from_interchange(
    id: u64,
    asset_id: u64,
    start_secs: f64,
    ic: &InterchangeClip,
) -> ClipInstance {
    ClipInstance {
        id,
        asset_id,
        start_secs,
        source_in_secs: ic.source_range.start_time.to_seconds(),
        source_out_secs: ic.source_range.start_time.to_seconds()
            + ic.source_range.duration.to_seconds(),
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor: ic.speed_factor,
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
        transition_in: ic
            .transition_in
            .map(InterchangeTransitionKind::to_transition_type)
            .unwrap_or(TransitionType::None),
        transition_duration_secs: 0.5,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
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

/// One clip [`interchange_to_timeline`] couldn't reconstruct, and why — the doc's own "preserve
/// unsupported fields as warnings, never silently approximate them" requirement, applied to
/// import's own failure case (an offline [`MediaReference`] that doesn't resolve against the
/// target project's media library) rather than inventing a placeholder asset.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportWarning {
    pub track_name: String,
    pub message: String,
}

/// [`interchange_to_timeline`]'s result: the reconstructed [`Timeline`] plus any clip that
/// couldn't be placed.
#[derive(Debug, Clone, PartialEq)]
pub struct InterchangeImportResult {
    pub timeline: Timeline,
    pub warnings: Vec<ImportWarning>,
}

/// Reconstructs a [`Timeline`] from `interchange`, the reverse of [`sequence_to_interchange`] —
/// CF-05's own slice 3 ("Import the same supported subset into a new sequence"), scoped for now
/// to this module's own intermediate representation rather than a real parsed `.otio` file (see
/// this module's own doc comment for why). `*next_id` is the first id to hand out; every
/// allocated [`Track::id`]/[`ClipInstance::id`] increments it, so a caller can keep allocating
/// unique ids afterward without recomputing a high-water mark itself. A clip whose
/// [`MediaReference`] no longer resolves against `project`'s media library is skipped — not
/// given a placeholder asset id — and reported in [`InterchangeImportResult::warnings`] instead,
/// per the doc's own "unsupported fields as warnings, never silently approximate" rule (extended
/// here to "unresolvable" as well as "unsupported"). `media_root`, when given, is an explicitly
/// user-selected folder [`resolve_asset_id`] may additionally match an unresolved reference's
/// bare file name against — see [`resolve_under_media_root`] for the path-safety contract this
/// guarantees.
pub fn interchange_to_timeline(
    interchange: &InterchangeTimeline,
    project: &Project,
    next_id: &mut u64,
    media_root: Option<&Path>,
) -> InterchangeImportResult {
    let mut warnings = Vec::new();
    let mut tracks = Vec::with_capacity(interchange.tracks.len());

    for itrack in &interchange.tracks {
        let kind = match itrack.kind {
            InterchangeTrackKind::Video => TrackKind::Video,
            InterchangeTrackKind::Audio => TrackKind::Audio,
        };
        let track_id = *next_id;
        *next_id += 1;

        let mut clips = Vec::new();
        let mut cursor_secs = 0.0f64;
        for item in &itrack.items {
            match item {
                InterchangeTrackItem::Gap(gap) => {
                    cursor_secs += gap.duration.to_seconds();
                }
                InterchangeTrackItem::Clip(ic) => {
                    let start_secs = cursor_secs;
                    let duration_secs = ic.source_range.duration.to_seconds()
                        / if ic.speed_factor == 0.0 {
                            1.0
                        } else {
                            ic.speed_factor as f64
                        };
                    match resolve_asset_id(project, &ic.media_reference, media_root) {
                        Some(asset_id) => {
                            let clip_id = *next_id;
                            *next_id += 1;
                            clips.push(clip_instance_from_interchange(
                                clip_id, asset_id, start_secs, ic,
                            ));
                        }
                        None => warnings.push(ImportWarning {
                            track_name: itrack.name.clone(),
                            message: format!(
                                "clip {} (source id {}): media reference does not resolve in this project — skipped",
                                clips.len() + 1,
                                ic.source_id
                            ),
                        }),
                    }
                    cursor_secs += duration_secs;
                }
            }
        }

        tracks.push(Track {
            id: track_id,
            name: itrack.name.clone(),
            kind,
            clips,
            text_clips: vec![],
            shape_clips: vec![],
            visible: true,
            audio_role: crate::timeline::AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        });
    }

    let markers = interchange
        .markers
        .iter()
        .map(|im| {
            let id = *next_id;
            *next_id += 1;
            Marker {
                id,
                position_secs: im.marked_range.start_time.to_seconds(),
                label: im.name.clone(),
                kind: im.kind,
                completed: false,
            }
        })
        .collect();

    InterchangeImportResult {
        timeline: Timeline {
            tracks,
            playhead_secs: 0.0,
            markers,
            multicam_groups: vec![],
        },
        warnings,
    }
}

/// One entry in an [`InterchangeCompatibilityReport`] — always names its track/clip context, per
/// the doc's own "Unsupported effects are reported with clip/track context" acceptance
/// criterion.
#[derive(Debug, Clone, PartialEq)]
pub struct CompatibilityReportEntry {
    pub track_name: String,
    pub clip_id: Option<u64>,
    pub description: String,
}

/// The result of [`interchange_compatibility_report`] — CF-05's slice 4 ("Add a compatibility
/// report listing exported, approximated, and omitted features"), built early (before real OTIO
/// serialization exists) since it only needs to know what *would* survive interchange, not
/// actually write a file.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct InterchangeCompatibilityReport {
    pub supported: Vec<CompatibilityReportEntry>,
    pub approximated: Vec<CompatibilityReportEntry>,
    pub omitted: Vec<CompatibilityReportEntry>,
}

fn entry(
    track_name: &str,
    clip_id: Option<u64>,
    description: impl Into<String>,
) -> CompatibilityReportEntry {
    CompatibilityReportEntry {
        track_name: track_name.to_string(),
        clip_id,
        description: description.into(),
    }
}

/// Effect/keyframe fields [`sequence_to_interchange`] never carries into the interchange model —
/// every non-empty one on a clip produces one `omitted` report entry. Deliberately conservative:
/// only fields with a real, currently-shipped visual/audio effect are checked, not every cosmetic
/// field ([`ClipInstance::color_label`] and the like stay out of the report — nothing about
/// interchange fidelity depends on them).
fn clip_omitted_effects(clip: &ClipInstance) -> Vec<&'static str> {
    let mut omitted = Vec::new();
    let has_crop = clip.crop_x != 0.0
        || clip.crop_y != 0.0
        || clip.crop_w != 1.0
        || clip.crop_h != 1.0
        || clip.has_crop_keyframes();
    if has_crop {
        omitted.push("crop/pan");
    }
    if clip.mask_shape != crate::timeline::MaskShape::None {
        omitted.push("mask");
    }
    if clip.flipped_h {
        omitted.push("horizontal flip");
    }
    let has_color_grade = clip.color_filter != crate::timeline::ColorFilter::None
        || clip.brightness != 0.0
        || clip.contrast != 1.0
        || clip.saturation != 1.0
        || clip.has_color_keyframes();
    if has_color_grade {
        omitted.push("color grading");
    }
    if clip.sharpen > 0.0 {
        omitted.push("sharpen");
    }
    if clip.chroma_key_enabled {
        omitted.push("chroma key");
    }
    if clip.blur_intensity > 0.0 {
        omitted.push("blur");
    }
    if clip.shake_intensity > 0.0 {
        omitted.push("shake");
    }
    if clip.glitch_intensity > 0.0 {
        omitted.push("glitch");
    }
    if clip.pixelize_intensity > 0.0 {
        omitted.push("pixelize");
    }
    if clip.vignette_intensity > 0.0 {
        omitted.push("vignette");
    }
    if !clip.lut_path.is_empty() {
        omitted.push("3D LUT");
    }
    if clip.stabilization_intensity > 0.0 {
        omitted.push("stabilization");
    }
    if clip.deflicker_enabled {
        omitted.push("deflicker");
    }
    if clip.background_removal_enabled {
        omitted.push("background removal");
    }
    if clip.voice_cleanup_enabled {
        omitted.push("voice cleanup");
    }
    if clip.has_position_keyframes() || clip.has_scale_keyframes() || clip.has_rotation_keyframes()
    {
        omitted.push("position/scale/rotation animation");
    }
    if clip.has_opacity_keyframes() {
        omitted.push("opacity animation");
    }
    if clip.has_gain_keyframes() {
        omitted.push("gain animation");
    }
    if clip.nested_sequence_id.is_some() {
        omitted.push("compound clip (nested sequence)");
    }
    omitted
}

/// Reports what [`sequence_to_interchange`] would carry, approximate, or drop for `sequence` —
/// CF-05's own doc-mandated compatibility report, independent of whether an actual `.otio` file
/// is ever written.
pub fn interchange_compatibility_report(sequence: &Sequence) -> InterchangeCompatibilityReport {
    let mut report = InterchangeCompatibilityReport::default();
    for track in &sequence.timeline.tracks {
        if InterchangeTrackKind::from_track_kind(track.kind).is_none() {
            report.omitted.push(entry(
                &track.name,
                None,
                "overlay track (text/shape) — not part of the editorial media subset yet",
            ));
            continue;
        }
        for clip in &track.clips {
            report.supported.push(entry(
                &track.name,
                Some(clip.id),
                "source range, speed, media reference",
            ));
            if clip.transition_in != TransitionType::None {
                report.approximated.push(entry(
                    &track.name,
                    Some(clip.id),
                    "transition kind carried, but not real in/out-offset semantics yet",
                ));
            }
            for effect in clip_omitted_effects(clip) {
                report
                    .omitted
                    .push(entry(&track.name, Some(clip.id), effect));
            }
        }
    }
    if !sequence.timeline.markers.is_empty() {
        report.supported.push(entry(
            "(timeline)",
            None,
            format!("{} marker(s)", sequence.timeline.markers.len()),
        ));
    }
    report
}

#[cfg(test)]
#[path = "interchange/interchange_test.rs"]
mod tests;
