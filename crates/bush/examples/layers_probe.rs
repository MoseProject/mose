//! Playback cost as layers pile up: N copies of one clip stacked on the canvas, optionally with
//! an effect on each. Run: `cargo run --release -p bush --example layers_probe -- clip.mp4 N
//! [plain|colour|blur|mask|rotate] [width height frames]`.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use bush::{
    Contributor, ContributorContent, ContributorId, CropRect, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, LayerEffects, LayerTransform, MediaSource, Placement,
    RendererChoice, SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn main() {
    let mut args = std::env::args().skip(1);
    let source = PathBuf::from(
        args.next()
            .expect("usage: layers_probe clip.mp4 N [effect]"),
    );
    let layers: usize = args.next().map_or(4, |v| v.parse().unwrap());
    let effect = args.next().unwrap_or_else(|| "plain".into());
    let width: u32 = args.next().map_or(1280, |v| v.parse().unwrap());
    let height: u32 = args.next().map_or(720, |v| v.parse().unwrap());
    let frames: u32 = args.next().map_or(60, |v| v.parse().unwrap());
    let fps = 30.0;

    // One file per layer, so each layer needs its own decoder like separate clips would.
    let dir = std::env::temp_dir().join("bush_layers_probe");
    std::fs::create_dir_all(&dir).unwrap();
    let paths: Vec<PathBuf> = (0..layers)
        .map(|index| {
            let path = dir.join(format!("layer-{index}.mp4"));
            if !path.exists() {
                std::fs::copy(&source, &path).unwrap();
            }
            path
        })
        .collect();

    let contributors = paths
        .into_iter()
        .enumerate()
        .map(|(index, path)| {
            let mut placement = Placement {
                fit_canvas: true,
                opacity: if matches!(effect.as_str(), "plain" | "colour" | "colour-solid") {
                    1.0
                } else {
                    0.8
                },
                ..Placement::default()
            };
            match effect.as_str() {
                "colour" | "colour-solid" => {
                    placement.effects = LayerEffects {
                        brightness: 0.1,
                        contrast: 1.2,
                        saturation: 1.1,
                        ..LayerEffects::default()
                    }
                }
                "blur" => placement.blur_sigma = 3.0,
                "mask" => {
                    placement.effects = LayerEffects {
                        mask: Some(bush::LayerMask {
                            shape: bush::MaskShape::Ellipse,
                            corner_radius: 0.0,
                            feather: 0.1,
                            invert: false,
                        }),
                        ..LayerEffects::default()
                    }
                }
                "rotate" => {
                    placement.transform = LayerTransform {
                        scale_x: 0.6,
                        scale_y: 0.6,
                        rotation_deg: 10.0 + index as f32,
                        crop: CropRect::FULL,
                        ..LayerTransform::default()
                    }
                }
                _ => {}
            }
            Contributor {
                id: ContributorId(index as u64 + 1),
                layer: index as i32,
                visible: true,
                audible: false,
                content: ContributorContent::Media(MediaSource {
                    path,
                    range: SourceRange::new(0.0, f64::from(frames) / fps + 1.0).unwrap(),
                    version: Version(1),
                }),
                placement,
            }
        })
        .collect();
    let scene = SceneSnapshot::new(SceneVersions::default(), contributors).unwrap();
    let engine = Engine::new(EngineConfig {
        canvas_width: width,
        canvas_height: height,
        max_open_sources: 2,
        frame_cache_bytes: 128 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    let mut per_frame = Vec::new();
    for index in 0..frames {
        let begin = Instant::now();
        engine
            .submit(EngineRequest {
                scene: scene.clone(),
                playhead_secs: f64::from(index) / fps,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(60)).unwrap() {
            EngineEvent::Frame { .. } => {}
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
        if index > 0 {
            per_frame.push(begin.elapsed().as_secs_f64() * 1e3);
        }
    }
    per_frame.sort_by(f64::total_cmp);
    let pick = |q: f64| per_frame[((per_frame.len() - 1) as f64 * q) as usize];
    println!(
        "{layers:>3} layers {effect:<7} at {width}x{height}: p50 {:>7.1} ms  p95 {:>7.1} ms  => {:>5.1} fps",
        pick(0.5),
        pick(0.95),
        1e3 / pick(0.5)
    );
}
