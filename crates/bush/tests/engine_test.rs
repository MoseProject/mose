use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use bush::{
    BlendMode, Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
    EngineConfig, EngineError, EngineEvent, EngineRequest, MediaSource, PixelFormat, Placement,
    RasterOverlay, SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};

const WAIT: Duration = Duration::from_secs(20);

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn dimensions() -> (u32, u32) {
    let info = DecodeSource::open(&fixture()).unwrap().info();
    (info.width, info.height)
}

fn config(max_open_sources: usize) -> EngineConfig {
    let (width, height) = dimensions();
    EngineConfig {
        canvas_width: width,
        canvas_height: height,
        max_open_sources,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
    }
}

fn clip(id: u64, layer: i32, path: PathBuf, placement: Placement) -> Contributor {
    Contributor {
        id: ContributorId(id),
        layer,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path,
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement,
    }
}

fn scene(contributors: Vec<Contributor>) -> SceneSnapshot {
    SceneSnapshot::new(SceneVersions::default(), contributors).unwrap()
}

fn request(scene: SceneSnapshot, playhead_secs: f64) -> EngineRequest {
    EngineRequest {
        scene,
        playhead_secs,
        overlays: Vec::new(),
    }
}

fn expect_frame(engine: &Engine) -> (bush::Generation, VideoFrame) {
    match engine.wait(WAIT).expect("engine produced no event") {
        EngineEvent::Frame { generation, frame } => (generation, frame),
        EngineEvent::Failed { message, .. } => panic!("engine failed: {message}"),
    }
}

fn reference_frame(target_secs: f64) -> Arc<VideoFrame> {
    DecodeSource::open(&fixture())
        .unwrap()
        .frame_at(target_secs, &AtomicBool::new(false))
        .unwrap()
        .unwrap()
}

#[test]
fn single_clip_renders_the_decoded_frame_at_the_playhead() {
    let engine = Engine::new(config(2)).unwrap();
    let generation = engine
        .submit(request(
            scene(vec![clip(1, 0, fixture(), Placement::default())]),
            0.5,
        ))
        .unwrap();

    let (event_generation, frame) = expect_frame(&engine);

    assert_eq!(event_generation, generation);
    assert_eq!(frame.timestamp_secs(), 0.5);
    assert_eq!(frame.pixels(), reference_frame(0.5).pixels());
}

#[test]
fn clip_outside_its_window_renders_a_transparent_canvas() {
    let engine = Engine::new(config(1)).unwrap();
    let placement = Placement {
        timeline_start_secs: 10.0,
        ..Placement::default()
    };
    engine
        .submit(request(scene(vec![clip(1, 0, fixture(), placement)]), 1.0))
        .unwrap();

    let (_, frame) = expect_frame(&engine);
    assert!(frame.pixels().iter().all(|byte| *byte == 0));
}

#[test]
fn speed_and_timeline_start_map_playhead_to_source_time() {
    let engine = Engine::new(config(1)).unwrap();
    let placement = Placement {
        timeline_start_secs: 2.0,
        speed: 2.0,
        ..Placement::default()
    };
    // Playhead 2.2 -> source time (2.2 - 2.0) * 2 = 0.4.
    engine
        .submit(request(scene(vec![clip(1, 0, fixture(), placement)]), 2.2))
        .unwrap();
    let (_, frame) = expect_frame(&engine);

    assert_eq!(frame.pixels(), reference_frame(0.4).pixels());
}

