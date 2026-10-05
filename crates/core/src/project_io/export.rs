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

use serde::{Deserialize, Serialize};

use crate::{AudioSegment, Canvas, ClipSegment, PrivacyBlurSegment, ShapeSegment, TextSegment};

/// Target output aspect ratio for a timeline export. `Original` preserves the source
/// resolution inferred from the first clip; the fixed presets override width/height while
/// keeping fps and bitrate from the timeline analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportAspectRatio {
    /// Keep the canvas dimensions as inferred from the first clip's source resolution.
    #[default]
    Original,
    /// 1920 x 1080 — standard landscape (YouTube, Twitch).
    Landscape,
    /// 1080 x 1920 — vertical/shorts (YouTube Shorts, TikTok, Reels).
    Portrait,
    /// 1080 x 1080 — square (Instagram feed, general social).
    Square,
}

impl ExportAspectRatio {
    pub const ALL: &'static [ExportAspectRatio] = &[
        ExportAspectRatio::Original,
        ExportAspectRatio::Landscape,
        ExportAspectRatio::Portrait,
        ExportAspectRatio::Square,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ExportAspectRatio::Original => "Original",
            ExportAspectRatio::Landscape => "16:9",
            ExportAspectRatio::Portrait => "9:16",
            ExportAspectRatio::Square => "1:1",
        }
    }

    /// This ratio's target width/height, or `fallback` for [`ExportAspectRatio::Original`]
    /// (which has no fixed dimensions of its own — it keeps whatever the source/canvas already
    /// is). Shares the same numbers as [`crate::render::apply_export_aspect_ratio`]; `ui`'s
    /// preview panel uses this to size its canvas-space layer-transform overlay to the same
    /// aspect ratio export will actually use, without duplicating the width/height table.
    pub fn dims_or(self, fallback: (u32, u32)) -> (u32, u32) {
        match self {
            ExportAspectRatio::Original => fallback,
            ExportAspectRatio::Landscape => (1920, 1080),
            ExportAspectRatio::Portrait => (1080, 1920),
            ExportAspectRatio::Square => (1080, 1080),
        }
    }
}

/// A named export preset bundling a target aspect ratio (and, through it,
/// [`ExportAspectRatio::dims_or`]'s fixed resolution) with a loudness normalization target
/// under one platform-recognizable name — "pick once for TikTok" instead of separately setting
/// an aspect ratio and a LUFS target and hoping they're the right combination. Every preset here
/// happens to resolve to the same numbers today (1080x1920, -14 LUFS — the vertical/shorts
/// convention this codebase's own `LUFS_PROFILES` already labels "YouTube"), which is a real
/// current fact about these three platforms' delivery specs, not an assumption this type bakes
/// in structurally: each variant carries its own [`Self::settings`] independently, so a future
/// platform (or a spec change for one of these three) doesn't need this type's shape to change,
/// only one match arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlatformExportPreset {
    YoutubeShorts,
    InstagramReels,
    TikTok,
}

impl PlatformExportPreset {
    pub const ALL: &'static [PlatformExportPreset] = &[
        PlatformExportPreset::YoutubeShorts,
        PlatformExportPreset::InstagramReels,
        PlatformExportPreset::TikTok,
    ];

    /// Platform name as every one of them brands it — a proper noun, identical in every locale,
    /// same "not routed through `i18n::Text`" convention `ui`'s own `LUFS_PROFILES` labels use.
    pub fn label(self) -> &'static str {
        match self {
            PlatformExportPreset::YoutubeShorts => "YouTube Shorts",
            PlatformExportPreset::InstagramReels => "Instagram Reels",
            PlatformExportPreset::TikTok => "TikTok",
        }
    }

    /// This preset's target aspect ratio + loudness normalization target.
    pub fn settings(self) -> (ExportAspectRatio, f32) {
        match self {
            PlatformExportPreset::YoutubeShorts => (ExportAspectRatio::Portrait, -14.0),
            PlatformExportPreset::InstagramReels => (ExportAspectRatio::Portrait, -14.0),
            PlatformExportPreset::TikTok => (ExportAspectRatio::Portrait, -14.0),
        }
    }
}

#[cfg(test)]
mod export_aspect_ratio_test {
    use super::*;

    #[test]
    fn original_is_the_default() {
        assert_eq!(ExportAspectRatio::default(), ExportAspectRatio::Original);
    }

