//! GPU compositor parity against the CPU reference. Requires `--features gpu` and an adapter;
//! each test skips itself (with a message) when none exists so headless CI stays green.
#![cfg(feature = "gpu")]

use bush::{
    BlendMode, ChromaKey, CpuRenderer, CropRect, GpuRenderer, LayerEffects, LayerFrame, LayerMask,
    Lut3D, MaskShape, PixelFormat, RenderError, Renderer, VideoFrame,
};

fn gpu(max_bytes: usize) -> Option<GpuRenderer> {
    match GpuRenderer::new(max_bytes) {
        Ok(renderer) => {
            eprintln!("gpu adapter: {}", renderer.adapter_name());
            Some(renderer)
        }
        Err(error) => {
            eprintln!("skipped: no usable GPU adapter ({error:?})");
            None
        }
    }
}

fn noise(width: u32, height: u32, seed: u64, translucent: bool) -> VideoFrame {
    let mut state = seed;
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let bytes = state.to_le_bytes();
        let alpha = match (translucent, bytes[3] % 4) {
            (false, _) => 255,
            (true, 0) => 0,
            (true, 1) => 255,
            (true, _) => bytes[4],
        };
        pixels.extend_from_slice(&[bytes[0], bytes[1], bytes[2], alpha]);
    }
    VideoFrame::new(
        PixelFormat::Rgba8,
        width,
        height,
        width as usize * 4,
        0.0,
        pixels,
    )
    .unwrap()
}

struct Comparison {
    max_difference: u8,
    differing_pixels: usize,
    pixels: usize,
}

fn compare(
    gpu: &mut GpuRenderer,
    width: u32,
    height: u32,
    layers: &[LayerFrame<'_>],
) -> Comparison {
    let mut cpu = CpuRenderer::new((width * height * 4) as usize).unwrap();
    let expected = Renderer::render(&mut cpu, width, height, 0.5, layers).unwrap();
    let actual = gpu.render(width, height, 0.5, layers).unwrap();
    assert_eq!(actual.timestamp_secs(), 0.5);
    let mut max_difference = 0;
    let mut differing_pixels = 0;
    for (left, right) in expected
        .pixels()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(actual.pixels().as_chunks::<4>().0.iter())
    {
        let difference = left
            .iter()
            .zip(right)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap();
        if difference > 0 {
            differing_pixels += 1;
        }
        max_difference = max_difference.max(difference);
    }
    Comparison {
        max_difference,
        differing_pixels,
        pixels: (width * height) as usize,
    }
}

/// Floating-point GPU and CPU arithmetic can disagree by one level at a rounding boundary; a
/// real porting bug shows up as large differences or many differing pixels.
fn assert_parity(label: &str, comparison: &Comparison, max_difference: u8, max_fraction: f64) {
    let fraction = comparison.differing_pixels as f64 / comparison.pixels as f64;
    eprintln!(
        "{label}: max difference {}, {:.4}% pixels differ",
        comparison.max_difference,
        fraction * 100.0
    );
    assert!(
        comparison.max_difference <= max_difference,
        "{label}: max difference {}",
        comparison.max_difference
    );
    assert!(
        fraction <= max_fraction,
        "{label}: {:.4}% differ",
        fraction * 100.0
    );
}

#[test]
fn opaque_and_translucent_layers_match_the_reference() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let background = noise(320, 240, 1, false);
    let translucent = noise(200, 150, 2, true);
    let faded = noise(120, 120, 3, true);
    let mut second = LayerFrame::new(&translucent);
    second.x = 40;
    second.y = 25;
    let mut third = LayerFrame::new(&faded);
    third.x = -30;
    third.y = 150;
    third.opacity = 0.6;
    let comparison = compare(
        &mut gpu,
        320,
        240,
        &[LayerFrame::new(&background), second, third],
    );
    assert_parity("alpha layers", &comparison, 0, 0.0);
}

#[test]
fn every_blend_mode_matches_the_reference() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let backdrop = noise(160, 120, 11, false);
    let source = noise(160, 120, 12, true);
    for mode in [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Add,
        BlendMode::Difference,
    ] {
        let mut top = LayerFrame::new(&source);
        top.blend = mode;
        top.opacity = 0.7;
        let comparison = compare(&mut gpu, 160, 120, &[LayerFrame::new(&backdrop), top]);
        assert_parity(&format!("{mode:?}"), &comparison, 1, 0.005);
    }
}