#[test]
fn raster_overlay_composites_above_video_in_layer_order() {
    let (width, height) = dimensions();
    let engine = Engine::new(config(1)).unwrap();
    let overlay_frame = Arc::new(
        VideoFrame::new(
            PixelFormat::Rgba8,
            width,
            height,
            width as usize * 4,
            0.0,
            [255, 0, 255, 255].repeat((width * height) as usize),
        )
        .unwrap(),
    );
    let overlay = Contributor {
        id: ContributorId(2),
        layer: 5,
        visible: true,
        audible: false,
        content: ContributorContent::RasterOverlay(RasterOverlay {
            width,
            height,
            version: Version(1),
        }),
        placement: Placement::default(),
    };
    engine
        .submit(EngineRequest {
            scene: scene(vec![overlay, clip(1, 0, fixture(), Placement::default())]),
            playhead_secs: 0.3,
            overlays: vec![(ContributorId(2), overlay_frame)],
        })
        .unwrap();

    let (_, frame) = expect_frame(&engine);
    assert!(frame
        .pixels()
        .chunks_exact(4)
        .all(|pixel| pixel == [255, 0, 255, 255]));
}

#[test]
fn burst_of_submits_publishes_only_the_newest_generation() {
    let engine = Engine::new(config(1)).unwrap();
    let mut last = bush::Generation(0);
    for step in 0..40 {
        last = engine
            .submit(request(
                scene(vec![clip(1, 0, fixture(), Placement::default())]),
                f64::from(step % 8) / 10.0,
            ))
            .unwrap();
    }

    let (generation, _) = expect_frame(&engine);
    assert_eq!(generation, last, "a stale generation was published");
    std::thread::sleep(Duration::from_millis(100));
    assert!(engine.poll().is_none(), "a second event was published");
    let stats = engine.stats();
    assert_eq!(stats.submitted, 40);
    assert!(stats.superseded + stats.cancelled > 0);
}

#[test]
fn unchanged_source_frame_is_served_from_the_cache_on_a_live_edit() {
    let engine = Engine::new(config(1)).unwrap();
    let visible = scene(vec![clip(1, 0, fixture(), Placement::default())]);
    engine.submit(request(visible, 0.4)).unwrap();
    expect_frame(&engine);

    let faded = scene(vec![clip(
        1,
        0,
        fixture(),
        Placement {
            opacity: 0.5,
            blend: BlendMode::Multiply,
            ..Placement::default()
        },
    )]);
    engine.submit(request(faded, 0.4)).unwrap();
    let (_, frame) = expect_frame(&engine);

    let stats = engine.stats();
    assert_eq!(stats.cache_misses, 1);
    assert_eq!(stats.cache_hits, 1);
    assert_eq!(frame.width(), dimensions().0);
}

#[test]
fn unreadable_media_fails_with_a_redacted_message() {
    let engine = Engine::new(config(1)).unwrap();
    let garbage =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/garbage.bin");
    engine
        .submit(request(
            scene(vec![clip(1, 0, garbage, Placement::default())]),
            0.1,
        ))
        .unwrap();

    match engine.wait(WAIT).unwrap() {
        EngineEvent::Failed { message, .. } => {
            assert!(message.contains("garbage.bin"));
            assert!(!message.contains("fixtures"), "path leaked: {message}");
        }
        other => panic!("expected failure, got {other:?}"),
    }
    assert_eq!(engine.stats().failures, 1);
}

#[test]
fn engine_recovers_after_a_failed_request() {
    let engine = Engine::new(config(1)).unwrap();
    engine
        .submit(request(
            scene(vec![clip(
                1,
                0,
                PathBuf::from("missing.mp4"),
                Placement::default(),
            )]),
            0.1,
        ))
        .unwrap();
    assert!(matches!(
        engine.wait(WAIT).unwrap(),
        EngineEvent::Failed { .. }
    ));

    engine
        .submit(request(
            scene(vec![clip(1, 0, fixture(), Placement::default())]),
            0.1,
        ))
        .unwrap();
    expect_frame(&engine);
}

#[test]
fn repeated_non_linear_scrubs_stay_frame_accurate() {
    let engine = Engine::new(config(1)).unwrap();
    let timeline = scene(vec![clip(1, 0, fixture(), Placement::default())]);
    for playhead in [0.1, 0.6, 0.2, 0.8, 0.0] {
        engine.submit(request(timeline.clone(), playhead)).unwrap();
        let (_, frame) = expect_frame(&engine);
        assert_eq!(frame.timestamp_secs(), playhead);
        assert_eq!(frame.pixels(), reference_frame(playhead).pixels());
    }
}

