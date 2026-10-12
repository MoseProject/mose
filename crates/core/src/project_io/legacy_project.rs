//! The project layout of file version 4, kept so such files still open. Version 5 added
//! [`MediaAsset::streams`], [`MediaAsset::hdr_handling`] and [`MediaAsset::audio_stream`] inside
//! every asset, which moves every byte after the first asset: the old layout is read as these
//! mirror types and converted, with the new fields at their defaults.
//!
//! Only `MediaAsset` changed; everything else in a project is the same type in both layouts.

use std::path::PathBuf;

use serde::Deserialize;

use crate::frame_rate::FrameRate;
use crate::media::{HdrHandling, LoudnessMetrics, MediaAsset, MediaKind, StreamInfo};
use crate::project::{PanelLayout, Project, Recency, Sequence};
use crate::smart_bins::SmartBin;

/// [`Project`] as version 4 wrote it. Field order is the file layout: do not touch.
#[derive(Deserialize)]
pub(crate) struct ProjectV4 {
    id: u64,
    name: String,
    last_edited: Recency,
    summary: String,
    media_library: Vec<MediaAssetV4>,
    sequences: Vec<Sequence>,
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
            sequences: old.sequences,
            active_sequence: old.active_sequence,
            file_path: None,
            panel_layout: old.panel_layout,
            smart_bins: old.smart_bins,
            recent_asset_ids: old.recent_asset_ids,
            frame_rate: old.frame_rate,
        }
    }
}
