//! Scrubbing a long-GOP file: a seek may settle for the newest frame it decoded within a budget,
//! and the exact request that follows is not affected by it.
//!
//! The clip is made with the `ffmpeg` command line (one key frame per 150 frames); where there is
//! none the tests return early.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
    EngineConfig, EngineEvent, EngineRequest, MediaSource, Placement, RendererChoice,
    SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;

/// A 12 s, 30 fps clip whose only key frames are at 0 s and 5 s (and 10 s); `None` without ffmpeg.
fn long_gop() -> Option<&'static Path> {
    static CLIP: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLIP.get_or_init(|| {
        let path = std::env::temp_dir().join(format!("bush-long-gop-{}.mp4", std::process::id()));
        // libx264 where the build has it, MPEG-4 part 2 (every build has it) otherwise.
        for encoder in ["libx264", "mpeg4"] {
            let made = Command::new("ffmpeg")
                .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
                .arg(format!(
                    "testsrc2=size={WIDTH}x{HEIGHT}:rate=30:duration=12"
                ))
                .args([
                    "-c:v", encoder, "-g", "150", "-bf", "0", "-pix_fmt", "yuv420p",
                ])
                .arg(&path)
                .output();
            match made {
                Ok(output) if output.status.success() => return Some(path),
                Ok(_) => {}
                Err(_) => {
                    eprintln!("no ffmpeg on PATH: the long-GOP tests are skipped");
                    return None;
                }
            }
        }
        panic!("ffmpeg could not make the long-GOP clip with libx264 or mpeg4");
    })
    .as_deref()
}

fn cancel() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn a_spent_budget_presents_the_newest_frame_decoded_and_the_exact_request_carries_on() {
    let Some(clip) = long_gop() else { return };
    let mut source = DecodeSource::open(clip).unwrap();
    let target = 4.0;

    // The deadline has passed before the first frame: one candidate is all it gets.
    let started = Instant::now();
    let rough = source
        .frame_at_by(target, &cancel(), Instant::now())
        .unwrap()
        .unwrap();
    let rough_time = started.elapsed();
    assert!(source.last_frame_was_approximate());
    assert!(
        rough.timestamp_secs() < target - 1.0,
        "stopped near the key frame"
    );

    // The exact request after it reaches the target frame, from where the first one stopped.
    let exact = source.frame_at(target, &cancel()).unwrap().unwrap();
    assert!(!source.last_frame_was_approximate());
    let reference = DecodeSource::open(clip)
        .unwrap()
        .frame_at(target, &cancel())
        .unwrap()
        .unwrap();
    assert_eq!(exact.timestamp_secs(), reference.timestamp_secs());
    assert_eq!(exact.pixels(), reference.pixels());
    assert!((exact.timestamp_secs() - target).abs() < 1.0 / 30.0 + 1e-6);
    // Reaching the key frame costs a fraction of decoding 100 frames forward.
    assert!(rough_time < Duration::from_secs(2));
}

#[test]
fn a_budget_that_is_long_enough_changes_nothing() {
    let Some(clip) = long_gop() else { return };
    let mut source = DecodeSource::open(clip).unwrap();
    let exact = source
        .frame_at_by(4.0, &cancel(), Instant::now() + Duration::from_secs(60))
        .unwrap()
        .unwrap();
    assert!(!source.last_frame_was_approximate());
    let reference = DecodeSource::open(clip)
        .unwrap()
        .frame_at(4.0, &cancel())
        .unwrap()
        .unwrap();
    assert_eq!(exact.pixels(), reference.pixels());
}

fn engine() -> Engine {
    Engine::new(EngineConfig {
        canvas_width: WIDTH,
        canvas_height: HEIGHT,
        max_open_sources: 2,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap()
}

fn request(clip: &Path, playhead_secs: f64) -> EngineRequest {
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path: clip.to_path_buf(),
                range: SourceRange::new(0.0, 12.0).unwrap(),
                version: Version(1),
            }),
            placement: Placement {
                fit_canvas: true,
                ..Placement::default()
            },
        }],
    )
    .unwrap();
    EngineRequest {
        scene,
        playhead_secs,
        overlays: Vec::new(),
    }
}

fn next_frame(engine: &Engine) -> VideoFrame {
    match engine.wait(Duration::from_secs(30)).unwrap() {
        EngineEvent::Frame { frame, .. } => frame,
        EngineEvent::Failed { message, .. } => panic!("{message}"),
    }
}

#[test]
fn the_engine_scrubs_roughly_and_the_exact_frame_that_follows_is_not_poisoned_by_it() {
    let Some(clip) = long_gop() else { return };
    let scrubbing = engine();
    scrubbing.set_seek_budget(Some(Duration::from_micros(1)));
    scrubbing.submit(request(clip, 4.0)).unwrap();
    let rough = next_frame(&scrubbing);

    scrubbing.set_seek_budget(None);
    scrubbing.submit(request(clip, 4.0)).unwrap();
    let settled = next_frame(&scrubbing);

    let reference_engine = engine();
    reference_engine.submit(request(clip, 4.0)).unwrap();
    let reference = next_frame(&reference_engine);

    assert_ne!(
        rough.pixels(),
        reference.pixels(),
        "the rough frame is an earlier one"
    );
    assert_eq!(settled.pixels(), reference.pixels());
}

#[test]
fn without_a_budget_the_engine_is_exact() {
    let Some(clip) = long_gop() else { return };
    let plain = engine();
    plain.submit(request(clip, 7.0)).unwrap();
    let frame = next_frame(&plain);
    let reference: Arc<VideoFrame> = DecodeSource::open(clip)
        .unwrap()
        .frame_at(7.0, &cancel())
        .unwrap()
        .unwrap();
    assert_eq!(frame.pixels().len(), reference.pixels().len());
    assert_eq!(frame.timestamp_secs(), 7.0);
}
