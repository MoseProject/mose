use bush::{
    BlendMode, CpuRenderer, CropRect, LayerEffects, LayerFrame, LayerTransform, PixelFormat,
    RenderError, VideoFrame,
};

fn frame(width: u32, height: u32, pixels: Vec<u8>) -> VideoFrame {
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

#[test]
fn composites_layers_in_painter_order() {
    let red = frame(1, 1, vec![255, 0, 0, 255]);
    let blue = frame(1, 1, vec![0, 0, 255, 255]);
    let renderer = CpuRenderer::new(16).unwrap();

    let output = renderer
        .render(
            1,
            1,
            0.0,
            &[
                LayerFrame {
                    frame: &red,
                    x: 0,
                    y: 0,
                    opacity: 1.0,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 1.0,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                },
            ],
        )
        .unwrap();

    assert_eq!(output.pixels(), &[0, 0, 255, 255]);
}

#[test]
fn straight_alpha_and_layer_opacity_blend_deterministically() {
    let red = frame(1, 1, vec![255, 0, 0, 255]);
    let blue = frame(1, 1, vec![0, 0, 255, 128]);
    let renderer = CpuRenderer::new(16).unwrap();

    let output = renderer
        .render(
            1,
            1,
            0.0,
            &[
                LayerFrame {
                    frame: &red,
                    x: 0,
                    y: 0,
                    opacity: 1.0,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 0.5,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                },
            ],
        )
        .unwrap();

    assert_eq!(output.pixels(), &[191, 0, 64, 255]);
}

#[test]
fn clips_layers_outside_the_canvas() {
    let green = frame(2, 1, vec![0, 255, 0, 255, 0, 255, 0, 255]);
    let renderer = CpuRenderer::new(16).unwrap();

    let output = renderer
        .render(
            1,
            1,
            0.0,
            &[LayerFrame {
                frame: &green,
                x: -1,
                y: 0,
                opacity: 1.0,
                transform: LayerTransform::default(),
                blend: BlendMode::Normal,
                effects: LayerEffects::default(),
            }],
        )
        .unwrap();

    assert_eq!(output.pixels(), &[0, 255, 0, 255]);
}

#[test]
fn rejects_invalid_render_input_before_allocating() {
    let renderer = CpuRenderer::new(16).unwrap();
    let source = frame(1, 1, vec![0, 0, 0, 0]);

    assert_eq!(
        renderer.render(0, 1, 0.0, &[]),
        Err(RenderError::ZeroCanvas)
    );
    assert!(matches!(
        renderer.render(1, 1, f64::NAN, &[]),
        Err(RenderError::NonFiniteTimestamp(value)) if value.is_nan()
    ));
    assert_eq!(
        renderer.render(
            1,
            1,
            0.0,
            &[LayerFrame {
                frame: &source,
                x: 0,
                y: 0,
                opacity: 1.1,
                transform: LayerTransform::default(),
                blend: BlendMode::Normal,
                effects: LayerEffects::default(),
            }]
        ),
        Err(RenderError::InvalidOpacity(1.1))
    );
    assert_eq!(
        renderer.render(3, 2, 0.0, &[]),
        Err(RenderError::OutputExceedsBudget {
            requested_bytes: 24,
            budget_bytes: 16
        })
    );
}

fn solid(width: u32, height: u32, rgba: [u8; 4]) -> VideoFrame {
    frame(width, height, rgba.repeat((width * height) as usize))
}

fn pixel(output: &VideoFrame, x: usize, y: usize) -> [u8; 4] {
    let i = y * output.stride_bytes() + x * 4;
    output.pixels()[i..i + 4].try_into().unwrap()
}

#[test]
fn blend_modes_follow_the_w3c_formulas() {
    let backdrop = solid(1, 1, [200, 100, 50, 255]);
    let source = solid(1, 1, [100, 100, 100, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let render = |blend| {
        let mut top = LayerFrame::new(&source);
        top.blend = blend;
        renderer
            .render(1, 1, 0.0, &[LayerFrame::new(&backdrop), top])
            .unwrap()
            .pixels()
            .to_vec()
    };

    assert_eq!(render(BlendMode::Normal), [100, 100, 100, 255]);
    assert_eq!(render(BlendMode::Multiply), [78, 39, 20, 255]);
    assert_eq!(render(BlendMode::Screen), [222, 161, 130, 255]);
    assert_eq!(render(BlendMode::Darken), [100, 100, 50, 255]);
    assert_eq!(render(BlendMode::Lighten), [200, 100, 100, 255]);
    assert_eq!(render(BlendMode::Add), [255, 200, 150, 255]);
    assert_eq!(render(BlendMode::Difference), [100, 0, 50, 255]);
}

#[test]
fn blend_over_transparent_backdrop_keeps_the_source_colour() {
    let source = solid(1, 1, [10, 200, 30, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let mut top = LayerFrame::new(&source);
    top.blend = BlendMode::Multiply;
    let output = renderer.render(1, 1, 0.0, &[top]).unwrap();
    assert_eq!(output.pixels(), &[10, 200, 30, 255]);
}

#[test]
fn scale_doubles_each_source_pixel() {
    let source = frame(2, 1, vec![255, 0, 0, 255, 0, 0, 255, 255]);
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut top = LayerFrame::new(&source);
    top.transform.scale_x = 2.0;
    top.transform.scale_y = 2.0;
    let output = renderer.render(4, 2, 0.0, &[top]).unwrap();

    for y in 0..2 {
        assert_eq!(pixel(&output, 0, y), [255, 0, 0, 255]);
        assert_eq!(pixel(&output, 1, y), [255, 0, 0, 255]);
        assert_eq!(pixel(&output, 2, y), [0, 0, 255, 255]);
        assert_eq!(pixel(&output, 3, y), [0, 0, 255, 255]);
    }
}

#[test]
fn crop_selects_a_source_region_and_places_it_at_the_layer_origin() {
    let source = frame(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]);
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut top = LayerFrame::new(&source);
    top.transform.crop = CropRect {
        x: 0.5,
        y: 0.0,
        width: 0.5,
        height: 1.0,
    };
    let output = renderer.render(2, 1, 0.0, &[top]).unwrap();

    assert_eq!(pixel(&output, 0, 0), [0, 255, 0, 255]);
    assert_eq!(pixel(&output, 1, 0), [0, 0, 0, 0]);
}

#[test]
fn rotation_by_90_degrees_turns_a_row_into_a_column() {
    // A 3x1 strip R,G,B placed so its centre is canvas pixel (0,1), rotated clockwise 90
    // degrees: a vertical column with R at the top.
    let source = frame(3, 1, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255]);
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut top = LayerFrame::new(&source);
    top.x = -1;
    top.y = 1;
    top.transform.rotation_deg = 90.0;
    let output = renderer.render(1, 3, 0.0, &[top]).unwrap();

    assert_eq!(pixel(&output, 0, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&output, 0, 1), [0, 255, 0, 255]);
    assert_eq!(pixel(&output, 0, 2), [0, 0, 255, 255]);
}

#[test]
fn invalid_transforms_are_rejected() {
    let source = solid(1, 1, [0, 0, 0, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let mutations: [fn(&mut LayerTransform); 5] = [
        |t| t.scale_x = 0.0,
        |t| t.scale_y = f32::NAN,
        |t| t.rotation_deg = f32::INFINITY,
        |t| t.crop.width = 0.0,
        |t| t.crop.x = 0.8,
    ];
    for mutate in mutations {
        let mut top = LayerFrame::new(&source);
        mutate(&mut top.transform);
        assert_eq!(
            renderer.render(1, 1, 0.0, &[top]).unwrap_err(),
            RenderError::InvalidTransform
        );
    }
}

#[test]
fn colour_effects_follow_their_formulas() {
    let source = solid(1, 1, [100, 150, 200, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let render = |effects: LayerEffects| {
        let mut layer = LayerFrame::new(&source);
        layer.effects = effects;
        renderer
            .render(1, 1, 0.0, &[layer])
            .unwrap()
            .pixels()
            .to_vec()
    };

    assert_eq!(render(LayerEffects::default()), [100, 150, 200, 255]);
    // +0.2 brightness adds exactly 51 per channel.
    assert_eq!(
        render(LayerEffects {
            brightness: 0.2,
            ..LayerEffects::default()
        }),
        [151, 201, 251, 255]
    );
    // Contrast 1.5: (c - 0.5) * 1.5 + 0.5.
    assert_eq!(
        render(LayerEffects {
            contrast: 1.5,
            ..LayerEffects::default()
        }),
        [86, 161, 236, 255]
    );
    // Zero saturation collapses to Rec.709 luma.
    let grey = render(LayerEffects {
        saturation: 0.0,
        ..LayerEffects::default()
    });
    assert_eq!(grey[0], grey[1]);
    assert_eq!(grey[1], grey[2]);
    assert_eq!(grey[0], 143);
}

#[test]
fn vignette_darkens_corners_but_not_the_centre() {
    let source = solid(9, 9, [200, 200, 200, 255]);
    let renderer = CpuRenderer::new(9 * 9 * 4).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.effects.vignette = 1.0;
    let output = renderer.render(9, 9, 0.0, &[layer]).unwrap();

    assert_eq!(pixel(&output, 4, 4)[0], 200);
    assert!(
        pixel(&output, 0, 0)[0] < 60,
        "corner is {:?}",
        pixel(&output, 0, 0)
    );
    assert!(pixel(&output, 4, 0)[0] < pixel(&output, 4, 4)[0]);
}

#[test]
fn out_of_range_effects_are_rejected() {
    let source = solid(1, 1, [0, 0, 0, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    for effects in [
        LayerEffects {
            brightness: 2.0,
            ..LayerEffects::default()
        },
        LayerEffects {
            contrast: -1.0,
            ..LayerEffects::default()
        },
        LayerEffects {
            saturation: f32::NAN,
            ..LayerEffects::default()
        },
        LayerEffects {
            vignette: 1.5,
            ..LayerEffects::default()
        },
    ] {
        let mut layer = LayerFrame::new(&source);
        layer.effects = effects;
        assert_eq!(
            renderer.render(1, 1, 0.0, &[layer]).unwrap_err(),
            RenderError::InvalidEffects
        );
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
    frame(width, height, pixels)
}

/// Independent per-pixel W3C normal-blend compositor used as an oracle for the optimized,
/// parallel renderer.
fn naive_normal_composite(
    width: u32,
    height: u32,
    layers: &[(&VideoFrame, i32, i32, f32)],
) -> Vec<u8> {
    let mut canvas = vec![0u8; (width * height * 4) as usize];
    for (source, ox, oy, opacity) in layers {
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let (sx, sy) = (x - ox, y - oy);
                if sx < 0 || sy < 0 || sx >= source.width() as i32 || sy >= source.height() as i32 {
                    continue;
                }
                let s = (sy as usize * source.stride_bytes()) + sx as usize * 4;
                let d = (y as usize * width as usize + x as usize) * 4;
                let src = &source.pixels()[s..s + 4];
                let alpha_s = src[3] as f32 / 255.0 * opacity;
                if alpha_s == 0.0 {
                    continue;
                }
                let alpha_b = canvas[d + 3] as f32 / 255.0;
                let alpha_o = alpha_s + alpha_b * (1.0 - alpha_s);
                for c in 0..3 {
                    let cs = src[c] as f32 / 255.0;
                    let cb = canvas[d + c] as f32 / 255.0;
                    let premultiplied = alpha_s * (1.0 - alpha_b) * cs
                        + alpha_s * alpha_b * cs
                        + (1.0 - alpha_s) * alpha_b * cb;
                    canvas[d + c] = ((premultiplied / alpha_o) * 255.0).round() as u8;
                }
                canvas[d + 3] = (alpha_o * 255.0).round() as u8;
            }
        }
    }
    canvas
}

#[test]
fn parallel_fast_path_matches_an_independent_oracle() {
    // 320x300 = 96k pixels: above the parallel threshold, rows do not divide evenly.
    let (width, height) = (320, 300);
    let background = noise(width, height, 0x1234_5678_9ABC_DEF1, false);
    let translucent = noise(200, 180, 0x0FED_CBA9_8765_4321, true);
    let faded = noise(150, 150, 0x1357_9BDF_2468_ACE1, true);
    let oracle = naive_normal_composite(
        width,
        height,
        &[
            (&background, 0, 0, 1.0),
            (&translucent, 40, 25, 1.0),
            (&faded, -30, 160, 0.6),
        ],
    );

    let renderer = CpuRenderer::new((width * height * 4) as usize).unwrap();
    let mut second = LayerFrame::new(&translucent);
    second.x = 40;
    second.y = 25;
    let mut third = LayerFrame::new(&faded);
    third.x = -30;
    third.y = 160;
    third.opacity = 0.6;
    let output = renderer
        .render(
            width,
            height,
            0.0,
            &[LayerFrame::new(&background), second, third],
        )
        .unwrap();

    assert_eq!(output.pixels(), &oracle[..]);
}

#[test]
fn identity_fast_path_equals_the_general_mapping_path() {
    let (width, height) = (300, 300);
    let background = noise(width, height, 0xDEAD_BEEF_CAFE_F00D, true);
    let overlay = noise(120, 90, 0xABAD_1DEA_5EED_0001, true);
    let renderer = CpuRenderer::new((width * height * 4) as usize).unwrap();

    let render = |forced_general: bool| {
        let mut top = LayerFrame::new(&overlay);
        top.x = 33;
        top.y = 47;
        top.opacity = 0.75;
        // A contrast that rounds back to the same bytes defeats the identity shortcut only.
        if forced_general {
            top.effects.contrast = 1.000_000_1;
        }
        renderer
            .render(width, height, 0.0, &[LayerFrame::new(&background), top])
            .unwrap()
            .pixels()
            .to_vec()
    };

    assert_eq!(render(false), render(true));
}
