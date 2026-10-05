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

use super::*;
use crate::timeline::{
    BlendMode, ColorFilter, MarkerKind, MaskShape, ShapeClip, ShapeKind, TextClip, TrackKind,
    TransitionType,
};
use crate::AudioRole;

fn clip(
    id: u64,
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
) -> crate::timeline::ClipInstance {
    crate::timeline::ClipInstance {
        id,
        asset_id: 1,
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

fn video_track(clips: Vec<crate::timeline::ClipInstance>) -> Track {
    Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    }
}

fn text_clip(id: u64, start_secs: f64, duration_secs: f64) -> TextClip {
    TextClip {
        id,
        start_secs,
        duration_secs,
        text: "hi".to_string(),
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
        words: vec![],
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

fn timeline_with_tracks(tracks: Vec<Track>) -> Timeline {
    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: vec![crate::timeline::Marker {
            id: 1,
            position_secs: 5.0,
            label: "should not carry over".to_string(),
            kind: MarkerKind::Standard,
            completed: false,
        }],
        multicam_groups: Vec::new(),
    }
}

#[test]
fn extracts_a_clip_fully_inside_the_window_and_rebases_it() {
    let timeline = timeline_with_tracks(vec![video_track(vec![clip(1, 10.0, 0.0, 20.0)])]);
    let mut next_id = 2;

    let windowed = extract_timeline_window(&timeline, 5.0, 30.0, &mut next_id);

    assert_eq!(windowed.tracks[0].clips.len(), 1);
    assert_eq!(
        windowed.tracks[0].clips[0].start_secs, 5.0,
        "10.0 - window_start (5.0)"
    );
    assert_eq!(
        next_id, 2,
        "no boundary landed inside the clip, no split needed"
    );
}

#[test]
fn splits_a_clip_straddling_the_window_start() {
    // One 20s clip [0, 20); window [10, 30) straddles it at 10.
    let timeline = timeline_with_tracks(vec![video_track(vec![clip(1, 0.0, 0.0, 20.0)])]);
    let mut next_id = 2;

    let windowed = extract_timeline_window(&timeline, 10.0, 30.0, &mut next_id);

    assert_eq!(
        next_id, 3,
        "both a start-boundary split and an end-boundary no-op attempt"
    );
    assert_eq!(windowed.tracks[0].clips.len(), 1);
    let remaining = &windowed.tracks[0].clips[0];
    assert_eq!(
        remaining.start_secs, 0.0,
        "rebased: 10.0 - window_start (10.0)"
    );
    assert_eq!(
        remaining.source_in_secs, 10.0,
        "kept only the source range past the split"
    );
    assert_eq!(remaining.source_out_secs, 20.0);
}

#[test]
fn splits_a_clip_straddling_the_window_end() {
    // One 20s clip [0, 20); window [0, 10) straddles it at 10.
    let timeline = timeline_with_tracks(vec![video_track(vec![clip(1, 0.0, 0.0, 20.0)])]);
    let mut next_id = 2;

    let windowed = extract_timeline_window(&timeline, 0.0, 10.0, &mut next_id);

    assert_eq!(windowed.tracks[0].clips.len(), 1);
    let remaining = &windowed.tracks[0].clips[0];
    assert_eq!(remaining.start_secs, 0.0);
    assert_eq!(
        remaining.source_out_secs, 10.0,
        "trimmed to the window's own end"
    );
}

#[test]
fn drops_a_clip_entirely_outside_the_window() {
    let timeline = timeline_with_tracks(vec![video_track(vec![
        clip(1, 0.0, 0.0, 5.0),
        clip(2, 50.0, 0.0, 5.0),
    ])]);
    let mut next_id = 3;

    let windowed = extract_timeline_window(&timeline, 10.0, 20.0, &mut next_id);

    assert!(windowed.tracks[0].clips.is_empty());
}

#[test]
fn drops_a_text_clip_straddling_a_boundary_rather_than_trimming_word_timing() {
    let mut track = video_track(vec![]);
    track.kind = TrackKind::Text;
    track.text_clips = vec![
        text_clip(1, 8.0, 4.0),  // [8, 12) straddles window start at 10 -- dropped
        text_clip(2, 12.0, 3.0), // [12, 15) fully inside [10, 20) -- kept
    ];
    let timeline = timeline_with_tracks(vec![track]);
    let mut next_id = 3;

    let windowed = extract_timeline_window(&timeline, 10.0, 20.0, &mut next_id);

    assert_eq!(windowed.tracks[0].text_clips.len(), 1);
    assert_eq!(windowed.tracks[0].text_clips[0].id, 2);
    assert_eq!(
        windowed.tracks[0].text_clips[0].start_secs, 2.0,
        "12.0 - window_start (10.0)"
    );
}

#[test]
fn extracts_a_shape_clip_fully_inside_the_window_and_rebases_it() {
    let mut track = video_track(vec![]);
    track.kind = TrackKind::Shape;
    track.shape_clips = vec![shape_clip(1, 12.0, 3.0)];
    let timeline = timeline_with_tracks(vec![track]);
    let mut next_id = 2;

    let windowed = extract_timeline_window(&timeline, 10.0, 20.0, &mut next_id);

    assert_eq!(windowed.tracks[0].shape_clips.len(), 1);
    assert_eq!(windowed.tracks[0].shape_clips[0].start_secs, 2.0);
}

#[test]
fn drops_markers_and_resets_the_playhead() {
    let timeline = timeline_with_tracks(vec![video_track(vec![clip(1, 0.0, 0.0, 5.0)])]);
    let mut next_id = 2;

    let windowed = extract_timeline_window(&timeline, 0.0, 5.0, &mut next_id);

    assert!(windowed.markers.is_empty());
    assert_eq!(windowed.playhead_secs, 0.0);
}

#[test]
fn empty_or_inverted_window_yields_no_tracks() {
    let timeline = timeline_with_tracks(vec![video_track(vec![clip(1, 0.0, 0.0, 5.0)])]);
    let mut next_id = 2;

    assert!(extract_timeline_window(&timeline, 5.0, 5.0, &mut next_id)
        .tracks
        .is_empty());
    assert!(extract_timeline_window(&timeline, 8.0, 3.0, &mut next_id)
        .tracks
        .is_empty());
    assert_eq!(next_id, 2, "no-op consumes no ids");
}