#[test]
fn scale_rotation_and_crop_match_the_reference() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let source = noise(200, 150, 21, true);
    let base = noise(320, 240, 22, false);
    for (label, scale, rotation, crop) in [
        ("upscale", 1.7_f32, 0.0_f32, CropRect::FULL),
        ("downscale", 0.45, 0.0, CropRect::FULL),
        ("rotate", 1.0, 30.0, CropRect::FULL),
        ("rotate+scale", 1.3, -73.5, CropRect::FULL),
        (
            "crop",
            1.0,
            0.0,
            CropRect {
                x: 0.2,
                y: 0.1,
                width: 0.5,
                height: 0.6,
            },
        ),
        (
            "crop+rotate+scale",
            0.8,
            15.0,
            CropRect {
                x: 0.1,
                y: 0.25,
                width: 0.7,
                height: 0.5,
            },
        ),
    ] {
        let mut layer = LayerFrame::new(&source);
        layer.x = 60;
        layer.y = 40;
        layer.transform.scale_x = scale;
        layer.transform.scale_y = scale;
        layer.transform.rotation_deg = rotation;
        layer.transform.crop = crop;
        let comparison = compare(&mut gpu, 320, 240, &[LayerFrame::new(&base), layer]);
        // Transformed sampling depends on float rounding at pixel centres, so allow a thin
        // fringe of different pixels but no large errors.
        assert_parity(label, &comparison, 255, 0.01);
    }
}

#[test]
fn colour_effects_chroma_mask_pixelize_and_lut_match_the_reference() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let base = noise(256, 192, 31, false);
    let source = noise(256, 192, 32, true);

    let table_size = 9;
    let mut entries = Vec::new();
    let mut state = 0xC0FFEE_u64;
    for _ in 0..table_size * table_size * table_size {
        let mut entry = [0.0_f32; 3];
        for channel in &mut entry {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *channel = (state % 1000) as f32 / 1000.0;
        }
        entries.push(entry);
    }
    let lut = Lut3D::new(table_size, entries).unwrap();

    let cases: Vec<(&str, LayerEffects, Option<&Lut3D>)> = vec![
        (
            "colour",
            LayerEffects {
                brightness: 0.1,
                contrast: 1.3,
                saturation: 0.6,
                vignette: 0.5,
                ..LayerEffects::default()
            },
            None,
        ),
        (
            "chroma key",
            LayerEffects {
                chroma_key: Some(ChromaKey {
                    color: [128, 128, 128],
                    tolerance: 0.15,
                    softness: 0.2,
                }),
                ..LayerEffects::default()
            },
            None,
        ),
        (
            "ellipse mask",
            LayerEffects {
                mask: Some(LayerMask {
                    shape: MaskShape::Ellipse,
                    corner_radius: 0.0,
                    feather: 0.25,
                    invert: false,
                }),
                ..LayerEffects::default()
            },
            None,
        ),
        (
            "rounded rectangle mask",
            LayerEffects {
                mask: Some(LayerMask {
                    shape: MaskShape::Rectangle,
                    corner_radius: 0.5,
                    feather: 0.0,
                    invert: true,
                }),
                ..LayerEffects::default()
            },
            None,
        ),
        (
            "pixelize",
            LayerEffects {
                pixelize_block: 9,
                ..LayerEffects::default()
            },
            None,
        ),
        (
            "sepia",
            LayerEffects {
                sepia: 0.8,
                ..LayerEffects::default()
            },
            None,
        ),
        ("lut", LayerEffects::default(), Some(&lut)),
        (
            "everything",
            LayerEffects {
                brightness: -0.05,
                contrast: 0.9,
                saturation: 1.4,
                vignette: 0.3,
                sepia: 0.4,
                blur_sigma: 2.5,
                sharpen: 0.8,
                pixelize_block: 4,
                chroma_key: Some(ChromaKey {
                    color: [10, 200, 30],
                    tolerance: 0.3,
                    softness: 0.1,
                }),
                mask: Some(LayerMask {
                    shape: MaskShape::Ellipse,
                    corner_radius: 0.0,
                    feather: 0.1,
                    invert: false,
                }),
            },
            Some(&lut),
        ),
    ];
    for (label, effects, lut) in cases {
        let mut layer = LayerFrame::new(&source);
        layer.effects = effects;
        layer.lut = lut;
        layer.opacity = 0.85;
        let comparison = compare(&mut gpu, 256, 192, &[LayerFrame::new(&base), layer]);
        assert_parity(label, &comparison, 2, 0.01);
    }
}

