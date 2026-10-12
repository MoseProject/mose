//! The project layouts of file versions 3 to 7, kept so such files still open.
//!
//! Version 8 added [`ClipInstance::grade`] as the last field of every clip, which moves every byte
//! after the first clip: version 7 and older are read through [`ClipInstanceV7`] (inside the same
//! mirror types as before, which now name it) and converted with a neutral grade.
//!
//! Version 7 widened [`TrackMix`] from volume and pan to the whole mixer strip (EQ, filters,
//! compressor), which moves every byte after the first track's mix: version 6 is read through
//! mirror types of the project, sequence, timeline and track, with the strip neutral.
//!
//! Version 6 added [`Track::mix`] inside every track, which moves every byte after the first
//! track: version 5 and older are read as mirror types of the sequence, timeline and track and
//! converted, with the new field neutral. Version 5 added
//! [`MediaAsset::streams`], [`MediaAsset::hdr_handling`] and [`MediaAsset::audio_stream`] inside
//! every asset, which moves every byte after the first asset: the old layout is read as these
//! mirror types and converted, with the new fields at their defaults.
//!
//! Only `MediaAsset` changed; everything else in a project is the same type in both layouts.

use std::path::PathBuf;

use serde::Deserialize;

use crate::frame_rate::FrameRate;
use crate::keyframe::{Keyframe, Position};
use crate::media::{HdrHandling, LoudnessMetrics, MediaAsset, MediaKind, StreamInfo};
use crate::project::{PanelLayout, Project, Recency, Sequence, SequenceExportSettings};
use crate::smart_bins::SmartBin;
use crate::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, Marker, MaskShape, ShapeClip, TextClip,
    Timeline, Track, TrackKind, TrackMix, TransitionType,
};
use crate::timeline::{Grade, MulticamGroup};

/// [`Project`] as version 5 wrote it (assets as now, tracks without a mix). Field order is the file
/// layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ProjectV5 {
    id: u64,
    name: String,
    last_edited: Recency,
    summary: String,
    media_library: Vec<MediaAsset>,
    sequences: Vec<SequenceV5>,
    active_sequence: usize,
    panel_layout: Option<PanelLayout>,
    smart_bins: Vec<SmartBin>,
    recent_asset_ids: Vec<u64>,
    frame_rate: Option<FrameRate>,
}

#[derive(Deserialize)]
struct SequenceV5 {
    id: u64,
    name: String,
    timeline: TimelineV5,
    export_settings: SequenceExportSettings,
}

#[derive(Deserialize)]
struct TimelineV5 {
    tracks: Vec<TrackV5>,
    playhead_secs: f64,
    markers: Vec<Marker>,
    multicam_groups: Vec<MulticamGroup>,
}

#[derive(Deserialize)]
struct TrackV5 {
    id: u64,
    name: String,
    kind: TrackKind,
    clips: Vec<ClipInstanceV7>,
    text_clips: Vec<TextClip>,
    shape_clips: Vec<ShapeClip>,
    visible: bool,
    audio_role: AudioRole,
    muted: bool,
    solo: bool,
    color_label: Option<[u8; 3]>,
    locked: bool,
}

impl From<SequenceV5> for Sequence {
    fn from(old: SequenceV5) -> Self {
        Sequence {
            id: old.id,
            name: old.name,
            timeline: Timeline {
                tracks: old
                    .timeline
                    .tracks
                    .into_iter()
                    .map(|track| Track {
                        id: track.id,
                        name: track.name,
                        kind: track.kind,
                        clips: track.clips.into_iter().map(Into::into).collect(),
                        text_clips: track.text_clips,
                        shape_clips: track.shape_clips,
                        visible: track.visible,
                        audio_role: track.audio_role,
                        muted: track.muted,
                        solo: track.solo,
                        color_label: track.color_label,
                        locked: track.locked,
                        mix: TrackMix::default(),
                    })
                    .collect(),
                playhead_secs: old.timeline.playhead_secs,
                markers: old.timeline.markers,
                multicam_groups: old.timeline.multicam_groups,
            },
            export_settings: old.export_settings,
        }
    }
}