#[test]
fn rejects_invalid_configuration_and_playhead() {
    let mut bad = config(1);
    bad.canvas_width = 0;
    assert_eq!(Engine::new(bad).unwrap_err(), EngineError::InvalidConfig);
    assert_eq!(
        Engine::new(config(0)).unwrap_err(),
        EngineError::InvalidConfig
    );
    let mut no_cache = config(1);
    no_cache.frame_cache_bytes = 0;
    assert_eq!(
        Engine::new(no_cache).unwrap_err(),
        EngineError::InvalidConfig
    );

    let engine = Engine::new(config(1)).unwrap();
    assert_eq!(
        engine.submit(request(scene(vec![]), f64::NAN)).unwrap_err(),
        EngineError::InvalidPlayhead
    );
}

#[test]
fn shutdown_is_idempotent_and_rejects_later_submits() {
    let engine = Engine::new(config(1)).unwrap();
    engine.shutdown();
    engine.shutdown();
    assert_eq!(
        engine.submit(request(scene(vec![]), 0.0)).unwrap_err(),
        EngineError::Stopped
    );
    assert!(engine.wait(Duration::from_millis(10)).is_none());
}

fn h264() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video_h264.mp4")
}

fn h264_frame(policy: DecodePolicy, playhead: f64) -> VideoFrame {
    let engine = Engine::new(EngineConfig {
        canvas_width: 640,
        canvas_height: 360,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: policy,
    })
    .unwrap();
    engine
        .submit(request(
            scene(vec![clip(1, 0, h264(), Placement::default())]),
            playhead,
        ))
        .unwrap();
    expect_frame(&engine).1
}

#[test]
fn hardware_decode_policy_renders_the_same_picture_as_software() {
    for playhead in [0.0, 0.4, 0.84] {
        let software = h264_frame(DecodePolicy::Software, playhead);
        let hardware = h264_frame(DecodePolicy::HardwareWithFallback, playhead);
        let difference = software
            .pixels()
            .iter()
            .zip(hardware.pixels())
            .map(|(a, b)| f64::from(a.abs_diff(*b)))
            .sum::<f64>()
            / software.pixels().len() as f64;
        assert!(
            difference < 3.0,
            "mean difference {difference} at {playhead}s"
        );
    }
}

#[test]
fn fade_in_and_keyframed_position_are_applied_per_frame() {
    let engine = Engine::new(config(1)).unwrap();
    let (width, _) = dimensions();
    let placement = Placement {
        fade: bush::Fade {
            in_secs: 1.0,
            out_secs: 0.0,
        },
        animation: bush::Animation {
            x: Some(
                bush::KeyframeTrack::new(vec![
                    bush::Keyframe {
                        time_secs: 0.0,
                        value: 0.0,
                        easing: bush::Easing::Linear,
                    },
                    bush::Keyframe {
                        time_secs: 1.0,
                        value: width as f32,
                        easing: bush::Easing::Linear,
                    },
                ])
                .unwrap(),
            ),
            ..bush::Animation::default()
        },
        ..Placement::default()
    };
    let timeline = scene(vec![clip(1, 0, fixture(), placement)]);

    // At the clip start: fully faded out, so the canvas is empty.
    engine.submit(request(timeline.clone(), 0.0)).unwrap();
    let (_, start) = expect_frame(&engine);
    assert!(start.pixels().chunks_exact(4).all(|p| p[3] == 0));

    // Halfway: half opacity and shifted right by half the width, so the left half is empty.
    engine.submit(request(timeline, 0.5)).unwrap();
    let (_, middle) = expect_frame(&engine);
    let row = width as usize * 4;
    let left_empty = middle.pixels()[..row / 2 - 8]
        .chunks_exact(4)
        .all(|p| p[3] == 0);
    let right_alpha = middle.pixels()[row - 8 + 3];
    assert!(left_empty, "left half should be uncovered");
    assert!((110..=145).contains(&right_alpha), "alpha {right_alpha}");
}

