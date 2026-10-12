//! Sequential playback cost per frame, software against hardware decode. Release builds only.
//! Run: `cargo run --release -p bush --example playback_probe -- <video> [width height frames]`.
//! Set `PROBE_DECODE=hardware` for `DecodePolicy::HardwareWithFallback`, `PROBE_TRACE=1` to list
//! the frames that took more than twice the median.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, MediaSource, Placement, RendererChoice, SceneSnapshot,
    SceneVersions, SourceRange, Version,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("video path"));
    let width: u32 = args.next().map_or(1280, |v| v.parse().unwrap());
    let height: u32 = args.next().map_or(720, |v| v.parse().unwrap());
    let frames: u32 = args.next().map_or(240, |v| v.parse().unwrap());
    let hardware = std::env::var("PROBE_DECODE").is_ok_and(|v| v == "hardware");
    let fps = 30.0;

    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path,
                range: SourceRange::new(0.0, f64::from(frames) / fps + 1.0).unwrap(),
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
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: if hardware {
            DecodePolicy::HardwareWithFallback
        } else {
            DecodePolicy::Software
        },
        renderer: RendererChoice::Cpu,
    })
    .unwrap();

    let mut per_frame = Vec::with_capacity(frames as usize);
    let mut first = Duration::ZERO;
    let started = Instant::now();
    for index in 0..frames {
        let begin = Instant::now();
        engine
            .submit(EngineRequest {
                scene: scene.clone(),
                playhead_secs: f64::from(index) / fps,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(30)).unwrap() {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
        let elapsed = begin.elapsed();
        if index == 0 {
            first = elapsed;
        } else {
            per_frame.push(elapsed.as_secs_f64() * 1000.0);
        }
    }
    let total = started.elapsed().as_secs_f64();
    if std::env::var("PROBE_TRACE").is_ok() {
        // The frames well over the median, by index: a pattern here (every GOP, every N frames)
        // points at the cause; scattered ones at the machine.
        let mut ordered = per_frame.clone();
        ordered.sort_by(f64::total_cmp);
        let limit = (ordered[ordered.len() / 2] * 2.0).max(20.0);
        for (index, ms) in per_frame.iter().enumerate() {
            if *ms > limit {
                println!("frame {:>4}: {:.1} ms", index + 1, ms);
            }
        }
    }
    per_frame.sort_by(f64::total_cmp);
    let pick = |q: f64| per_frame[((per_frame.len() - 1) as f64 * q) as usize];
    println!(
        "{} decode, {} frames at {width}x{height}: first {:.0} ms, p50 {:.2} ms, p95 {:.2} ms, max {:.2} ms, {:.1} frames/s sustained",
        if hardware { "hardware" } else { "software" },
        frames,
        first.as_secs_f64() * 1000.0,
        pick(0.5),
        pick(0.95),
        per_frame.last().unwrap(),
        f64::from(frames) / total
    );
}
