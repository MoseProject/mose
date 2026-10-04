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

//! Multi-track compositing integration tests — `avbridge_encode_timeline_export_multi` had zero
//! Rust coverage before this file, even though its filter-string builders duplicate the same
//! geq-based Slide/Zoom-transition and Ken-Burns zoom fixes validated end-to-end for the
//! single-track path in `timeline_export_test.rs`. These exercise the real multi-track composite
//! path (`resolve_timeline_segments_multi` + `render_export_job_multi`), not just the C string
//! builders in isolation.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use avcore::project::Sequence;
use avcore::render::{
    render_export_job_multi, resolve_audio_segments, resolve_timeline_segments_multi, RenderOutcome,
};
use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, MaskShape, Timeline, Track, TrackKind,
    TransitionType,
};
use avcore::GpuEncoderPreference;
use avcore::{probe_media, MediaAsset};
use avcore::{Keyframe, Position};

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

fn audio_asset(id: u64) -> MediaAsset {
    let path = fixture("audio.m4a");
    probe_media(&path)
        .unwrap()
        .into_media_asset(id, "audio.m4a".to_string(), path)
}

fn track(id: u64, name: &str, clips: Vec<ClipInstance>) -> Track {
    Track {
        id,
        name: name.to_string(),
        kind: TrackKind::Video,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    }
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

fn render_multi(sequence: &Sequence, assets: &[MediaAsset], output_name: &str) -> RenderOutcome {
    let (track_segments, canvas) = resolve_timeline_segments_multi(sequence, assets).unwrap();
    let output = std::env::temp_dir().join(output_name);
    let cancel = AtomicBool::new(false);

    let outcome = render_export_job_multi(
        &track_segments,
        canvas,
        &output,
        -14.0,
        GpuEncoderPreference::Auto,
        &[],
        &[],
        &[],
        &cancel,
        |_| {},
    )
    .unwrap();

    let _ = std::fs::remove_file(&output);
    outcome
}

#[test]
fn two_video_tracks_composite_into_one_export() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let overlay = track(2, "V2", vec![clip(2, 1, 0.0, 0.0, 0.5)]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_composite_ok.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn three_video_tracks_composite_into_one_export() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let middle = track(2, "V2", vec![clip(2, 1, 0.0, 0.0, 0.5)]);
    let top = track(3, "V3", vec![clip(3, 1, 0.0, 0.0, 0.5)]);
    let sequence = sequence_with(vec![background, middle, top]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_three_layer_composite_ok.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn resolve_timeline_segments_multi_preserves_all_layer_order() {
    let asset = video_asset(1);
    let background = track(10, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut middle_clip = clip(2, 1, 0.0, 0.0, 0.5);
    middle_clip.layer_scale_x = 0.5;
    let middle = track(20, "V2", vec![middle_clip]);
    let mut top_clip = clip(3, 1, 0.0, 0.0, 0.5);
    top_clip.layer_scale_x = 0.25;
    let top = track(30, "V3", vec![top_clip]);
    let sequence = sequence_with(vec![background, middle, top]);

    let (track_segments, _canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();

    assert_eq!(track_segments.len(), 3);
    assert!(track_segments[0][0].video_filter.is_empty());
    assert!(track_segments[1][0]
        .video_filter
        .contains("scale=iw*0.5000"));
    assert!(track_segments[2][0]
        .video_filter
        .contains("scale=iw*0.2500"));
}

#[test]
fn resolve_timeline_segments_multi_skips_hidden_tracks() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut hidden_overlay = track(2, "V2", vec![clip(2, 1, 0.0, 0.0, 0.5)]);
    hidden_overlay.visible = false;
    let sequence = sequence_with(vec![background, hidden_overlay]);

    let (track_segments, _canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();

    assert_eq!(track_segments.len(), 1);
}

#[test]
fn resolve_audio_segments_includes_background_and_additional_audio_tracks() {
    let video = video_asset(1);
    let audio = audio_asset(2);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut audio_clip = clip(2, 2, 0.25, 0.1, 0.6);
    audio_clip.gain_db = -5.0;
    audio_clip.speed_factor = 1.25;
    let audio_track = Track {
        id: 2,
        name: "A1".to_string(),
        kind: TrackKind::Audio,
        clips: vec![audio_clip],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![background, audio_track]);

    let segments = resolve_audio_segments(&sequence, &[video, audio]).unwrap();

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].timeline_start_secs, 0.0);
    assert_eq!(segments[1].timeline_start_secs, 0.25);
    assert_eq!(segments[1].gain_db, -5.0);
    assert_eq!(segments[1].speed_factor, 1.25);
}

/// P2 item 6, "Audio ducking": `Track::audio_role` must reach each resolved `AudioSegment` as
/// the raw `duck_role` code `avbridge`'s `build_mix_graph` switches on (0/Normal, 1/Mic-trigger,
/// 2/Music-target) -- see `AudioRole::to_duck_role_code`.
#[test]
fn resolve_audio_segments_carries_the_track_audio_role_as_duck_role() {
    let video = video_asset(1);
    let mic = audio_asset(2);
    let music = audio_asset(3);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mic_track = Track {
        id: 2,
        name: "A1".to_string(),
        kind: TrackKind::Audio,
        clips: vec![clip(2, 2, 0.0, 0.0, 0.5)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Mic,
        locked: false,
        color_label: None,
    };
    let music_track = Track {
        id: 3,
        name: "A2".to_string(),
        kind: TrackKind::Audio,
        clips: vec![clip(3, 3, 0.0, 0.0, 0.5)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Music,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![background, mic_track, music_track]);

    let segments = resolve_audio_segments(&sequence, &[video, mic, music]).unwrap();

    assert_eq!(segments.len(), 3);
    assert_eq!(segments[0].duck_role, 0); // background video track, Unspecified
    assert_eq!(segments[1].duck_role, 1); // Mic -> trigger
    assert_eq!(segments[2].duck_role, 2); // Music -> target
}

#[test]
fn resolve_audio_segments_is_empty_without_an_additional_contributor() {
    let video = video_asset(1);
    let sequence = sequence_with(vec![track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)])]);

    assert!(resolve_audio_segments(&sequence, &[video])
        .unwrap()
        .is_empty());
}

#[test]
fn resolve_audio_segments_ignores_hidden_audio_tracks() {
    let video = video_asset(1);
    let audio = audio_asset(2);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let hidden_audio = Track {
        id: 2,
        name: "A1".to_string(),
        kind: TrackKind::Audio,
        clips: vec![clip(2, 2, 0.0, 0.0, 0.5)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: false,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![background, hidden_audio]);

    assert!(resolve_audio_segments(&sequence, &[video, audio])
        .unwrap()
        .is_empty());
}

#[test]
fn resolve_audio_segments_ignores_video_assets_without_audio() {
    let background_asset = video_asset(1);
    let mut silent_overlay_asset = video_asset(2);
    silent_overlay_asset.has_audio = false;
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let overlay = track(2, "V2", vec![clip(2, 2, 0.0, 0.0, 0.5)]);
    let sequence = sequence_with(vec![background, overlay]);

    assert!(
        resolve_audio_segments(&sequence, &[background_asset, silent_overlay_asset])
            .unwrap()
            .is_empty()
    );
}

#[test]
fn layer_scale_is_appended_to_the_overlay_tracks_video_filter() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.layer_scale_x = 0.5;
    c2.layer_scale_y = 0.25;
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let (track_segments, _canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();

    assert!(!track_segments[0][0].video_filter.contains("scale=iw*"));
    assert!(track_segments[1][0]
        .video_filter
        .contains("scale=iw*0.5000:ih*0.2500"));
}

#[test]
fn layer_scale_at_native_size_adds_no_filter_stage() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let overlay = track(2, "V2", vec![clip(2, 1, 0.0, 0.0, 0.5)]);
    let sequence = sequence_with(vec![background, overlay]);

    let (track_segments, _canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();

    assert!(track_segments[1][0].video_filter.is_empty());
}

#[test]
fn overlay_track_slide_transition_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.transition_in = TransitionType::Slide;
    c2.transition_duration_secs = 0.3;
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_slide.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_zoom_transition_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.transition_in = TransitionType::Zoom;
    c2.transition_duration_secs = 0.3;
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_zoom.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_animated_scale_keyframes_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.5,
        },
    ];
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_ken_burns.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_animated_position_keyframes_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.position_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: Position { x: 0.0, y: 0.0 },
        },
        Keyframe {
            time_fraction: 1.0,
            value: Position { x: 0.25, y: 0.1 },
        },
    ];
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_position.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_animated_opacity_keyframes_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.opacity_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.2,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_opacity.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_animated_rotation_keyframes_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.rotation_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 45.0,
        },
    ];
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_rotation.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_mask_shape_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.mask_shape = MaskShape::Circle;
    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_mask.mp4",
    );

    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn overlay_track_background_removal_composites_without_error() {
    let asset = video_asset(1);
    let background = track(1, "V1", vec![clip(1, 1, 0.0, 0.0, 0.5)]);
    let mut c2 = clip(2, 1, 0.0, 0.0, 0.5);
    c2.background_removal_enabled = true;

    // A small synthetic matte covering the overlay clip's 0.5s trim: 8 frames at 16fps, an
    // arbitrary size independent of the source's own resolution — build_overlay_vfilter scales
    // both the overlay clip and the matte to canvas dimensions independently before alphamerge,
    // so the matte's own native size doesn't need to match anything.
    let matte_dir = std::env::temp_dir().join("avcore_test_multi_overlay_matte_dir");
    let _ = std::fs::create_dir_all(&matte_dir);
    let matte_path = matte_dir.join("clip_2_matte.mp4");
    let frames: Vec<Vec<u8>> = (0..8).map(|i| vec![(i * 30) as u8; 64 * 64]).collect();
    avcore::encode_matte_video(&frames, 64, 64, 16, 1, &matte_path).unwrap();
    c2.background_removal_mask_path = matte_path.to_string_lossy().to_string();

    let overlay = track(2, "V2", vec![c2]);
    let sequence = sequence_with(vec![background, overlay]);

    let outcome = render_multi(
        &sequence,
        std::slice::from_ref(&asset),
        "avcore_test_multi_overlay_background_removal.mp4",
    );

    let _ = std::fs::remove_dir_all(&matte_dir);
    assert_eq!(outcome, RenderOutcome::Completed);
}

#[test]
fn cancelling_mid_multi_track_export_reports_cancelled() {
    let asset = video_asset(1);
    let background = track(
        1,
        "V1",
        vec![clip(1, 1, 0.0, 0.0, 0.5), clip(3, 1, 0.5, 0.0, 1.0)],
    );
    let overlay = track(
        2,
        "V2",
        vec![clip(2, 1, 0.0, 0.0, 0.5), clip(4, 1, 0.5, 0.0, 1.0)],
    );
    let sequence = sequence_with(vec![background, overlay]);

    let (track_segments, canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();
    let output = std::env::temp_dir().join("avcore_test_multi_cancelled.mp4");
    let cancel = AtomicBool::new(false);
    let mut calls = 0;

    let outcome = render_export_job_multi(
        &track_segments,
        canvas,
        &output,
        -14.0,
        GpuEncoderPreference::Auto,
        &[],
        &[],
        &[],
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
