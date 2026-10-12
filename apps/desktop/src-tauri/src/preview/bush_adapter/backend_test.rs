// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The parts of the Bush playback backend that do not need a window or an audio device.

use std::path::PathBuf;
use std::sync::Mutex;

use avcore::timeline::{AudioRole, ClipInstance, Track};
use bush::{EngineConfig, SceneSnapshot, SceneVersions};

use super::*;
use crate::frames::PREVIEW_MAX_DIM;

/// `OCA_BUSH_*` are process-wide: the tests that set them take turns.
fn env_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn asset(id: u64, kind: MediaKind, resolution: Option<(u32, u32)>, fps: Option<f32>) -> MediaAsset {
    MediaAsset {
        id,
        file_name: format!("asset-{id}"),
        source_path: PathBuf::from(format!("asset-{id}.mp4")),
        kind,
        has_audio: true,
        duration_secs: 20.0,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution,
        fps,
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
        streams: Default::default(),
        hdr_handling: Default::default(),
        audio_stream: None,
    }
}

fn timeline_with(kind: TrackKind, visible: bool, clip_asset: u64) -> Timeline {
    Timeline {
        tracks: vec![Track {
            id: 1,
            name: "T1".into(),
            kind,
            clips: vec![ClipInstance::new(1, clip_asset, 0.0, 0.0, 4.0)],
            text_clips: Vec::new(),
            shape_clips: Vec::new(),
            visible,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            color_label: None,
            locked: false,
        }],
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

#[test]
fn a_smaller_preview_size_shrinks_the_canvas_with_the_aspect_kept() {
    let media = [asset(1, MediaKind::Video, Some((1920, 1080)), Some(30.0))];
    let (width, height) = preview_canvas(&timeline_with(TrackKind::Video, true, 1), &media, 640);
    assert_eq!((width, height), (640, 360));
}

#[test]
fn the_preview_canvas_follows_the_first_video_clip_scaled_to_the_preview_size() {
    let media = [asset(1, MediaKind::Video, Some((3840, 2160)), Some(30.0))];
    let (width, height) = preview_canvas(
        &timeline_with(TrackKind::Video, true, 1),
        &media,
        PREVIEW_MAX_DIM,
    );
    assert_eq!(width, PREVIEW_MAX_DIM);
    assert_eq!(height, 720);
    assert!(width % 2 == 0 && height % 2 == 0, "video sizes stay even");

    // A source smaller than the preview is never enlarged.
    let small = [asset(1, MediaKind::Video, Some((640, 360)), None)];
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, true, 1),
            &small,
            PREVIEW_MAX_DIM
        ),
        (640, 360)
    );
}

#[test]
fn without_a_usable_video_clip_the_preview_uses_the_default_canvas() {
    let video = [asset(1, MediaKind::Video, Some((1920, 1080)), None)];
    // No timeline at all, a hidden track, an audio track, or an audio-only asset.
    let empty = Timeline {
        tracks: Vec::new(),
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    assert_eq!(
        preview_canvas(&empty, &video, PREVIEW_MAX_DIM),
        DEFAULT_CANVAS
    );
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, false, 1),
            &video,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS
    );
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Audio, true, 1),
            &video,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS
    );
    let audio_asset = [asset(1, MediaKind::Audio, Some((1920, 1080)), None)];
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, true, 1),
            &audio_asset,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS
    );
    let unknown_size = [asset(1, MediaKind::Video, None, None)];
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, true, 1),
            &unknown_size,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS
    );
    let zero = [asset(1, MediaKind::Video, Some((0, 1080)), None)];
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, true, 1),
            &zero,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS
    );
    assert_eq!(
        preview_canvas(
            &timeline_with(TrackKind::Video, true, 99),
            &video,
            PREVIEW_MAX_DIM
        ),
        DEFAULT_CANVAS,
        "a clip pointing at a missing asset"
    );
}

#[test]
fn the_playback_rate_is_the_first_video_clips_frame_rate() {
    let timeline = timeline_with(TrackKind::Video, true, 1);
    assert_eq!(
        first_video_fps(&timeline, &[asset(1, MediaKind::Video, None, Some(59.94))]),
        f64::from(59.94_f32)
    );
    for media in [
        vec![asset(1, MediaKind::Video, None, None)],
        vec![asset(1, MediaKind::Video, None, Some(0.5))],
        Vec::new(),
    ] {
        assert_eq!(first_video_fps(&timeline, &media), FALLBACK_FPS);
    }
    assert_eq!(
        first_video_fps(
            &timeline_with(TrackKind::Video, false, 1),
            &[asset(1, MediaKind::Video, None, Some(24.0))]
        ),
        FALLBACK_FPS
    );
}