impl From<ProjectV5> for Project {
    fn from(old: ProjectV5) -> Self {
        Project {
            id: old.id,
            name: old.name,
            last_edited: old.last_edited,
            summary: old.summary,
            media_library: old.media_library,
            sequences: old.sequences.into_iter().map(Into::into).collect(),
            active_sequence: old.active_sequence,
            file_path: None,
            panel_layout: old.panel_layout,
            smart_bins: old.smart_bins,
            recent_asset_ids: old.recent_asset_ids,
            frame_rate: old.frame_rate,
        }
    }
}

/// [`Project`] as version 4 wrote it. Field order is the file layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ProjectV4 {
    id: u64,
    name: String,
    last_edited: Recency,
    summary: String,
    media_library: Vec<MediaAssetV4>,
    sequences: Vec<SequenceV5>,
    active_sequence: usize,
    panel_layout: Option<PanelLayout>,
    smart_bins: Vec<SmartBin>,
    recent_asset_ids: Vec<u64>,
    frame_rate: Option<FrameRate>,
}

/// [`MediaAsset`] as version 4 wrote it.
#[derive(Deserialize)]
struct MediaAssetV4 {
    id: u64,
    file_name: String,
    source_path: PathBuf,
    kind: MediaKind,
    has_audio: bool,
    duration_secs: f64,
    codec: String,
    source_bitrate_mbps: f32,
    resolution: Option<(u32, u32)>,
    fps: Option<f32>,
    sample_rate_khz: Option<f32>,
    loudness: Option<LoudnessMetrics>,
    waveform_peaks: Option<Vec<(f32, f32)>>,
    favorited: bool,
}

impl From<MediaAssetV4> for MediaAsset {
    fn from(old: MediaAssetV4) -> Self {
        MediaAsset {
            id: old.id,
            file_name: old.file_name,
            source_path: old.source_path,
            kind: old.kind,
            has_audio: old.has_audio,
            duration_secs: old.duration_secs,
            codec: old.codec,
            source_bitrate_mbps: old.source_bitrate_mbps,
            resolution: old.resolution,
            fps: old.fps,
            sample_rate_khz: old.sample_rate_khz,
            loudness: old.loudness,
            proxy_path: None,
            waveform_peaks: old.waveform_peaks,
            favorited: old.favorited,
            streams: StreamInfo::default(),
            hdr_handling: HdrHandling::default(),
            audio_stream: None,
        }
    }
}

impl From<ProjectV4> for Project {
    fn from(old: ProjectV4) -> Self {
        Project {
            id: old.id,
            name: old.name,
            last_edited: old.last_edited,
            summary: old.summary,
            media_library: old.media_library.into_iter().map(Into::into).collect(),
            sequences: old.sequences.into_iter().map(Into::into).collect(),
            active_sequence: old.active_sequence,
            file_path: None,
            panel_layout: old.panel_layout,
            smart_bins: old.smart_bins,
            recent_asset_ids: old.recent_asset_ids,
            frame_rate: old.frame_rate,
        }
    }
}

/// [`TrackMix`] as version 6 wrote it. Field order is the file layout: do not touch.
#[derive(Deserialize)]
struct TrackMixV6 {
    volume_db: f32,
    pan: f32,
}

/// [`Project`] as version 6 wrote it (tracks with volume and pan only). Field order is the file
/// layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ProjectV6 {
    id: u64,
    name: String,
    last_edited: Recency,
    summary: String,
    media_library: Vec<MediaAsset>,
    sequences: Vec<SequenceV6>,
    active_sequence: usize,
    panel_layout: Option<PanelLayout>,
    smart_bins: Vec<SmartBin>,
    recent_asset_ids: Vec<u64>,
    frame_rate: Option<FrameRate>,
}

#[derive(Deserialize)]
struct SequenceV6 {
    id: u64,
    name: String,
    timeline: TimelineV6,
    export_settings: SequenceExportSettings,
}

