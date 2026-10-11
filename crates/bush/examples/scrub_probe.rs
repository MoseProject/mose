//! Latency of random scrubbing over a whole clip (long-GOP seeks included), as the engine sees
//! it. Run: `cargo run --release -p bush --example scrub_probe -- clip.mp4 [width height steps]`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
    EngineConfig, EngineEvent, EngineRequest, MediaSource, Placement, RendererChoice,
    SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .expect("usage: scrub_probe clip.mp4 [w h steps]"),
    );
    let width: u32 = args.next().map_or(1280, |v| v.parse().unwrap());
    let height: u32 = args.next().map_or(720, |v| v.parse().unwrap());
    let steps: u32 = args.next().map_or(60, |v| v.parse().unwrap());
    let duration = DecodeSource::open(&path)
        .unwrap()
        .info()
        .duration_secs
        .expect("a clip with a duration");
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path,
                range: SourceRange::new(0.0, duration).unwrap(),
                version: Version(1),
            }),
            placement: Placement {
                fit_canvas: true,
                ..Placement::default()
            },
        }],
    )
    .unwrap();
    let engine = Engine::new(EngineConfig {
        canvas_width: width,
        canvas_height: height,
        max_open_sources: 2,
        frame_cache_bytes: 256 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    let mut times = Vec::new();
    for _ in 0..steps {
        // A cheap deterministic spread over the clip.
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let playhead = (state >> 33) as f64 / f64::from(1u32 << 31) * (duration - 0.2);
        let begin = Instant::now();
        engine
            .submit(EngineRequest {
                scene: scene.clone(),
                playhead_secs: playhead,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(30)).unwrap() {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
        times.push(begin.elapsed().as_secs_f64() * 1e3);
    }
    times.sort_by(f64::total_cmp);
    let pick = |q: f64| times[((times.len() - 1) as f64 * q) as usize];
    println!(
        "{steps} random seeks at {width}x{height}: p50 {:.1} ms, p95 {:.1} ms, max {:.1} ms",
        pick(0.5),
        pick(0.95),
        times.last().unwrap()
    );
}