#[test]
fn environment_switches_select_the_renderer_and_the_decode_policy() {
    let _turn = env_lock();
    std::env::remove_var("OCA_BUSH_RENDERER");
    std::env::remove_var("OCA_BUSH_DECODE");
    assert!(matches!(renderer_choice(), RendererChoice::Cpu));
    assert!(!hardware_decode_requested());

    std::env::set_var("OCA_BUSH_RENDERER", "gpu");
    std::env::set_var("OCA_BUSH_DECODE", "hardware");
    assert!(matches!(
        renderer_choice(),
        RendererChoice::GpuWithCpuFallback
    ));
    assert!(hardware_decode_requested());

    std::env::set_var("OCA_BUSH_RENDERER", "something-else");
    std::env::set_var("OCA_BUSH_DECODE", "software");
    assert!(matches!(renderer_choice(), RendererChoice::Cpu));
    assert!(!hardware_decode_requested());
    std::env::remove_var("OCA_BUSH_RENDERER");
    std::env::remove_var("OCA_BUSH_DECODE");
}

#[test]
fn an_engine_starts_for_a_sane_canvas_and_not_for_an_empty_one() {
    let _turn = env_lock();
    assert!(new_engine((320, 180)).is_some());
    assert!(new_engine((0, 0)).is_none());
}

