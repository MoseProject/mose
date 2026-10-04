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
use avcore::render::{render_timeline_export, RenderError, RenderOutcome};
use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, MaskShape, TextClip, Timeline, Track,
    TrackKind, TransitionType, WordTiming,
};
use avcore::Keyframe;
use avcore::{probe_media, MediaAsset, MediaKind};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn clip(
    id: u64,
    asset_id: u64,
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
) -> ClipInstance {
    ClipInstance {
        id,
        asset_id,
        start_secs,
        source_in_secs,
        source_out_secs,
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor: 1.0,
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
        transition_in: TransitionType::None,
        transition_duration_secs: 0.5,
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

fn video_asset(id: u64) -> MediaAsset {
    let path = fixture("video.mp4");
    probe_media(&path)
        .unwrap()
        .into_media_asset(id, "video.mp4".to_string(), path)
}

fn sequence_with(tracks: Vec<Track>) -> Sequence {
    Sequence {
        id: 1,
        name: "Sequence 1".to_string(),
        timeline: Timeline {
            tracks,
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        },
        export_settings: Default::default(),
    }
}

#[test]
fn renders_two_clips_with_different_effects_as_one_concatenated_export() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.3);
    let mut c2 = clip(2, 1, 0.3, 0.3, 0.6);
    c1.flipped_h = true;
    c2.vignette_intensity = 0.5;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c2, c1], // deliberately out of start_secs order

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_ok.mp4");
    let cancel = AtomicBool::new(false);
    let mut last_percent = 0u8;

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |percent| last_percent = percent,
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);
    assert_eq!(last_percent, 100);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn frozen_clip_still_exports_its_full_timeline_duration() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.3);
    c1.frozen = true;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_frozen.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn rejects_a_sequence_with_no_video_track() {
    let sequence = sequence_with(vec![]);
    let output = std::env::temp_dir().join("avcore_test_timeline_export_empty.mp4");
    let cancel = AtomicBool::new(false);

    let err = render_timeline_export(
        &sequence,
        &[],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, RenderError::EmptyTimeline));
}

#[test]
fn rejects_a_clip_with_a_missing_asset() {
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 999, 0.0, 0.0, 0.3)],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);
    let output = std::env::temp_dir().join("avcore_test_timeline_export_missing_asset.mp4");
    let cancel = AtomicBool::new(false);

    // No assets at all in the media library — clip's asset_id (999) can't resolve.
    let err = render_timeline_export(
        &sequence,
        &[],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, RenderError::MissingAsset));
}

#[test]
fn cancelling_mid_timeline_export_reports_cancelled() {
    let asset = video_asset(1);
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 1, 0.0, 0.0, 0.3), clip(2, 1, 0.3, 0.3, 0.6)],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);
    let output = std::env::temp_dir().join("avcore_test_timeline_export_cancelled.mp4");
    let cancel = AtomicBool::new(false);
    let mut calls = 0;

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
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
fn deflicker_exports_without_error() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.deflicker_enabled = true;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_deflicker.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn word_highlight_text_overlay_exports_without_error() {
    let asset = video_asset(1);
    let video_track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 1, 0.0, 0.0, 0.5)],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let text_track = Track {
        id: 2,
        name: "Legendas".to_string(),
        kind: TrackKind::Text,
        clips: vec![],
        text_clips: vec![TextClip {
            id: 1,
            start_secs: 0.0,
            duration_secs: 0.4,
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
            words: vec![
                WordTiming {
                    text: "Hello".to_string(),
                    start_secs: 0.0,
                    end_secs: 0.2,
                },
                WordTiming {
                    text: "World".to_string(),
                    start_secs: 0.2,
                    end_secs: 0.4,
                },
            ],
            highlight_enabled: true,
            highlight_color_rgba: [255, 220, 0, 255],
            opacity_keyframes: vec![],
            pos_x_keyframes: vec![],
            pos_y_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            direction: Default::default(),
            language: None,
            text_align: Default::default(),
        }],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![video_track, text_track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_word_highlight.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn fade_transition_exports_without_error() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.transition_in = TransitionType::Fade;
    c1.transition_duration_secs = 0.3;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_fade.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn slide_transition_exports_without_error() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.transition_in = TransitionType::Slide;
    c1.transition_duration_secs = 0.3;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_slide.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn zoom_transition_exports_without_error() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.transition_in = TransitionType::Zoom;
    c1.transition_duration_secs = 0.3;

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_zoom_transition.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn animated_scale_keyframes_export_without_error() {
    // Two keyframes (1.0 -> 1.5) is the multi-point-piecewise, non-degenerate branch of
    // scale_filter_expr — the direct descendant of the old animated Ken-Burns zoom test: a
    // single-keyframe-equivalent (zoom_start == zoom_end) case used to be (and still is,
    // covered by every other fixture in this file which leaves scale_keyframes empty) the
    // "safe" static-crop branch; the truly animated geq expression is what previously failed
    // filter-graph init outright ("Expressions with frame variables 'n', 't', 'pos' are not
    // valid in init eval_mode") before being reimplemented as a geq inverse-sample.
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.5,
        },
    ];

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_ken_burns_zoom.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}

#[test]
fn animated_rotation_keyframes_export_without_error() {
    let asset = video_asset(1);
    let mut c1 = clip(1, 1, 0.0, 0.0, 0.5);
    c1.rotation_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 30.0,
        },
    ];

    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![c1],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![track]);

    let output = std::env::temp_dir().join("avcore_test_timeline_export_rotation.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = render_timeline_export(
        &sequence,
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, RenderOutcome::Completed);

    let info = probe_media(&output).unwrap();
    assert_eq!(info.kind, MediaKind::Video);

    let _ = std::fs::remove_file(&output);
}
