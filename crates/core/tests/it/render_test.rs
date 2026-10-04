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

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use avcore::project::Sequence;
use avcore::render::{render_export, resolve_shape_segments, resolve_text_segments, RenderOutcome};
use avcore::timeline::{
    AudioRole, ShapeClip, ShapeKind, TextClip, Timeline, Track, TrackKind, WordTiming,
};
use avcore::{measure_loudness, probe_media};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn renders_and_normalizes_loudness_toward_target() {
    let source = fixture("video.mp4");
    let output = std::env::temp_dir().join("avcore_test_render_ok.mp4");
    let duration_secs = probe_media(&source).unwrap().duration_secs;
    let cancel = AtomicBool::new(false);
    let mut last_percent = 0u8;

    let outcome = render_export(&source, &output, -14.0, duration_secs, &cancel, |percent| {
        last_percent = percent;
    })
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);
    assert_eq!(last_percent, 100);

    // Independent cross-check: measure_loudness is a *separate* code path (still spawns
    // ffmpeg as a subprocess) from render_export's FFI encode, so this genuinely verifies
    // the normalization happened rather than re-testing the same code against itself.
    let source_loudness = measure_loudness(&source).unwrap();
    let output_loudness = measure_loudness(&output).unwrap();
    assert!(output_loudness.integrated_lufs > source_loudness.integrated_lufs);
    assert!((output_loudness.integrated_lufs - (-14.0)).abs() < 2.0);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn cancelling_mid_render_reports_cancelled() {
    let source = fixture("video.mp4");
    let output = std::env::temp_dir().join("avcore_test_render_cancelled.mp4");
    let duration_secs = probe_media(&source).unwrap().duration_secs;
    let cancel = AtomicBool::new(false);
    let mut calls = 0;

    let outcome = render_export(
        &source,
        &output,
        -14.0,
        duration_secs,
        &cancel,
        |_percent| {
            calls += 1;
            if calls >= 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Cancelled);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn rejects_a_missing_source() {
    let output = std::env::temp_dir().join("avcore_test_render_missing.mp4");
    let cancel = AtomicBool::new(false);

    let result = render_export(
        &fixture("does_not_exist.mp4"),
        &output,
        -14.0,
        1.0,
        &cancel,
        |_| {},
    );

    assert!(result.is_err());
}

fn text_clip(words: Vec<WordTiming>, highlight_enabled: bool) -> TextClip {
    TextClip {
        id: 1,
        start_secs: 10.0,
        duration_secs: 2.0,
        text: "Hello World".to_string(),
        font_size: 48.0,
        font_family: Default::default(),
        font_style: Default::default(),
        font_weight: None,
        color_rgba: [255, 255, 255, 255],
        background_rgba: [0, 0, 0, 0],
        background_padding: 8.0,
        background_corner_radius: 8.0,
        pos_x: 0.1,
        pos_y: 0.85,
        words,
        highlight_enabled,
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

fn sequence_with_text_track(clip: TextClip) -> Sequence {
    Sequence {
        id: 1,
        name: "Sequence 1".to_string(),
        timeline: Timeline {
            tracks: vec![Track {
                id: 1,
                name: "Legendas".to_string(),
                kind: TrackKind::Text,
                clips: vec![],
                text_clips: vec![clip],
                shape_clips: vec![],
                visible: true,
                audio_role: AudioRole::Unspecified,
                locked: false,
                color_label: None,
            }],
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        },
        export_settings: Default::default(),
    }
}

#[test]
fn resolve_text_segments_returns_one_segment_for_a_plain_text_clip() {
    let mut clip = text_clip(Vec::new(), false);
    clip.font_family = avcore::TextFontFamily::PlayfairDisplay;
    clip.font_style = avcore::TextFontStyle::Bold;
    clip.background_rgba = [10, 20, 30, 180];
    clip.background_padding = 12.0;
    clip.background_corner_radius = 6.0;
    let sequence = sequence_with_text_track(clip);

    let segments = resolve_text_segments(&sequence, 1920, 1080);

    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text, "Hello World");
    assert_eq!(segments[0].start_secs, 10.0);
    assert_eq!(segments[0].duration_secs, 2.0);
    assert_eq!(segments[0].pos_x, 0.1);
    assert_eq!(
        segments[0].font_family,
        avcore::TextFontFamily::PlayfairDisplay
    );
    assert_eq!(segments[0].font_style, avcore::TextFontStyle::Bold);
    assert_eq!(segments[0].background_rgba, [10, 20, 30, 180]);
    assert_eq!(segments[0].background_padding, 12.0);
    assert_eq!(segments[0].background_corner_radius, 6.0);
}

#[test]
fn resolve_text_segments_ignores_words_when_highlight_is_disabled() {
    let words = vec![
        WordTiming {
            text: "Hello".to_string(),
            start_secs: 0.0,
            end_secs: 0.5,
        },
        WordTiming {
            text: "World".to_string(),
            start_secs: 0.5,
            end_secs: 1.0,
        },
    ];
    let sequence = sequence_with_text_track(text_clip(words, false));

    let segments = resolve_text_segments(&sequence, 1920, 1080);

    assert_eq!(segments.len(), 1);
}

#[test]
fn resolve_text_segments_expands_a_highlighted_clip_into_a_base_plus_one_segment_per_word() {
    let words = vec![
        WordTiming {
            text: "Hello".to_string(),
            start_secs: 0.0,
            end_secs: 0.5,
        },
        WordTiming {
            text: "World".to_string(),
            start_secs: 0.5,
            end_secs: 1.0,
        },
    ];
    let sequence = sequence_with_text_track(text_clip(words, true));

    let segments = resolve_text_segments(&sequence, 1920, 1080);

    // 1 base (full sentence, whole clip duration) + 2 word overlays.
    assert_eq!(segments.len(), 3);

    let base = segments
        .iter()
        .find(|s| s.glyph_byte_range.is_none())
        .expect("base segment missing");
    assert_eq!(base.start_secs, 10.0);
    assert_eq!(base.duration_secs, 2.0);
    assert_eq!(base.color_rgba, [255, 255, 255, 255]);
    assert_eq!(base.glyph_byte_range, None);

    let word_segments: Vec<_> = segments
        .iter()
        .filter(|s| s.glyph_byte_range.is_some())
        .collect();
    assert_eq!(word_segments.len(), 2);
    for word in &word_segments {
        assert_eq!(word.color_rgba, [255, 220, 0, 255]);
        assert_eq!(word.background_rgba, [0, 0, 0, 0]);
        assert_eq!(word.text, "Hello World");
        assert_eq!(word.pos_x, base.pos_x);
        assert_eq!(word.pos_y, base.pos_y);
        // Timeline-absolute: clip.start_secs (10.0) + the word's own relative offset.
        assert!(word.start_secs >= 10.0 && word.start_secs < 11.5);
    }
    assert_eq!(word_segments[0].glyph_byte_range, Some([0, 5]));
    assert_eq!(word_segments[1].glyph_byte_range, Some([6, 11]));
}

#[test]
fn resolve_text_segments_falls_back_to_one_segment_when_canvas_width_is_zero() {
    let words = vec![WordTiming {
        text: "Hello".to_string(),
        start_secs: 0.0,
        end_secs: 0.5,
    }];
    let sequence = sequence_with_text_track(text_clip(words, true));

    let segments = resolve_text_segments(&sequence, 0, 1080);

    assert_eq!(segments.len(), 1);
}

fn shape_clip(id: u64, start_secs: f64, duration_secs: f64) -> ShapeClip {
    ShapeClip {
        id,
        start_secs,
        duration_secs,
        shape_kind: ShapeKind::rectangle(),
        center_x: 0.5,
        center_y: 0.5,
        center_x_keyframes: vec![],
        center_y_keyframes: vec![],
        width_keyframes: vec![],
        height_keyframes: vec![],
        rotation_keyframes: vec![],
        width: 0.3,
        height: 0.3,
        rotation_deg: 0.0,
        color_rgba: [255, 255, 255, 255],
        stroke_thickness_px: 0.0,
    }
}

fn sequence_with_shape_track(clips: Vec<ShapeClip>) -> Sequence {
    Sequence {
        id: 1,
        name: "Sequence 1".to_string(),
        timeline: Timeline {
            tracks: vec![Track {
                id: 1,
                name: "Forma".to_string(),
                kind: TrackKind::Shape,
                clips: vec![],
                text_clips: vec![],
                shape_clips: clips,
                visible: true,
                audio_role: AudioRole::Unspecified,
                locked: false,
                color_label: None,
            }],
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        },
        export_settings: Default::default(),
    }
}

#[test]
fn resolve_shape_segments_returns_one_segment_for_a_shape_clip() {
    let sequence = sequence_with_shape_track(vec![shape_clip(1, 1.0, 2.0)]);

    let segments = resolve_shape_segments(&sequence, 1920, 1080);

    assert_eq!(segments.len(), 1);
    assert!(segments[0].filter_desc.starts_with("geq="));
    assert!(segments[0]
        .filter_desc
        .contains("between(t\\,1.0000\\,3.0000)"));
}

#[test]
fn resolve_shape_segments_sorts_by_start_secs_ascending() {
    // Deliberately built out of start_secs order.
    let sequence =
        sequence_with_shape_track(vec![shape_clip(2, 5.0, 1.0), shape_clip(1, 0.0, 1.0)]);

    let segments = resolve_shape_segments(&sequence, 1920, 1080);

    assert_eq!(segments.len(), 2);
    assert!(segments[0]
        .filter_desc
        .contains("between(t\\,0.0000\\,1.0000)"));
    assert!(segments[1]
        .filter_desc
        .contains("between(t\\,5.0000\\,6.0000)"));
}

#[test]
fn resolve_shape_segments_returns_empty_when_no_shape_track_exists() {
    let sequence = sequence_with_text_track(text_clip(Vec::new(), false));

    let segments = resolve_shape_segments(&sequence, 1920, 1080);

    assert!(segments.is_empty());
}