#[derive(Deserialize)]
struct TimelineV6 {
    tracks: Vec<TrackV6>,
    playhead_secs: f64,
    markers: Vec<Marker>,
    multicam_groups: Vec<MulticamGroup>,
}

#[derive(Deserialize)]
struct TrackV6 {
    id: u64,
    name: String,
    kind: TrackKind,
    clips: Vec<ClipInstanceV7>,
    text_clips: Vec<TextClip>,
    shape_clips: Vec<ShapeClip>,
    visible: bool,
    audio_role: AudioRole,
    muted: bool,
    solo: bool,
    color_label: Option<[u8; 3]>,
    locked: bool,
    mix: TrackMixV6,
}

impl From<SequenceV6> for Sequence {
    fn from(old: SequenceV6) -> Self {
        Sequence {
            id: old.id,
            name: old.name,
            timeline: Timeline {
                tracks: old
                    .timeline
                    .tracks
                    .into_iter()
                    .map(|track| Track {
                        id: track.id,
                        name: track.name,
                        kind: track.kind,
                        clips: track.clips.into_iter().map(Into::into).collect(),
                        text_clips: track.text_clips,
                        shape_clips: track.shape_clips,
                        visible: track.visible,
                        audio_role: track.audio_role,
                        muted: track.muted,
                        solo: track.solo,
                        color_label: track.color_label,
                        locked: track.locked,
                        mix: TrackMix {
                            volume_db: track.mix.volume_db,
                            pan: track.mix.pan,
                            ..TrackMix::default()
                        },
                    })
                    .collect(),
                playhead_secs: old.timeline.playhead_secs,
                markers: old.timeline.markers,
                multicam_groups: old.timeline.multicam_groups,
            },
            export_settings: old.export_settings,
        }
    }
}

impl From<ProjectV6> for Project {
    fn from(old: ProjectV6) -> Self {
        Project {
            id: old.id,
            name: old.name,
            last_edited: old.last_edited,
            summary: old.summary,
            media_library: old.media_library,
            sequences: old.sequences.into_iter().map(Into::into).collect(),
            active_sequence: old.active_sequence,
            file_path: None,
            panel_layout: old.panel_layout,
            smart_bins: old.smart_bins,
            recent_asset_ids: old.recent_asset_ids,
            frame_rate: old.frame_rate,
        }
    }
}

/// [`ClipInstance`] as versions 7 and older wrote it, before the grade. Field order is the file
/// layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ClipInstanceV7 {
    id: u64,
    asset_id: u64,
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
    nested_sequence_id: Option<u64>,
    composite_id: Option<u64>,
    color_label: Option<[u8; 3]>,
    gain_db: f32,
    frozen: bool,
    speed_factor: f32,
    speed_ramp_end_factor: Option<f32>,
    crop_x: f32,
    crop_y: f32,
    crop_w: f32,
    crop_h: f32,
    crop_x_keyframes: Vec<Keyframe<f32>>,
    crop_y_keyframes: Vec<Keyframe<f32>>,
    crop_w_keyframes: Vec<Keyframe<f32>>,
    crop_h_keyframes: Vec<Keyframe<f32>>,
    mask_shape: MaskShape,
    mask_corner_radius: f32,
    flipped_h: bool,
    color_filter: ColorFilter,
    vignette_intensity: f32,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    brightness_keyframes: Vec<Keyframe<f32>>,
    contrast_keyframes: Vec<Keyframe<f32>>,
    saturation_keyframes: Vec<Keyframe<f32>>,
    sharpen: f32,
    chroma_key_enabled: bool,
    chroma_key_color: [u8; 3],
    chroma_key_tolerance: f32,
    blur_intensity: f32,
    shake_intensity: f32,
    glitch_intensity: f32,
    pixelize_intensity: f32,
    transition_in: TransitionType,
    transition_duration_secs: f32,
    fade_in_secs: f32,
    fade_out_secs: f32,
    position_keyframes: Vec<Keyframe<Position>>,
    scale_keyframes: Vec<Keyframe<f32>>,
    rotation_keyframes: Vec<Keyframe<f32>>,
    opacity_keyframes: Vec<Keyframe<f32>>,
    gain_keyframes: Vec<Keyframe<f32>>,
    voice_cleanup_enabled: bool,
    voice_cleanup_noise_floor_db: f32,
    voice_cleanup_compressor_threshold_db: f32,
    voice_cleanup_compressor_ratio: f32,
    voice_cleanup_ceiling_linear: f32,
    lut_path: String,
    layer_scale_x: f32,
    layer_scale_y: f32,
    deflicker_enabled: bool,
    stabilization_intensity: f32,
    background_removal_enabled: bool,
    background_removal_mask_path: String,
    blend_mode: BlendMode,
    anchor_x: f32,
    anchor_y: f32,
    reframe_seed_point: Option<(f32, f32)>,
    privacy_blur_enabled: bool,
    privacy_blur_mask_path: String,
    privacy_blur_sigma: f32,
    privacy_blur_seed_vertices: Vec<(f32, f32)>,
    privacy_blur_seed_center_x_frac: f32,
    privacy_blur_seed_center_y_frac: f32,
}

