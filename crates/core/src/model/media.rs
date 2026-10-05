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

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Video,
    Audio,
}

/// Result of a `loudnorm`-style analysis pass on a clip, before or after processing.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoudnessMetrics {
    pub integrated_lufs: f32,
    pub true_peak_dbtp: f32,
    pub loudness_range_lu: f32,
}

/// A source file imported into the project's media library. Built from
/// [`crate::probe::probe_media`] (via [`crate::probe::ProbedMedia::into_media_asset`]) plus a
/// [`crate::loudness::measure_loudness`] pass for the `loudness` field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaAsset {
    pub id: u64,
    pub file_name: String,
    /// Where the source file actually lives on disk — unlike `proxy_path`, this **is**
    /// serialized: it's the one piece of data a reopened project needs to find its media
    /// again, not a regenerable cache artifact.
    pub source_path: PathBuf,
    pub kind: MediaKind,
    /// Whether this source has an audio stream. Defaults to true for projects saved before the
    /// field existed, matching the editor's historical assumption that video clips had audio.
    #[serde(default = "default_has_audio")]
    pub has_audio: bool,
    pub duration_secs: f64,
    pub codec: String,
    pub source_bitrate_mbps: f32,
    /// `None` for audio-only assets.
    pub resolution: Option<(u32, u32)>,
    /// `None` for audio-only assets.
    pub fps: Option<f32>,
    /// `None` for video assets without a separate sample rate to show.
    pub sample_rate_khz: Option<f32>,
    pub loudness: Option<LoudnessMetrics>,
    /// Path to this asset's lightweight editing proxy (see [`crate::proxy`]), if one has been
    /// generated. Not serialized — it's a derived cache artifact, regenerated on demand (e.g.
    /// after the proxy cache is cleared, or the project is opened on another machine).
    #[serde(skip)]
    pub proxy_path: Option<PathBuf>,
    /// Per-bucket (min, max) amplitude peaks of this asset's audio track (see
    /// [`crate::waveform`]), for drawing a waveform under its audio clips on the timeline.
    /// `None` for a video-only asset, or before the background enrichment pass computes it.
    /// Serialized (unlike `proxy_path`) — recomputing it means a full audio decode pass, so a
    /// reopened project keeps it rather than paying that cost again.
    pub waveform_peaks: Option<Vec<(f32, f32)>>,
    /// User-marked favorite — the Media library panel's Favorites filter shows only assets with
    /// this set. Purely a manual flag (not derived from usage — see [`crate::project::Project::
    /// recent_asset_ids`] for that), toggled by a star button on each asset row/tile.
    /// `#[serde(default)]` so a project saved before this field existed loads with every asset
    /// unfavorited.
    #[serde(default)]
    pub favorited: bool,
}

fn default_has_audio() -> bool {
    true
}

impl MediaAsset {
    /// This asset's duration as a display-ready timecode (e.g. `"02:14"`).
    pub fn duration_label(&self) -> String {
        format_timecode(self.duration_secs)
    }
}

/// Formats seconds as `H:MM:SS` (or `MM:SS` under an hour), matching the mockup's timecodes.
pub fn format_timecode(total_secs: f64) -> String {
    let total = total_secs.round() as i64;
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}
