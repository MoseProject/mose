//! Memory and cost of a timeline with many clips. Release builds only.
//! Run: `cargo run --release -p bush --example memory_probe -- <clip1> <clip2> ... [--open N] [--cache-mb M]`
//! Give it distinct files (copies of one clip work): the engine keys its decoders by path.
//!
//! Two scenarios, each printing the process's resident memory:
//!   cut:   play 45 frames of each clip in turn, the way a playhead crosses a timeline of cuts;
//!   stack: show N of the clips as layers of one frame (`--stack N`), which needs N decoders.
//! They are separate runs so one scenario's memory does not show up in the other.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, MediaSource, Placement, RendererChoice, SceneSnapshot,
    SceneVersions, SourceRange, Version,
};

const FPS: f64 = 60.0;
const FRAMES_PER_CLIP: u32 = 45;

/// Resident set size and its peak, in MiB (Linux; elsewhere nothing is reported).
fn memory() -> (f64, f64) {
    let Ok(status) = std::fs::read_to_string("/proc/self/status") else {
        return (0.0, 0.0);
    };
    let field = |name: &str| {
        status
            .lines()
            .find(|line| line.starts_with(name))
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|kb| kb.parse::<f64>().ok())
            .map_or(0.0, |kb| kb / 1024.0)
    };
    (field("VmRSS:"), field("VmHWM:"))
}

fn layer(index: usize, path: &std::path::Path, seconds: f64) -> Contributor {
    Contributor {
        id: ContributorId(index as u64 + 1),
        layer: index as i32,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path: path.to_path_buf(),
            range: SourceRange::new(0.0, seconds).unwrap(),
            version: Version(1),
        }),
        placement: Placement {
            fit_canvas: true,
            ..Placement::default()
        },
    }
}

fn play(engine: &Engine, scene: &SceneSnapshot, frames: u32) -> Vec<f64> {
    let mut times = Vec::new();
    for index in 0..frames {
        let begin = Instant::now();
        engine
            .submit(EngineRequest {
                scene: scene.clone(),
                playhead_secs: f64::from(index) / FPS,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(60)).unwrap() {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
        times.push(begin.elapsed().as_secs_f64() * 1000.0);
    }
    times
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn main() {
    let mut paths = Vec::new();
    let mut open = 6usize;
    let mut stack: Option<usize> = None;
    let mut cache_mb = 192usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open" => open = args.next().unwrap().parse().unwrap(),
            "--cache-mb" => cache_mb = args.next().unwrap().parse().unwrap(),
            "--stack" => stack = Some(args.next().unwrap().parse().unwrap()),
            _ => paths.push(PathBuf::from(arg)),
        }
    }
    assert!(!paths.is_empty(), "give at least one clip");
    let config = EngineConfig {
        canvas_width: 1280,
        canvas_height: 720,
        max_open_sources: open,
        frame_cache_bytes: cache_mb * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    };
    println!(
        "{} clips, max_open_sources {open}, frame cache {cache_mb} MiB, canvas 1280x720",
        paths.len()
    );
    let (start, _) = memory();
    println!("start: {start:.0} MiB resident");

    let engine = Engine::new(config).unwrap();
    if let Some(count) = stack {
        assert!(count <= paths.len(), "--stack needs that many clips");
        let layers = (0..count).map(|i| layer(i, &paths[i], 20.0)).collect();
        let scene = SceneSnapshot::new(SceneVersions::default(), layers).unwrap();
        let times = play(&engine, &scene, FRAMES_PER_CLIP);
        let (rss, peak) = memory();
        println!(
            "stack of {count} layers: median {:>7.1} ms/frame   resident {rss:>6.0} MiB (peak {peak:.0}){}",
            median(times),
            if count > open { "   (more layers than open decoders)" } else { "" }
        );
        return;
    }
    println!("\ncut: {FRAMES_PER_CLIP} frames of each clip in turn");
    for (index, path) in paths.iter().enumerate() {
        let scene =
            SceneSnapshot::new(SceneVersions::default(), vec![layer(0, path, 20.0)]).unwrap();
        let times = play(&engine, &scene, FRAMES_PER_CLIP);
        let (rss, peak) = memory();
        println!(
            "  clip {:>2}: median {:>6.1} ms/frame   resident {rss:>6.0} MiB (peak {peak:.0})",
            index + 1,
            median(times)
        );
    }
}