#[test]
fn padded_strides_off_canvas_layers_and_empty_scenes_match_the_reference() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    // A frame whose rows are padded: stride 4 pixels wider than the image.
    let (width, height, stride) = (50_u32, 40_u32, 54_usize * 4);
    let mut padded = vec![7_u8; stride * height as usize];
    for row in 0..height as usize {
        for column in 0..width as usize {
            let at = row * stride + column * 4;
            padded[at..at + 4].copy_from_slice(&[(column * 5) as u8, (row * 6) as u8, 200, 255]);
        }
    }
    let padded = VideoFrame::new(PixelFormat::Rgba8, width, height, stride, 0.0, padded).unwrap();
    let off = noise(30, 30, 41, false);
    let base = noise(96, 64, 42, false);

    let mut placed = LayerFrame::new(&padded);
    placed.x = 10;
    placed.y = 8;
    let mut away = LayerFrame::new(&off);
    away.x = 5_000;
    let mut partial = LayerFrame::new(&off);
    partial.x = -15;
    partial.y = 50;
    let comparison = compare(
        &mut gpu,
        96,
        64,
        &[LayerFrame::new(&base), placed, away, partial],
    );
    assert_parity("padded + off-canvas", &comparison, 0, 0.0);

    let empty = compare(&mut gpu, 32, 32, &[]);
    assert_parity("empty scene", &empty, 0, 0.0);
}

#[test]
fn gpu_rejects_the_same_invalid_input_as_the_reference() {
    let Some(mut gpu) = gpu(1024) else { return };
    let source = noise(2, 2, 51, false);
    assert_eq!(
        gpu.render(0, 4, 0.0, &[]).unwrap_err(),
        RenderError::ZeroCanvas
    );
    let mut bad = LayerFrame::new(&source);
    bad.opacity = 2.0;
    assert_eq!(
        gpu.render(2, 2, 0.0, &[bad]).unwrap_err(),
        RenderError::InvalidOpacity(2.0)
    );
    assert!(matches!(
        gpu.render(64, 64, 0.0, &[]).unwrap_err(),
        RenderError::OutputExceedsBudget { .. }
    ));
}

#[test]
fn consecutive_renders_with_changing_sizes_stay_correct() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let source = noise(100, 100, 61, true);
    for (width, height) in [(64_u32, 48_u32), (200, 120), (64, 48), (33, 17)] {
        let background = noise(width, height, u64::from(width), false);
        let mut layer = LayerFrame::new(&source);
        layer.x = 5;
        let comparison = compare(
            &mut gpu,
            width,
            height,
            &[LayerFrame::new(&background), layer],
        );
        assert_parity(&format!("{width}x{height}"), &comparison, 0, 0.0);
    }
}

#[test]
fn engine_with_the_gpu_renderer_matches_the_cpu_engine() {
    use std::path::PathBuf;
    use std::time::Duration;

    use bush::{
        Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
        EngineConfig, EngineEvent, EngineRequest, MediaSource, Placement, RendererChoice,
        SceneSnapshot, SceneVersions, SourceRange, Version,
    };

    let fixture =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4");
    let info = DecodeSource::open(&fixture).unwrap().info();
    let render = |choice: RendererChoice| {
        let engine = Engine::new(EngineConfig {
            canvas_width: info.width,
            canvas_height: info.height,
            max_open_sources: 2,
            frame_cache_bytes: 64 * 1024 * 1024,
            decode_policy: DecodePolicy::Software,
            renderer: choice,
        })
        .unwrap();
        let name = engine.renderer_name().to_string();
        let contributor = |id: u64, layer: i32, opacity: f32| Contributor {
            id: ContributorId(id),
            layer,
            visible: true,
            audible: false,
            content: ContributorContent::Media(MediaSource {
                path: fixture.clone(),
                range: SourceRange::new(0.0, 0.9).unwrap(),
                version: Version(1),
            }),
            placement: Placement {
                opacity,
                x: if id == 2 { 20 } else { 0 },
                ..Placement::default()
            },
        };
        let scene = SceneSnapshot::new(
            SceneVersions::default(),
            vec![contributor(1, 0, 1.0), contributor(2, 1, 0.5)],
        )
        .unwrap();
        engine
            .submit(EngineRequest {
                scene,
                playhead_secs: 0.4,
                overlays: Vec::new(),
            })
            .unwrap();
        match engine.wait(Duration::from_secs(30)).unwrap() {
            EngineEvent::Frame { frame, .. } => (name, frame),
            EngineEvent::Failed { message, .. } => panic!("{message}"),
        }
    };

    let (cpu_name, cpu_frame) = render(RendererChoice::Cpu);
    let (gpu_name, gpu_frame) = render(RendererChoice::GpuWithCpuFallback);
    assert_eq!(cpu_name, "cpu");
    eprintln!("engine renderer names: {cpu_name} / {gpu_name}");
    if !gpu_name.starts_with("gpu:") {
        eprintln!("skipped: no usable GPU adapter, the engine fell back to the CPU");
        return;
    }
    let max_difference = cpu_frame
        .pixels()
        .iter()
        .zip(gpu_frame.pixels())
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap();
    assert!(
        max_difference <= 1,
        "engine GPU vs CPU max difference {max_difference}"
    );
}