impl From<ClipInstanceV7> for ClipInstance {
    fn from(old: ClipInstanceV7) -> Self {
        ClipInstance {
            id: old.id,
            asset_id: old.asset_id,
            start_secs: old.start_secs,
            source_in_secs: old.source_in_secs,
            source_out_secs: old.source_out_secs,
            nested_sequence_id: old.nested_sequence_id,
            composite_id: old.composite_id,
            color_label: old.color_label,
            gain_db: old.gain_db,
            frozen: old.frozen,
            speed_factor: old.speed_factor,
            speed_ramp_end_factor: old.speed_ramp_end_factor,
            crop_x: old.crop_x,
            crop_y: old.crop_y,
            crop_w: old.crop_w,
            crop_h: old.crop_h,
            crop_x_keyframes: old.crop_x_keyframes,
            crop_y_keyframes: old.crop_y_keyframes,
            crop_w_keyframes: old.crop_w_keyframes,
            crop_h_keyframes: old.crop_h_keyframes,
            mask_shape: old.mask_shape,
            mask_corner_radius: old.mask_corner_radius,
            flipped_h: old.flipped_h,
            color_filter: old.color_filter,
            vignette_intensity: old.vignette_intensity,
            brightness: old.brightness,
            contrast: old.contrast,
            saturation: old.saturation,
            brightness_keyframes: old.brightness_keyframes,
            contrast_keyframes: old.contrast_keyframes,
            saturation_keyframes: old.saturation_keyframes,
            sharpen: old.sharpen,
            chroma_key_enabled: old.chroma_key_enabled,
            chroma_key_color: old.chroma_key_color,
            chroma_key_tolerance: old.chroma_key_tolerance,
            blur_intensity: old.blur_intensity,
            shake_intensity: old.shake_intensity,
            glitch_intensity: old.glitch_intensity,
            pixelize_intensity: old.pixelize_intensity,
            transition_in: old.transition_in,
            transition_duration_secs: old.transition_duration_secs,
            fade_in_secs: old.fade_in_secs,
            fade_out_secs: old.fade_out_secs,
            position_keyframes: old.position_keyframes,
            scale_keyframes: old.scale_keyframes,
            rotation_keyframes: old.rotation_keyframes,
            opacity_keyframes: old.opacity_keyframes,
            gain_keyframes: old.gain_keyframes,
            voice_cleanup_enabled: old.voice_cleanup_enabled,
            voice_cleanup_noise_floor_db: old.voice_cleanup_noise_floor_db,
            voice_cleanup_compressor_threshold_db: old.voice_cleanup_compressor_threshold_db,
            voice_cleanup_compressor_ratio: old.voice_cleanup_compressor_ratio,
            voice_cleanup_ceiling_linear: old.voice_cleanup_ceiling_linear,
            lut_path: old.lut_path,
            layer_scale_x: old.layer_scale_x,
            layer_scale_y: old.layer_scale_y,
            deflicker_enabled: old.deflicker_enabled,
            stabilization_intensity: old.stabilization_intensity,
            background_removal_enabled: old.background_removal_enabled,
            background_removal_mask_path: old.background_removal_mask_path,
            blend_mode: old.blend_mode,
            anchor_x: old.anchor_x,
            anchor_y: old.anchor_y,
            reframe_seed_point: old.reframe_seed_point,
            privacy_blur_enabled: old.privacy_blur_enabled,
            privacy_blur_mask_path: old.privacy_blur_mask_path,
            privacy_blur_sigma: old.privacy_blur_sigma,
            privacy_blur_seed_vertices: old.privacy_blur_seed_vertices,
            privacy_blur_seed_center_x_frac: old.privacy_blur_seed_center_x_frac,
            privacy_blur_seed_center_y_frac: old.privacy_blur_seed_center_y_frac,
            grade: Grade::default(),
        }
    }
}

