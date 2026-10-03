//! Prints engine latency percentiles for a few representative paths (release builds only make
//! sense). Run: `cargo run --release -p bush --example latency_probe`.

use std::path::PathBuf;
use std::time::Duration;

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, MediaSource, Placement, SceneSnapshot, SceneVersions, SourceRange,
    Version,
};

fn scene(path: PathBuf, opacity: f32) -> SceneSnapshot {
    SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path,
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement {
                opacity,
                ..Placement::default()
            },
        }],
    )
    .unwrap()
}

fn run(label: &str, path: PathBuf, width: u32, height: u32, edits_only: bool) {
    let engine = Engine::new(EngineConfig {
        canvas_width: width,
        canvas_height: height,
        max_open_sources: 2,
        frame_cache_bytes: 256 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
    })
    .unwrap();
    for step in 0..200u32 {
        let (playhead, opacity) = if edits_only {
            (0.4, 0.5 + f32::from((step % 2) as u8) * 0.25)
        } else {
            (f64::from(step * 7 % 90) / 100.0, 1.0)
        };
        engine
            .submit(EngineRequest {
                scene: scene(path.clone(), opacity),
                playhead_secs: playhead,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(30)).unwrap() {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
    }
    let latency = engine.latency();
    println!(
        "{label:<34} p50 {:>6} us  p95 {:>6} us  p99 {:>6} us  max {:>6} us",
        latency.p50_micros, latency.p95_micros, latency.p99_micros, latency.max_micros
    );
}

fn main() {
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures");
    run("640x360 scrub (random seek)", fixtures.join("video_h264.mp4"), 640, 360, false);
    run("640x360 live edit (cached)", fixtures.join("video_h264.mp4"), 640, 360, true);
    if let Some(dir) = std::env::var_os("BUSH_BENCH_MEDIA_DIR").map(PathBuf::from) {
        let big = dir.join("h264_1080p.mp4");
        if big.exists() {
            run("1080p scrub (random seek)", big.clone(), 1920, 1080, false);
            run("1080p live edit (cached)", big, 1920, 1080, true);
        }
    }
}
