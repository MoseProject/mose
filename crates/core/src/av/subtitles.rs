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

//! `.srt` subtitle file export — `request.md`'s Fase 4 ask for subtitles "exportável como
//! texto embutido no vídeo ou como arquivo `.srt` separado". The embedded half already exists
//! (`TextClip`'s shared Rust raster rendering on export); this covers the separate-file half. Pure
//! string formatting over already-placed [`crate::timeline::TextClip`]s — no FFI or
//! render-pipeline involvement, so it works the same whether or not this build can decode or
//! encode anything.

use crate::timeline::{TextClip, Timeline, TrackKind};

/// Renders every [`TextClip`] across every [`TrackKind::Text`] track in `timeline` as one
/// SubRip (`.srt`) file, ordered by `start_secs`. Clips whose text is empty/whitespace-only are
/// skipped — an empty caption entry has no meaning in a subtitle file. Returns an empty string
/// if there are no text clips at all.
pub fn export_srt(timeline: &Timeline) -> String {
    let mut clips: Vec<&TextClip> = timeline
        .tracks
        .iter()
        .filter(|track| track.kind == TrackKind::Text)
        .flat_map(|track| track.text_clips.iter())
        .filter(|clip| !clip.text.trim().is_empty())
        .collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

    let mut out = String::new();
    for (index, clip) in clips.iter().enumerate() {
        out.push_str(&(index + 1).to_string());
        out.push('\n');
        out.push_str(&format_srt_timestamp(clip.start_secs));
        out.push_str(" --> ");
        // Never ends before it starts, whatever duration the clip holds.
        let end_secs = (clip.start_secs + clip.duration_secs).max(clip.start_secs);
        out.push_str(&format_srt_timestamp(end_secs));
        out.push('\n');
        out.push_str(&srt_text(&clip.text));
        out.push_str("\n\n");
    }
    out
}

/// A caption's text as an SRT entry body: a blank line ends an entry in the format, so blank
/// lines inside the text are dropped (the line breaks around them stay), and Windows line endings
/// become plain ones.
fn srt_text(text: &str) -> String {
    text.lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Formats seconds as SRT's `HH:MM:SS,mmm` timestamp. Negative input clamps to zero — a
/// `start_secs` before the timeline origin shouldn't happen, but a subtitle file has no way to
/// represent a negative time regardless.
fn format_srt_timestamp(total_secs: f64) -> String {
    let total_millis = (total_secs.max(0.0) * 1000.0).round() as u64;
    let millis = total_millis % 1000;
    let total_seconds = total_millis / 1000;
    let secs = total_seconds % 60;
    let total_minutes = total_seconds / 60;
    let mins = total_minutes % 60;
    let hours = total_minutes / 60;
    format!("{hours:02}:{mins:02}:{secs:02},{millis:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::{AudioRole, Track};

    fn text_clip(id: u64, start_secs: f64, duration_secs: f64, text: &str) -> TextClip {
        TextClip {
            id,
            start_secs,
            duration_secs,
            text: text.to_string(),
            font_size: 32.0,
            font_family: Default::default(),
            font_style: Default::default(),
            font_weight: None,
            color_rgba: [255, 255, 255, 255],
            background_rgba: [0, 0, 0, 0],
            background_padding: 8.0,
            background_corner_radius: 8.0,
            pos_x: 0.5,
            pos_y: 0.9,
            words: Vec::new(),
            highlight_enabled: false,
            highlight_color_rgba: [255, 220, 0, 255],
            opacity_keyframes: vec![],
            pos_x_keyframes: vec![],
            pos_y_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            direction: Default::default(),
            language: None,
            text_align: Default::default(),
        }
    }

    fn timeline_with_text_track(clips: Vec<TextClip>) -> Timeline {
        Timeline {
            tracks: vec![Track {
                id: 1,
                name: "Text".to_string(),
                kind: TrackKind::Text,
                clips: Vec::new(),
                text_clips: clips,
                shape_clips: Vec::new(),
                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                locked: false,
                mix: Default::default(),
                color_label: None,
            }],
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        }
    }

    #[test]
    fn formats_a_timestamp_with_hours_minutes_seconds_and_millis() {
        assert_eq!(format_srt_timestamp(0.0), "00:00:00,000");
        assert_eq!(format_srt_timestamp(1.5), "00:00:01,500");
        assert_eq!(format_srt_timestamp(65.25), "00:01:05,250");
        assert_eq!(format_srt_timestamp(3661.001), "01:01:01,001");
    }

    #[test]
    fn clamps_a_negative_timestamp_to_zero() {
        assert_eq!(format_srt_timestamp(-2.0), "00:00:00,000");
    }

    #[test]
    fn exports_two_clips_as_sequential_numbered_entries() {
        let timeline = timeline_with_text_track(vec![
            text_clip(1, 1.0, 3.0, "First line"),
            text_clip(2, 5.0, 2.0, "Second line"),
        ]);

        let srt = export_srt(&timeline);

        assert_eq!(
            srt,
            "1\n00:00:01,000 --> 00:00:04,000\nFirst line\n\n2\n00:00:05,000 --> 00:00:07,000\nSecond line\n\n"
        );
    }

    #[test]
    fn orders_entries_by_start_time_regardless_of_input_order() {
        let timeline = timeline_with_text_track(vec![
            text_clip(1, 5.0, 1.0, "Later"),
            text_clip(2, 1.0, 1.0, "Earlier"),
        ]);

        let srt = export_srt(&timeline);

        let earlier_pos = srt.find("Earlier").unwrap();
        let later_pos = srt.find("Later").unwrap();
        assert!(earlier_pos < later_pos);
    }

    #[test]
    fn skips_clips_with_empty_or_whitespace_only_text() {
        let timeline = timeline_with_text_track(vec![
            text_clip(1, 1.0, 1.0, "   "),
            text_clip(2, 2.0, 1.0, "Real caption"),
        ]);

        let srt = export_srt(&timeline);

        assert_eq!(srt, "1\n00:00:02,000 --> 00:00:03,000\nReal caption\n\n");
    }

    #[test]
    fn ignores_non_text_tracks() {
        let mut timeline = timeline_with_text_track(vec![text_clip(1, 1.0, 1.0, "Caption")]);
        timeline.tracks.push(Track {
            id: 2,
            name: "Video".to_string(),
            kind: TrackKind::Video,
            clips: Vec::new(),
            text_clips: vec![text_clip(2, 2.0, 1.0, "Should not appear")],
            shape_clips: Vec::new(),
            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        });

        let srt = export_srt(&timeline);

        assert!(!srt.contains("Should not appear"));
    }

    #[test]
    fn returns_an_empty_string_for_a_timeline_with_no_text_clips() {
        let timeline = timeline_with_text_track(vec![]);
        assert_eq!(export_srt(&timeline), "");
    }
}
