use std::path::PathBuf;
use std::time::Duration;

use bush::{
    glitch, shake_offset, Contributor, ContributorContent, ContributorId, DecodePolicy, Engine,
    EngineConfig, EngineEvent, EngineRequest, MediaSource, PixelFormat, Placement, RendererChoice,
    SceneError, SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};

fn gradient(width: u32, height: u32, timestamp: f64) -> VideoFrame {
    let mut pixels = Vec::new();
    for y in 0..height {
        for x in 0..width {
            pixels.extend_from_slice(&[(x * 255 / width) as u8, (y * 255 / height) as u8, 90, 255]);
        }
    }
    VideoFrame::new(
        PixelFormat::Rgba8,
        width,
        height,
        width as usize * 4,
        timestamp,
        pixels,
    )
    .unwrap()
}

#[test]
fn zero_intensity_is_identity_and_results_are_deterministic() {
    let source = gradient(120, 80, 1.0);
    assert_eq!(glitch(&source, 0.0, 1.0), source);
    assert_eq!(glitch(&source, -1.0, 1.0), source);
    assert_eq!(glitch(&source, f32::NAN, 1.0), source);

    let first = glitch(&source, 0.6, 3.25);
    let second = glitch(&source, 0.6, 3.25);
    assert_eq!(first, second);
    assert_eq!(
        (first.width(), first.height(), first.timestamp_secs()),
        (120, 80, 1.0)
    );
}

#[test]
fn channels_split_horizontally_and_alpha_is_preserved() {
    // A vertical white bar on black: red moves left, blue moves right, green stays.
    let mut pixels = Vec::new();
    for _ in 0..40u32 {
        for x in 0..100u32 {
            let value = if (48..52).contains(&x) { 255 } else { 0 };
            pixels.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let source = VideoFrame::new(PixelFormat::Rgba8, 100, 40, 400, 0.0, pixels).unwrap();
    // Intensity small enough that no band is displaced on the sampled row would be luck, so
    // inspect rows with no displacement by comparing the green channel to the source instead.
    let out = glitch(&source, 0.3, 0.0);
    let mut split_rows = 0;
    for y in 0..40usize {
        let row = &out.pixels()[y * 400..(y + 1) * 400];
        let green: Vec<usize> = (0..100).filter(|x| row[x * 4 + 1] == 255).collect();
        let red: Vec<usize> = (0..100).filter(|x| row[x * 4] == 255).collect();
        let blue: Vec<usize> = (0..100).filter(|x| row[x * 4 + 2] == 255).collect();
        if green.is_empty() || red.is_empty() || blue.is_empty() {
            continue;
        }
        assert!(red[0] < green[0], "red must lean left of green");
        assert!(blue[0] > green[0], "blue must lean right of green");
        split_rows += 1;
        assert!((0..100).all(|x| row[x * 4 + 3] == 255));
    }
    assert!(split_rows > 20, "only {split_rows} rows showed the split");
}

#[test]
fn the_pattern_changes_between_steps_but_not_within_one() {
    let source = gradient(160, 90, 0.0);
    // 12 steps per second: 0.01 s and 0.05 s share a step, 0.2 s is a later one.
    assert_eq!(glitch(&source, 0.8, 0.01), glitch(&source, 0.8, 0.05));
    assert_ne!(glitch(&source, 0.8, 0.01), glitch(&source, 0.8, 0.2));
    // Stronger glitch distorts the picture more, averaged over several glitch steps so one
    // lucky band placement cannot decide the comparison.
    let distortion = |intensity: f32| -> f64 {
        (0..12)
            .map(|step| {
                let out = glitch(&source, intensity, f64::from(step) / 12.0 + 0.01);
                out.pixels()
                    .iter()
                    .zip(source.pixels())
                    .map(|(a, b)| f64::from(a.abs_diff(*b)))
                    .sum::<f64>()
            })
            .sum::<f64>()
    };
    assert!(distortion(1.0) > distortion(0.1) * 1.5);
}

#[test]
fn shake_is_smooth_bounded_deterministic_and_decorrelated() {
    assert_eq!(shake_offset(0.0, 1.0, 1), 0.0);
    assert_eq!(shake_offset(f32::NAN, 1.0, 1), 0.0);
    assert_eq!(shake_offset(10.0, 2.5, 1), shake_offset(10.0, 2.5, 1));

    let mut previous = shake_offset(40.0, 0.0, 1);
    let mut max_step = 0.0_f32;
    let mut differs_between_streams = false;
    for step in 1..2_000 {
        let time = f64::from(step) / 1_000.0;
        let value = shake_offset(40.0, time, 1);
        assert!(value.abs() <= 40.0);
        max_step = max_step.max((value - previous).abs());
        previous = value;
        differs_between_streams |= value != shake_offset(40.0, time, 2);
    }
    // Smooth: a millisecond never moves the layer more than a few pixels.
    assert!(max_step < 2.0, "max step {max_step}");
    assert!(differs_between_streams);
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn clip(placement: Placement) -> Contributor {
    Contributor {
        id: ContributorId(1),
        layer: 0,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path: fixture(),
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement,
    }
}

fn render(placement: Placement, playhead: f64) -> VideoFrame {
    let info = bush::DecodeSource::open(&fixture()).unwrap().info();
    let engine = Engine::new(EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    engine
        .submit(EngineRequest {
            scene: SceneSnapshot::new(SceneVersions::default(), vec![clip(placement)]).unwrap(),
            playhead_secs: playhead,
            overlays: Vec::new(),
        })
        .unwrap();
    match engine.wait(Duration::from_secs(30)).unwrap() {
        EngineEvent::Frame { frame, .. } => frame,
        EngineEvent::Failed { message, .. } => panic!("{message}"),
    }
}

#[test]
fn engine_applies_glitch_and_shake_deterministically() {
    let plain = render(Placement::default(), 0.4);

    let glitched = render(
        Placement {
            glitch: 0.7,
            ..Placement::default()
        },
        0.4,
    );
    assert_ne!(glitched.pixels(), plain.pixels());
    assert_eq!(
        glitched.pixels(),
        render(
            Placement {
                glitch: 0.7,
                ..Placement::default()
            },
            0.4
        )
        .pixels()
    );

    let shaken_placement = Placement {
        shake: 30.0,
        ..Placement::default()
    };
    let shaken = render(shaken_placement.clone(), 0.4);
    assert_ne!(shaken.pixels(), plain.pixels());
    assert_eq!(shaken.pixels(), render(shaken_placement, 0.4).pixels());
}

#[test]
fn out_of_range_glitch_and_shake_are_rejected() {
    for placement in [
        Placement {
            glitch: 1.5,
            ..Placement::default()
        },
        Placement {
            shake: 250.0,
            ..Placement::default()
        },
    ] {
        assert_eq!(
            SceneSnapshot::new(SceneVersions::default(), vec![clip(placement)]).unwrap_err(),
            SceneError::InvalidPlacement
        );
    }
}