    #[test]
    fn all_lists_every_variant_exactly_once() {
        assert_eq!(ExportAspectRatio::ALL.len(), 4);
        let mut labels: Vec<&str> = ExportAspectRatio::ALL.iter().map(|r| r.label()).collect();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), ExportAspectRatio::ALL.len());
    }

    #[test]
    fn original_falls_back_to_the_given_dimensions() {
        assert_eq!(
            ExportAspectRatio::Original.dims_or((1280, 720)),
            (1280, 720)
        );
    }

    #[test]
    fn fixed_presets_ignore_the_fallback_and_use_their_own_dimensions() {
        assert_eq!(ExportAspectRatio::Landscape.dims_or((1, 1)), (1920, 1080));
        assert_eq!(ExportAspectRatio::Portrait.dims_or((1, 1)), (1080, 1920));
        assert_eq!(ExportAspectRatio::Square.dims_or((1, 1)), (1080, 1080));
    }

    #[test]
    fn labels_match_the_expected_ratio_notation() {
        assert_eq!(ExportAspectRatio::Original.label(), "Original");
        assert_eq!(ExportAspectRatio::Landscape.label(), "16:9");
        assert_eq!(ExportAspectRatio::Portrait.label(), "9:16");
        assert_eq!(ExportAspectRatio::Square.label(), "1:1");
    }
}

#[cfg(test)]
mod platform_export_preset_test {
    use super::*;

    #[test]
    fn every_preset_targets_the_vertical_aspect_ratio() {
        for preset in PlatformExportPreset::ALL {
            let (aspect_ratio, _) = preset.settings();
            assert_eq!(aspect_ratio, ExportAspectRatio::Portrait);
        }
    }

    #[test]
    fn every_preset_has_a_distinct_non_empty_label() {
        let labels: Vec<&str> = PlatformExportPreset::ALL
            .iter()
            .map(|p| p.label())
            .collect();
        for label in &labels {
            assert!(!label.is_empty());
        }
        let mut deduped = labels.clone();
        deduped.sort_unstable();
        deduped.dedup();
        assert_eq!(
            deduped.len(),
            labels.len(),
            "preset labels must be unique so the UI picker never shows two identical entries"
        );
    }
}

/// A queued render's lifecycle. `Rendering`/`Paused` carry a snapshot progress percentage;
/// the Fase 5 queue drives these via the background worker channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ExportJobStatus {
    Queued,
    Rendering { percent: u8 },
    Paused { percent: u8 },
    Done,
    Failed { message: String },
}

/// One export job: a snapshot of a sequence's video-track clips (and text overlays), resolved
/// to [`ClipSegment`]s and [`TextSegment`]s at the moment the job entered the queue — later
/// edits to the source project must not retroactively change a queued job. Rendered via
/// [`crate::render::render_export_job`] (main encode) followed by
/// [`crate::render::apply_text_overlay_pass`]-equivalent (pre-rasterized PNG post-processing).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    pub id: u64,
    pub title: String,
    /// Single-track segment list kept for backwards compatibility with jobs serialized before
    /// multi-track support was added.  When [`ExportJob::track_segments`] is non-empty,
    /// it takes precedence and this field is ignored by the render path.
    pub segments: Vec<ClipSegment>,
    /// Text overlay clips snapshotted from the sequence's text tracks at queue time.
    /// `#[serde(default)]` keeps existing saved queues (without this field) loading correctly.
    #[serde(default)]
    pub text_segments: Vec<TextSegment>,
    /// Shape overlay clips snapshotted from the sequence's shape tracks at queue time — same
    /// reasoning as `text_segments`. `#[serde(default)]` keeps existing saved queues (without
    /// this field) loading correctly.
    #[serde(default)]
    pub shape_segments: Vec<ShapeSegment>,
    /// CF-09 privacy-blur clips snapshotted from the sequence's video tracks at queue time —
    /// same reasoning as `text_segments`/`shape_segments`. `#[serde(default)]` keeps existing
    /// saved queues (without this field) loading correctly.
    #[serde(default)]
    pub privacy_blur_segments: Vec<PrivacyBlurSegment>,
    /// One inner `Vec<ClipSegment>` per visible video track, as returned by
    /// [`crate::render::resolve_timeline_segments_multi`].  Empty when the job was queued
    /// before multi-track support (those jobs fall back to [`ExportJob::segments`]).
    #[serde(default)]
    pub track_segments: Vec<Vec<ClipSegment>>,
    /// Complete audio mix snapshot when a track beyond the background contributes audio.
    /// Empty keeps persisted pre-mix jobs on the established background-only path.
    #[serde(default)]
    pub audio_segments: Vec<AudioSegment>,
    pub canvas: Canvas,
    pub target_lufs: f32,
    pub output_path: String,
    pub status: ExportJobStatus,
}
