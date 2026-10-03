//! End-to-end editor workloads against real fixtures (spec B01-B04 subset).
//!
//! Run with `cargo bench -p bush --bench engine_workloads`. Unlike `editor_workloads`, these
//! include FFmpeg decode and CPU composition, so numbers depend on the machine: record the
//! hardware beside any figure quoted from them.

use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use bush::{
    BlendMode, Contributor, ContributorContent, ContributorId, CpuRenderer, DecodePolicy,
    DecodeSource, Engine, EngineConfig, EngineEvent, EngineRequest, LayerFrame, MediaSource,
    PixelFormat, Placement, SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};
use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../avbridge/tests/fixtures")
        .join(name)
}

fn scene(path: PathBuf, placement: Placement) -> SceneSnapshot {
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
            placement,
        }],
    )
    .unwrap()
}

fn engine(width: u32, height: u32, policy: DecodePolicy) -> Engine {
    Engine::new(EngineConfig {
        canvas_width: width,
        canvas_height: height,
        max_open_sources: 2,
        frame_cache_bytes: 128 * 1024 * 1024,
        decode_policy: policy,
    })
    .unwrap()
}

fn present(engine: &Engine, scene: &SceneSnapshot, playhead: f64) -> VideoFrame {
    engine
        .submit(EngineRequest {
            scene: scene.clone(),
            playhead_secs: playhead,
            overlays: Vec::new(),
        })
        .unwrap();
    match engine.wait(Duration::from_secs(30)).expect("no frame") {
        EngineEvent::Frame { frame, .. } => frame,
        EngineEvent::Failed { message, .. } => panic!("{message}"),
    }
}

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> VideoFrame {
    VideoFrame::new(
        PixelFormat::Rgba8,
        width,
        height,
        width as usize * 4,
        0.0,
        rgba.repeat((width * height) as usize),
    )
    .unwrap()
}

/// B04 core: two 1080p layers, the top one scaled, rotated and multiply-blended.
fn bench_composite_1080p(c: &mut Criterion) {
    let background = solid(1920, 1080, [40, 90, 160, 255]);
    let overlay = solid(1280, 720, [220, 180, 40, 200]);
    let renderer = CpuRenderer::new(1920 * 1080 * 4).unwrap();
    let mut group = c.benchmark_group("composite_1080p");
    group.sample_size(20);

    group.bench_function("two_layers_untransformed", |b| {
        let layers = [LayerFrame::new(&background), LayerFrame::new(&overlay)];
        b.iter(|| black_box(renderer.render(1920, 1080, 0.0, &layers).unwrap()));
    });
    group.bench_function("two_layers_scaled_rotated_blended", |b| {
        let mut top = LayerFrame::new(&overlay);
        top.x = 200;
        top.y = 100;
        top.blend = BlendMode::Multiply;
        top.opacity = 0.8;
        top.transform.scale_x = 0.75;
        top.transform.scale_y = 0.75;
        top.transform.rotation_deg = 12.0;
        let layers = [LayerFrame::new(&background), top];
        b.iter(|| black_box(renderer.render(1920, 1080, 0.0, &layers).unwrap()));
    });
    group.finish();
}

/// B02: scrub-to-correct-frame on a long-GOP-style source, cold decoder state each time.
fn bench_scrub(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_scrub");
    group.sample_size(20);
    let scene = scene(fixture("video_h264.mp4"), Placement::default());
    let engine = engine(640, 360, DecodePolicy::Software);
    let mut step = 0u32;
    group.bench_function("random_seek_h264_software", |b| {
        b.iter(|| {
            step = step.wrapping_add(7);
            let playhead = f64::from(step % 90) / 100.0;
            black_box(present(&engine, &scene, playhead))
        });
    });
    group.finish();
}

/// B04: a live parameter edit at an unchanged playhead must not re-decode.
fn bench_live_edit(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_live_edit");
    group.sample_size(20);
    let engine = engine(640, 360, DecodePolicy::Software);
    let opaque = scene(fixture("video_h264.mp4"), Placement::default());
    present(&engine, &opaque, 0.4);
    let mut toggle = false;
    group.bench_function("opacity_change_cached_frame", |b| {
        b.iter(|| {
            toggle = !toggle;
            let faded = scene(
                fixture("video_h264.mp4"),
                Placement {
                    opacity: if toggle { 0.5 } else { 0.9 },
                    ..Placement::default()
                },
            );
            black_box(present(&engine, &faded, 0.4))
        });
    });
    group.finish();
}