/// [`Project`] as version 7 wrote it (clips without a grade). Field order is the file
/// layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ProjectV7 {
    id: u64,
    name: String,
    last_edited: Recency,
    summary: String,
    media_library: Vec<MediaAsset>,
    sequences: Vec<SequenceV7>,
    active_sequence: usize,
    panel_layout: Option<PanelLayout>,
    smart_bins: Vec<SmartBin>,
    recent_asset_ids: Vec<u64>,
    frame_rate: Option<FrameRate>,
}

#[derive(Deserialize)]
struct SequenceV7 {
    id: u64,
    name: String,
    timeline: TimelineV7,
    export_settings: SequenceExportSettings,
}

#[derive(Deserialize)]
struct TimelineV7 {
    tracks: Vec<TrackV7>,
    playhead_secs: f64,
    markers: Vec<Marker>,
    multicam_groups: Vec<MulticamGroup>,
}

#[derive(Deserialize)]
struct TrackV7 {
    id: u64,
    name: String,
    kind: TrackKind,
    clips: Vec<ClipInstanceV7>,
    text_clips: Vec<TextClip>,
    shape_clips: Vec<ShapeClip>,
    visible: bool,
    audio_role: AudioRole,
    muted: bool,
    solo: bool,
    color_label: Option<[u8; 3]>,
    locked: bool,
    mix: TrackMix,
}

impl From<SequenceV7> for Sequence {
    fn from(old: SequenceV7) -> Self {
        Sequence {
            id: old.id,
            name: old.name,
            timeline: Timeline {
                tracks: old
                    .timeline
                    .tracks
                    .into_iter()
                    .map(|track| Track {
                        id: track.id,
                        name: track.name,
                        kind: track.kind,
                        clips: track.clips.into_iter().map(Into::into).collect(),
                        text_clips: track.text_clips,
                        shape_clips: track.shape_clips,
                        visible: track.visible,
                        audio_role: track.audio_role,
                        muted: track.muted,
                        solo: track.solo,
                        color_label: track.color_label,
                        locked: track.locked,
                        mix: track.mix,
                    })
                    .collect(),
                playhead_secs: old.timeline.playhead_secs,
                markers: old.timeline.markers,
                multicam_groups: old.timeline.multicam_groups,
            },
            export_settings: old.export_settings,
        }
    }
}

impl From<ProjectV7> for Project {
    fn from(old: ProjectV7) -> Self {
        Project {
            id: old.id,
            name: old.name,
            last_edited: old.last_edited,
            summary: old.summary,
            media_library: old.media_library,
            sequences: old.sequences.into_iter().map(Into::into).collect(),
            active_sequence: old.active_sequence,
            file_path: None,
            panel_layout: old.panel_layout,
            smart_bins: old.smart_bins,
            recent_asset_ids: old.recent_asset_ids,
            frame_rate: old.frame_rate,
        }
    }
}