#[test]
fn deflicker_runs_in_the_engine_and_is_stable_when_a_frame_repeats() {
    let engine = Engine::new(config(1)).unwrap();
    let timeline = scene(vec![clip(
        1,
        0,
        fixture(),
        Placement {
            deflicker: Some(0.2),
            ..Placement::default()
        },
    )]);

    let mut frames = Vec::new();
    for playhead in [0.0, 0.04, 0.08, 0.08, 0.08] {
        engine.submit(request(timeline.clone(), playhead)).unwrap();
        frames.push(expect_frame(&engine).1);
    }
    assert_eq!(frames[2].pixels(), frames[3].pixels());
    assert_eq!(frames[3].pixels(), frames[4].pixels());

    let invalid = Placement {
        deflicker: Some(0.0),
        ..Placement::default()
    };
    assert!(SceneSnapshot::new(
        SceneVersions::default(),
        vec![clip(1, 0, fixture(), invalid)]
    )
    .is_err());
}

fn copies(count: usize, tag: &str) -> Vec<PathBuf> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("bush-multicam");
    std::fs::create_dir_all(&dir).unwrap();
    (0..count)
        .map(|index| {
            let path = dir.join(format!("{tag}-angle-{index}.mp4"));
            std::fs::copy(fixture(), &path).unwrap();
            path
        })
        .collect()
}

fn multicam_scene(paths: &[PathBuf], visible: &[usize]) -> SceneSnapshot {
    scene(
        paths
            .iter()
            .enumerate()
            .map(|(index, path)| {
                let mut angle = clip(
                    index as u64 + 1,
                    index as i32,
                    path.clone(),
                    Placement::default(),
                );
                angle.visible = visible.contains(&index);
                angle
            })
            .collect(),
    )
}

#[test]
fn layers_from_different_sources_decode_in_parallel_and_composite_in_order() {
    let paths = copies(3, "parallel");
    let engine = Engine::new(config(3)).unwrap();

    for round in 0..10 {
        let playhead = f64::from(round) / 12.0;
        engine
            .submit(request(multicam_scene(&paths, &[0, 1, 2]), playhead))
            .unwrap();
        let (_, frame) = expect_frame(&engine);
        // All layers are opaque identical media: the top layer wins and matches the reference.
        assert_eq!(frame.pixels(), reference_frame(playhead).pixels());
    }
    let stats = engine.stats();
    assert_eq!(stats.failures, 0);
    assert_eq!(stats.cache_misses, 30);
}

#[test]
fn angle_switch_reuses_open_sessions_and_cached_frames() {
    let paths = copies(2, "switch");
    let engine = Engine::new(config(2)).unwrap();
    let playhead = 0.4;

    engine
        .submit(request(multicam_scene(&paths, &[0]), playhead))
        .unwrap();
    expect_frame(&engine);
    // Switching the visible angle opens the second source once.
    engine
        .submit(request(multicam_scene(&paths, &[1]), playhead))
        .unwrap();
    expect_frame(&engine);
    // Switching back is served from the cache without decoding.
    engine
        .submit(request(multicam_scene(&paths, &[0]), playhead))
        .unwrap();
    expect_frame(&engine);

    let stats = engine.stats();
    assert_eq!(stats.cache_misses, 2);
    assert_eq!(stats.cache_hits, 1);
}

#[test]
fn more_sources_than_session_slots_still_render_correctly() {
    let paths = copies(3, "slots");
    let engine = Engine::new(config(1)).unwrap();
    engine
        .submit(request(multicam_scene(&paths, &[0, 1, 2]), 0.3))
        .unwrap();
    let (_, frame) = expect_frame(&engine);
    assert_eq!(frame.pixels(), reference_frame(0.3).pixels());
}
