// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! A project file saved today must keep loading tomorrow. The `.ocproj` layout is positional, so
//! adding, removing or reordering a field of anything inside `Project` silently changes what the
//! bytes mean. This test pins the bytes of one project that touches every kind of content: when it
//! fails, either the change is a mistake or `PROJECT_FORMAT_VERSION` must be bumped (with a way to
//! read the old layout, as `upgrade_v3` does) before the fixture is regenerated.
//!
//! Regenerate: `OCA_UPDATE_GOLDEN=1 cargo test -p core --test it golden_project`.

use std::path::PathBuf;

use avcore::persistence::{from_project_bytes, to_project_bytes};
use avcore::timeline::ClipInstance;
use avcore::timeline::{
    AudioRole, BlendMode, ColorFilter, MarkerKind, MaskShape, ShapeClip, TextClip, Timeline, Track,
    TrackKind, TransitionType,
};
use avcore::{Keyframe, MediaAsset, MediaKind, Position, Project, Recency, Sequence};

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden.ocproj")
}

fn project() -> Project {
    let mut styled = ClipInstance::new(1, 1, 0.0, 1.0, 9.0);
    styled.gain_db = -3.0;
    styled.speed_factor = 1.5;
    styled.speed_ramp_end_factor = Some(2.0);
    styled.mask_shape = MaskShape::Circle;
    styled.color_filter = ColorFilter::Sepia;
    styled.blend_mode = BlendMode::Screen;
    styled.transition_in = TransitionType::Dissolve;
    styled.fade_in_secs = 0.5;
    styled.fade_out_secs = 0.25;
    styled.lut_path = "grade.cube".into();
    styled.composite_id = Some(7);
    styled.position_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: Position { x: 0.25, y: -0.5 },
    }];
    let plain = ClipInstance::new(2, 2, 8.0, 0.0, 4.0);

    let track = |id, name: &str, kind| Track {
        id,
        name: name.to_string(),
        kind,
        clips: vec![],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let mut video = track(1, "V1", TrackKind::Video);
    video.clips = vec![styled, plain];
    let mut audio = track(2, "A1", TrackKind::Audio);
    audio.clips = vec![ClipInstance::new(3, 3, 0.0, 0.0, 12.0)];
    let mut text = track(3, "T1", TrackKind::Text);
    text.text_clips = vec![TextClip::new(4, 1.0, "golden".to_string())];
    let mut shape = track(4, "S1", TrackKind::Shape);
    shape.shape_clips = vec![ShapeClip::new(5, 2.0)];

    let mut timeline = Timeline {
        tracks: vec![video, audio, text, shape],
        playhead_secs: 3.5,
        markers: vec![],
        multicam_groups: vec![],
    };
    timeline.add_marker(4.0, MarkerKind::Chapter);

    let asset = |id, name: &str, kind| MediaAsset {
        id,
        file_name: name.to_string(),
        source_path: PathBuf::from(name),
        kind,
        has_audio: true,
        duration_secs: 60.0,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: Some((1920, 1080)),
        fps: Some(60.0),
        sample_rate_khz: Some(48.0),
        loudness: None,
        proxy_path: None,
        waveform_peaks: Some(vec![(-0.5, 0.5), (-0.25, 0.75)]),
        favorited: true,
    };
    Project {
        id: 9,
        name: "Golden".to_string(),
        last_edited: Recency::DaysAgo(2),
        summary: "pinned layout".to_string(),
        media_library: vec![
            asset(1, "a.mp4", MediaKind::Video),
            asset(2, "b.mp4", MediaKind::Video),
            asset(3, "c.wav", MediaKind::Audio),
        ],
        sequences: vec![Sequence {
            id: 1,
            name: "Main".to_string(),
            timeline,
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: vec![],
        recent_asset_ids: vec![3, 1],
        frame_rate: None,
    }
}

/// Rewrites the pinned file once per run when `OCA_UPDATE_GOLDEN` is set, before either test reads it.
fn regenerate_if_asked() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        if std::env::var_os("OCA_UPDATE_GOLDEN").is_some() {
            let path = golden_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, to_project_bytes(&project()).unwrap()).unwrap();
        }
    });
}

#[test]
fn the_pinned_project_file_still_loads_to_the_same_project() {
    regenerate_if_asked();
    let path = golden_path();
    let bytes = std::fs::read(&path).expect("tests/fixtures/golden.ocproj");
    let loaded = from_project_bytes(&bytes).expect(
        "the pinned file no longer loads: the project layout changed without a version bump",
    );
    assert_eq!(
        loaded,
        project(),
        "the pinned file loads to a different project: the layout changed"
    );
}

#[test]
fn saving_the_pinned_project_still_writes_the_pinned_bytes() {
    regenerate_if_asked();
    let bytes = std::fs::read(golden_path()).expect("tests/fixtures/golden.ocproj");
    assert_eq!(
        to_project_bytes(&project()).unwrap(),
        bytes,
        "the saved bytes changed: bump PROJECT_FORMAT_VERSION and keep reading the old layout, \
         then regenerate with OCA_UPDATE_GOLDEN=1"
    );
}
