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

//! P4 item 22, "Smart bins" (`spec/ROADMAP.md`): rule-based media-pool folders that
//! auto-populate by file type/name/audio presence, DaVinci Resolve's Smart Bins. Lower priority
//! for a small/single-editor channel than a studio pipeline, but real and, unlike most of the
//! rest of P4/P5, needs neither GPU hardware nor a GStreamer element this dev machine lacks —
//! pure filtering over [`crate::MediaAsset`], the same "no mock/sample data, just real project
//! state" shape every other `core` model already has.
//!
//! A bin is defined by its rules, not by which assets it currently contains — nothing is stored
//! per-membership; [`SmartBin::matches`] is evaluated fresh against the live `media_library`
//! every time (same "recompute, don't cache stale membership" reasoning [`crate::timeline::
//! Marker`] and every other lightweight sidecar record in this codebase already follows).

use serde::{Deserialize, Serialize};

use crate::media::{MediaAsset, MediaKind};

/// A named filter over a project's `media_library`. Every set criterion must match
/// (AND, not OR) — an unset criterion (`None`/empty string) imposes no constraint.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SmartBin {
    pub id: u64,
    pub name: String,
    /// `None` matches both video and audio assets.
    #[serde(default)]
    pub kind_filter: Option<MediaKind>,
    /// Case-insensitive substring match against `MediaAsset::file_name`. Empty imposes no
    /// constraint.
    #[serde(default)]
    pub name_contains: String,
    /// `None` doesn't care whether the asset has an audio stream; `Some(true)`/`Some(false)`
    /// requires it to (not) have one.
    #[serde(default)]
    pub requires_audio: Option<bool>,
}

impl SmartBin {
    /// A fresh, unnamed bin with `id` and every criterion unset (matches everything until the
    /// caller fills in a rule) — the starting point for the "+ New Bin" flow.
    pub fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            kind_filter: None,
            name_contains: String::new(),
            requires_audio: None,
        }
    }

    /// Whether `asset` satisfies every criterion this bin has set.
    pub fn matches(&self, asset: &MediaAsset) -> bool {
        if let Some(kind) = self.kind_filter {
            if asset.kind != kind {
                return false;
            }
        }
        if !self.name_contains.is_empty()
            && !asset
                .file_name
                .to_lowercase()
                .contains(&self.name_contains.to_lowercase())
        {
            return false;
        }
        if let Some(requires_audio) = self.requires_audio {
            if asset.has_audio != requires_audio {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(file_name: &str, kind: MediaKind, has_audio: bool) -> MediaAsset {
        MediaAsset {
            id: 1,
            file_name: file_name.to_string(),
            source_path: file_name.into(),
            kind,
            has_audio,
            duration_secs: 10.0,
            codec: "h264".to_string(),
            source_bitrate_mbps: 8.0,
            resolution: Some((1920, 1080)),
            fps: Some(30.0),
            sample_rate_khz: None,
            loudness: None,
            proxy_path: None,
            waveform_peaks: None,
            favorited: false,
        }
    }

    #[test]
    fn an_empty_bin_matches_everything() {
        let bin = SmartBin::new(1, "All".to_string());
        assert!(bin.matches(&asset("clip.mp4", MediaKind::Video, true)));
        assert!(bin.matches(&asset("song.mp3", MediaKind::Audio, true)));
    }

    #[test]
    fn kind_filter_excludes_the_other_kind() {
        let mut bin = SmartBin::new(1, "Video only".to_string());
        bin.kind_filter = Some(MediaKind::Video);
        assert!(bin.matches(&asset("clip.mp4", MediaKind::Video, true)));
        assert!(!bin.matches(&asset("song.mp3", MediaKind::Audio, true)));
    }

    #[test]
    fn name_contains_is_case_insensitive() {
        let mut bin = SmartBin::new(1, "Boss fights".to_string());
        bin.name_contains = "BOSS".to_string();
        assert!(bin.matches(&asset("boss_fight_01.mp4", MediaKind::Video, true)));
        assert!(!bin.matches(&asset("intro.mp4", MediaKind::Video, true)));
    }

    #[test]
    fn requires_audio_filters_either_way() {
        let mut bin = SmartBin::new(1, "Silent".to_string());
        bin.requires_audio = Some(false);
        assert!(bin.matches(&asset("silent.mp4", MediaKind::Video, false)));
        assert!(!bin.matches(&asset("loud.mp4", MediaKind::Video, true)));
    }

    #[test]
    fn multiple_criteria_are_anded_together() {
        let mut bin = SmartBin::new(1, "Boss video with audio".to_string());
        bin.kind_filter = Some(MediaKind::Video);
        bin.name_contains = "boss".to_string();
        bin.requires_audio = Some(true);

        assert!(bin.matches(&asset("boss_01.mp4", MediaKind::Video, true)));
        assert!(!bin.matches(&asset("boss_01.mp4", MediaKind::Video, false)));
        assert!(!bin.matches(&asset("intro.mp4", MediaKind::Video, true)));
        assert!(!bin.matches(&asset("boss_theme.mp3", MediaKind::Audio, true)));
    }
}