#[test]
fn flips_match_the_reference_alone_and_with_other_transforms() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let base = noise(200, 150, 71, false);
    let source = noise(120, 90, 72, true);
    for (label, flip_x, flip_y, rotation, scale) in [
        ("flip x", true, false, 0.0_f32, 1.0_f32),
        ("flip y", false, true, 0.0, 1.0),
        ("flip both", true, true, 0.0, 1.0),
        ("flip x + scale", true, false, 0.0, 1.6),
        ("flip y + rotate", false, true, 25.0, 1.0),
    ] {
        let mut layer = LayerFrame::new(&source);
        layer.x = 30;
        layer.y = 20;
        layer.transform.flip_x = flip_x;
        layer.transform.flip_y = flip_y;
        layer.transform.rotation_deg = rotation;
        layer.transform.scale_x = scale;
        layer.transform.scale_y = scale;
        let comparison = compare(&mut gpu, 200, 150, &[LayerFrame::new(&base), layer]);
        assert_parity(label, &comparison, 255, 0.01);
    }
}

#[test]
fn gpu_reproduces_every_blend_mode_table() {
    let Some(mut gpu) = gpu(1 << 24) else { return };
    let mut source_pixels = Vec::new();
    let mut backdrop_pixels = Vec::new();
    for y in 0..256u32 {
        for x in 0..256u32 {
            source_pixels.extend_from_slice(&[x as u8, x as u8, x as u8, 255]);
            backdrop_pixels.extend_from_slice(&[y as u8, y as u8, y as u8, 255]);
        }
    }
    let source = VideoFrame::new(PixelFormat::Rgba8, 256, 256, 1024, 0.0, source_pixels).unwrap();
    let backdrop =
        VideoFrame::new(PixelFormat::Rgba8, 256, 256, 1024, 0.0, backdrop_pixels).unwrap();
    for mode in BlendMode::ALL {
        let mut top = LayerFrame::new(&source);
        top.blend = mode;
        let comparison = compare(&mut gpu, 256, 256, &[LayerFrame::new(&backdrop), top]);
        // Integer modes are exact; the two float-evaluated ones may differ by one level where
        // CPU and GPU round a cosine or square root differently.
        let (max_difference, max_fraction) = match mode {
            BlendMode::Interpolate | BlendMode::Geometric => (1, 0.005),
            _ => (0, 0.0),
        };
        assert_parity(
            mode.ffmpeg_name(),
            &comparison,
            max_difference,
            max_fraction,
        );
    }
}

#[test]
fn gpu_blur_and_sharpen_match_the_cpu_reference() {
    let Some(mut gpu) = gpu(1 << 26) else { return };
    let base = noise(300, 220, 81, false);
    let opaque = noise(300, 220, 82, false);
    let translucent = noise(300, 220, 83, true);
    let tiny = noise(9, 7, 84, true);

    for (label, source, sigma, sharpen) in [
        ("blur 0.8", &opaque, 0.8_f32, 0.0_f32),
        ("blur 3", &opaque, 3.0, 0.0),
        ("blur 12", &opaque, 12.0, 0.0),
        ("blur 64", &opaque, 64.0, 0.0),
        ("blur translucent", &translucent, 5.0, 0.0),
        ("sharpen", &opaque, 0.0, 1.2),
        ("blur + sharpen", &opaque, 4.0, 0.9),
        ("sharpen translucent", &translucent, 0.0, 2.0),
        ("tiny frame", &tiny, 6.0, 1.0),
    ] {
        let mut layer = LayerFrame::new(source);
        layer.effects.blur_sigma = sigma;
        layer.effects.sharpen = sharpen;
        let comparison = compare(&mut gpu, 300, 220, &[LayerFrame::new(&base), layer]);
        // The blur is integer arithmetic end to end (exact); the unsharp blend is float.
        let allowed = if sharpen > 0.0 { 1 } else { 0 };
        assert_parity(
            label,
            &comparison,
            allowed,
            if sharpen > 0.0 { 0.01 } else { 0.0 },
        );
    }
}

#[test]
fn gpu_blur_handles_several_layers_with_different_sizes_in_one_frame() {
    let Some(mut gpu) = gpu(1 << 26) else { return };
    let base = noise(256, 192, 91, false);
    let first = noise(200, 120, 92, true);
    let second = noise(64, 64, 93, false);
    let mut blurred = LayerFrame::new(&first);
    blurred.x = 20;
    blurred.effects.blur_sigma = 7.0;
    let mut plain = LayerFrame::new(&second);
    plain.x = 120;
    plain.y = 100;
    let mut sharpened = LayerFrame::new(&second);
    sharpened.x = 10;
    sharpened.y = 120;
    sharpened.effects.sharpen = 1.5;
    sharpened.effects.blur_sigma = 1.0;
    let comparison = compare(
        &mut gpu,
        256,
        192,
        &[LayerFrame::new(&base), blurred, plain, sharpened],
    );
    assert_parity("mixed layers", &comparison, 1, 0.01);
}