#[test]
fn the_resolver_parses_each_lut_once_and_ignores_unreadable_ones() {
    let dir = std::env::temp_dir().join(format!("oca_desktop_lut_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cube = dir.join("identity.cube");
    let mut text = String::from("LUT_3D_SIZE 2\n");
    for blue in 0..2 {
        for green in 0..2 {
            for red in 0..2 {
                text.push_str(&format!("{red} {green} {blue}\n"));
            }
        }
    }
    std::fs::write(&cube, text).unwrap();

    let mut resolver = AppResolver::default();
    let first = resolver.lut(cube.to_str().unwrap()).expect("a valid LUT");
    assert_eq!(first.size(), 2);
    let again = resolver.lut(cube.to_str().unwrap()).unwrap();
    assert!(Arc::ptr_eq(&first, &again), "the second lookup is cached");
    assert!(resolver
        .lut(dir.join("missing.cube").to_str().unwrap())
        .is_none());

    // A table that cannot be parsed is ignored too.
    let broken = dir.join("broken.cube");
    std::fs::write(&broken, "LUT_3D_SIZE 2\n0 0 0\n").unwrap();
    assert!(resolver.lut(broken.to_str().unwrap()).is_none());

    // Source paths come from the preview helper: only files that exist are offered.
    assert!(resolver
        .source_path(&asset(1, MediaKind::Video, None, None))
        .is_none());
    let mut present = asset(2, MediaKind::Video, None, None);
    present.source_path = cube.clone();
    assert_eq!(resolver.source_path(&present), Some(cube));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_jpeg_encoder_publishes_only_the_newest_queued_frame() {
    let latest: LatestFrame = Arc::new(Mutex::new(None));
    let encoder = spawn_encoder(Arc::clone(&latest));
    let frame = |value: u8| {
        bush::VideoFrame::new(
            bush::PixelFormat::Rgba8,
            16,
            16,
            64,
            0.0,
            vec![value; 16 * 16 * 4],
        )
        .unwrap()
    };
    encoder.tx.send(frame(10)).unwrap();
    encoder.tx.send(frame(200)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while encoder.shared.encoded_seq.load(Ordering::Acquire) == 0 {
        assert!(Instant::now() < deadline, "no frame was encoded");
        std::thread::sleep(Duration::from_millis(5));
    }
    let jpeg = latest.lock().unwrap().clone().expect("a published frame");
    assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    assert!(encoder.shared.frames.load(Ordering::Relaxed) >= 1);
    assert!(encoder.shared.micros_max.load(Ordering::Relaxed) > 0);
    assert!(encoder.shared.micros_total.load(Ordering::Relaxed) > 0);
}

fn engine() -> bush::Engine {
    bush::Engine::new(EngineConfig {
        canvas_width: 64,
        canvas_height: 64,
        max_open_sources: 1,
        frame_cache_bytes: 1 << 20,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap()
}

#[test]
fn playback_stats_report_once_per_window_and_reset_the_counters() {
    let engine = engine();
    let shared = EncoderShared::default();
    shared.frames.store(30, Ordering::Relaxed);
    shared.micros_total.store(60_000, Ordering::Relaxed);
    shared.micros_max.store(4_000, Ordering::Relaxed);

    let mut stats = PlaybackStats::new();
    // Inside the window nothing is reported or reset.
    stats.report(&engine, &shared, Some(1.5), Some(0));
    assert_eq!(shared.frames.load(Ordering::Relaxed), 30);

    stats.window_start = Instant::now() - STATS_WINDOW - Duration::from_secs(1);
    stats.report(&engine, &shared, Some(1.5), Some(0));
    assert_eq!(shared.frames.load(Ordering::Relaxed), 0);
    assert_eq!(shared.micros_total.load(Ordering::Relaxed), 0);
    assert_eq!(shared.micros_max.load(Ordering::Relaxed), 0);

    // An empty window reports zero encode time without dividing by zero.
    stats.window_start = Instant::now() - STATS_WINDOW - Duration::from_secs(1);
    stats.report(&engine, &shared, None, None);
}

#[test]
fn seeking_moves_the_player_and_reports_when_the_engine_is_gone() {
    let scene = SceneSnapshot::new(SceneVersions::default(), Vec::new()).unwrap();
    let mut player = Player::new(engine(), scene, 10.0).unwrap();
    let mut outstanding = None;
    seek(&mut player, None, 4.0, &mut outstanding);
    assert_eq!(player.position_secs(), 4.0);
    assert!(outstanding.is_some(), "a render is now in flight");

    // After the engine stops, a seek is a logged failure that leaves the state alone.
    player.engine().shutdown();
    let mut after_shutdown = None;
    seek(&mut player, None, 6.0, &mut after_shutdown);
    assert!(after_shutdown.is_none());
}

#[test]
fn the_engine_config_follows_the_decode_and_renderer_choices() {
    let software = engine_config((640, 360), false, RendererChoice::Cpu);
    assert!(matches!(software.decode_policy, DecodePolicy::Software));
    assert_eq!((software.canvas_width, software.canvas_height), (640, 360));
    let hardware = engine_config((640, 360), true, RendererChoice::GpuWithCpuFallback);
    assert!(matches!(
        hardware.decode_policy,
        DecodePolicy::HardwareWithFallback
    ));
    assert!(matches!(
        hardware.renderer,
        RendererChoice::GpuWithCpuFallback
    ));
}

fn project_with_clip() -> avcore::Project {
    let mut project = crate::state::blank_project("A".into(), "S".into());
    project
        .media_library
        .push(asset(1, MediaKind::Video, Some((1920, 1080)), Some(30.0)));
    project.timeline_mut().tracks.push(Track {
        id: 1,
        name: "V1".into(),
        kind: avcore::timeline::TrackKind::Video,
        clips: vec![ClipInstance::new(1, 1, 0.0, 0.0, 5.0)],
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    });
    project
}

#[test]
fn edits_the_preview_does_not_show_leave_the_scene_alone() {
    let before = project_with_clip();
    let mut after = before.clone();
    after.name = "Renamed".into();
    after.summary = "notes".into();
    after.timeline_mut().playhead_secs = 3.0;
    after.active_sequence_mut().export_settings.target_lufs = -23.0;
    after.recent_asset_ids = vec![1];
    assert!(same_preview_content(&before, &after));
}

#[test]
fn edits_the_preview_shows_rebuild_the_scene() {
    let before = project_with_clip();
    let mut moved = before.clone();
    moved.timeline_mut().tracks[0].clips[0].start_secs = 1.0;
    assert!(!same_preview_content(&before, &moved));
    let mut muted = before.clone();
    muted.timeline_mut().tracks[0].visible = false;
    assert!(!same_preview_content(&before, &muted));
    let mut media = before.clone();
    media.media_library[0].proxy_path = Some(PathBuf::from("proxy.mp4"));
    assert!(!same_preview_content(&before, &media));
    let mut sequences = before.clone();
    let copy = sequences.sequences[0].clone();
    sequences.sequences.push(copy);
    assert!(!same_preview_content(&before, &sequences));
}