/// B03: steady playback, 22 consecutive frames per iteration through one engine.
fn bench_playback(c: &mut Criterion) {
    let mut group = c.benchmark_group("engine_playback");
    group.sample_size(10);
    for (label, policy) in [
        ("h264_software", DecodePolicy::Software),
        ("h264_hardware_fallback", DecodePolicy::HardwareWithFallback),
    ] {
        let scene = scene(fixture("video_h264.mp4"), Placement::default());
        group.bench_function(label, |b| {
            b.iter_batched(
                || engine(640, 360, policy),
                |engine| {
                    for frame in 0..22 {
                        black_box(present(&engine, &scene, f64::from(frame) / 25.0));
                    }
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

/// Raw decode throughput, software vs hardware, no composition.
fn bench_decode_policy(c: &mut Criterion) {
    let mut group = c.benchmark_group("decode_policy");
    group.sample_size(10);
    let path = fixture("video_h264.mp4");
    for (label, policy) in [
        ("software", DecodePolicy::Software),
        ("hardware_fallback", DecodePolicy::HardwareWithFallback),
    ] {
        group.bench_function(label, |b| {
            let cancel = AtomicBool::new(false);
            b.iter(|| {
                let mut source = DecodeSource::open_with_policy(&path, policy).unwrap();
                let mut frames = 0;
                while let Some(frame) = source.next_frame(&cancel).unwrap() {
                    black_box(&frame);
                    frames += 1;
                }
                frames
            });
        });
    }
    group.finish();
}

/// Large-media workloads. Set `BUSH_BENCH_MEDIA_DIR` to a directory holding `h264_1080p.mp4`
/// and/or `hevc_2160p.mp4` (any clip of at least one second); missing files are skipped, so
/// the benchmark never depends on checked-in multi-megabyte media.
fn bench_large_media(c: &mut Criterion) {
    let Some(dir) = std::env::var_os("BUSH_BENCH_MEDIA_DIR").map(PathBuf::from) else {
        return;
    };
    let mut group = c.benchmark_group("large_media");
    group.sample_size(10);
    for (name, width, height) in [
        ("h264_1080p.mp4", 1920, 1080),
        ("hevc_2160p.mp4", 3840, 2160),
    ] {
        let path = dir.join(name);
        if !path.exists() {
            continue;
        }
        for (label, policy) in [
            ("software", DecodePolicy::Software),
            ("hardware_fallback", DecodePolicy::HardwareWithFallback),
        ] {
            group.bench_function(format!("decode_24_frames/{name}/{label}"), |b| {
                let cancel = AtomicBool::new(false);
                b.iter(|| {
                    let mut source = DecodeSource::open_with_policy(&path, policy).unwrap();
                    for _ in 0..24 {
                        black_box(source.next_frame(&cancel).unwrap().unwrap());
                    }
                });
            });
        }
        let scene = scene(path.clone(), Placement::default());
        group.bench_function(format!("engine_sequential_24_frames/{name}"), |b| {
            b.iter_batched(
                || engine(width, height, DecodePolicy::Software),
                |engine| {
                    for frame in 0..24 {
                        black_box(present(&engine, &scene, f64::from(frame) / 30.0));
                    }
                },
                BatchSize::PerIteration,
            );
        });
    }
    group.finish();
}

/// B05: three 1080p angles visible at once (worst case: every angle decodes each frame).
fn bench_multicam(c: &mut Criterion) {
    let Some(dir) = std::env::var_os("BUSH_BENCH_MEDIA_DIR").map(PathBuf::from) else {
        return;
    };
    let source = dir.join("h264_1080p.mp4");
    if !source.exists() {
        return;
    }
    let angles: Vec<PathBuf> = (0..3)
        .map(|index| {
            let path = dir.join(format!("multicam-angle-{index}.mp4"));
            if !path.exists() {
                std::fs::copy(&source, &path).unwrap();
            }
            path
        })
        .collect();
    let contributors: Vec<Contributor> = angles
        .iter()
        .enumerate()
        .map(|(index, path)| Contributor {
            id: ContributorId(index as u64 + 1),
            layer: index as i32,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path: path.clone(),
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement::default(),
        })
        .collect();
    let scene = SceneSnapshot::new(SceneVersions::default(), contributors).unwrap();

    let mut group = c.benchmark_group("multicam");
    group.sample_size(10);
    group.bench_function("three_1080p_angles_24_frames", |b| {
        b.iter_batched(
            || {
                Engine::new(EngineConfig {
                    canvas_width: 1920,
                    canvas_height: 1080,
                    max_open_sources: 3,
                    frame_cache_bytes: 256 * 1024 * 1024,
                    decode_policy: DecodePolicy::Software,
                })
                .unwrap()
            },
            |engine| {
                for frame in 0..24 {
                    black_box(present(&engine, &scene, f64::from(frame) / 30.0));
                }
            },
            BatchSize::PerIteration,
        );
    });
    group.finish();
}

criterion_group!(
    engine_workloads,
    bench_composite_1080p,
    bench_scrub,
    bench_live_edit,
    bench_playback,
    bench_decode_policy,
    bench_large_media,
    bench_multicam
);
criterion_main!(engine_workloads);
