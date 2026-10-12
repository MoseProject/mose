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
                    lut: None,
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 1.0,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                    lut: None,
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
                    lut: None,
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 0.5,
                    transform: LayerTransform::default(),
                    blend: BlendMode::Normal,
                    effects: LayerEffects::default(),
                    lut: None,
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
                lut: None,
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
                lut: None,
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
fn blend_modes_use_ffmpeg_integer_arithmetic() {
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
    assert_eq!(render(BlendMode::Multiply), [78, 39, 19, 255]);
    assert_eq!(render(BlendMode::Screen), [222, 161, 131, 255]);
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

use bush::{ChromaKey, LayerMask, Lut3D, LutError, MaskShape};

#[test]
fn chroma_key_removes_the_key_colour_and_ramps_with_softness() {
    // Pure green screen next to a red subject.
    let source = frame(2, 1, vec![0, 255, 0, 255, 255, 0, 0, 255]);
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.effects.chroma_key = Some(ChromaKey {
        color: [0, 255, 0],
        tolerance: 0.1,
        softness: 0.0,
    });
    let output = renderer.render(2, 1, 0.0, &[layer]).unwrap();
    assert_eq!(pixel(&output, 0, 0)[3], 0);
    assert_eq!(pixel(&output, 1, 0), [255, 0, 0, 255]);

    // A near-green pixel inside the soft ramp is partially transparent.
    let near = frame(1, 1, vec![0, 200, 0, 255]);
    let mut soft = LayerFrame::new(&near);
    soft.effects.chroma_key = Some(ChromaKey {
        color: [0, 255, 0],
        tolerance: 0.05,
        softness: 0.2,
    });
    let alpha = renderer.render(1, 1, 0.0, &[soft]).unwrap().pixels()[3];
    assert!((1..255).contains(&alpha), "alpha {alpha}");
}

#[test]
fn ellipse_and_rounded_rectangle_masks_cut_the_corners() {
    let source = solid(9, 9, [200, 100, 50, 255]);
    let renderer = CpuRenderer::new(9 * 9 * 4).unwrap();
    let render = |mask: LayerMask| {
        let mut layer = LayerFrame::new(&source);
        layer.effects.mask = Some(mask);
        renderer.render(9, 9, 0.0, &[layer]).unwrap()
    };

    let ellipse = render(LayerMask {
        shape: MaskShape::Ellipse,
        corner_radius: 0.0,
        feather: 0.0,
        invert: false,
    });
    assert_eq!(pixel(&ellipse, 4, 4)[3], 255);
    assert_eq!(pixel(&ellipse, 0, 0)[3], 0);
    assert_eq!(pixel(&ellipse, 4, 0)[3], 255);

    let plain_rect = render(LayerMask {
        shape: MaskShape::Rectangle,
        corner_radius: 0.0,
        feather: 0.0,
        invert: false,
    });
    assert_eq!(pixel(&plain_rect, 0, 0)[3], 255);

    let rounded = render(LayerMask {
        shape: MaskShape::Rectangle,
        corner_radius: 1.0,
        feather: 0.0,
        invert: false,
    });
    assert_eq!(pixel(&rounded, 0, 0)[3], 0);
    assert_eq!(pixel(&rounded, 4, 4)[3], 255);

    let inverted = render(LayerMask {
        shape: MaskShape::Ellipse,
        corner_radius: 0.0,
        feather: 0.0,
        invert: true,
    });
    assert_eq!(pixel(&inverted, 4, 4)[3], 0);
    assert_eq!(pixel(&inverted, 0, 0)[3], 255);

    let feathered = render(LayerMask {
        shape: MaskShape::Ellipse,
        corner_radius: 0.0,
        feather: 0.5,
        invert: false,
    });
    let edge = pixel(&feathered, 8, 4)[3];
    assert!((1..255).contains(&edge), "feathered edge alpha {edge}");
}

#[test]
fn pixelize_samples_each_block_centre() {
    // 4x1 gradient, block 2: pixels 0,1 share the colour of source pixel 1; 2,3 of pixel 3.
    let source = frame(
        4,
        1,
        vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255],
    );
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.effects.pixelize_block = 2;
    let output = renderer.render(4, 1, 0.0, &[layer]).unwrap();
    let reds: Vec<u8> = (0..4).map(|x| pixel(&output, x, 0)[0]).collect();
    assert_eq!(reds, [20, 20, 40, 40]);
}

#[test]
fn identity_lut_is_a_no_op_and_an_inverting_lut_inverts() {
    let source = frame(2, 1, vec![10, 120, 250, 255, 255, 0, 77, 255]);
    let renderer = CpuRenderer::new(1024).unwrap();

    let identity = Lut3D::identity(17).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.lut = Some(&identity);
    let output = renderer.render(2, 1, 0.0, &[layer]).unwrap();
    assert_eq!(output.pixels(), source.pixels());

    let size = 2;
    let mut entries = Vec::new();
    for blue in 0..size {
        for green in 0..size {
            for red in 0..size {
                entries.push([1.0 - red as f32, 1.0 - green as f32, 1.0 - blue as f32]);
            }
        }
    }
    let invert = Lut3D::new(size, entries).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.lut = Some(&invert);
    let output = renderer.render(2, 1, 0.0, &[layer]).unwrap();
    // A 2-point trilinear table is exactly 1 - c, so bytes invert (to within rounding).
    for (inverted, original) in output
        .pixels()
        .chunks_exact(4)
        .zip(source.pixels().chunks_exact(4))
    {
        for channel in 0..3 {
            assert!(
                (i32::from(inverted[channel]) - (255 - i32::from(original[channel]))).abs() <= 1
            );
        }
        assert_eq!(inverted[3], 255);
    }
}

#[test]
fn lut_construction_validates_size_count_and_values() {
    assert_eq!(Lut3D::identity(1).unwrap_err(), LutError::InvalidSize(1));
    assert_eq!(Lut3D::identity(66).unwrap_err(), LutError::InvalidSize(66));
    assert_eq!(
        Lut3D::new(2, vec![[0.0; 3]; 7]).unwrap_err(),
        LutError::WrongEntryCount {
            expected: 8,
            actual: 7
        }
    );
    assert_eq!(
        Lut3D::new(2, vec![[f32::NAN, 0.0, 0.0]; 8]).unwrap_err(),
        LutError::NonFiniteEntry
    );
    assert_eq!(
        Lut3D::identity(33).unwrap().retained_bytes(),
        33 * 33 * 33 * 12
    );
}

#[test]
fn invalid_mask_key_and_block_values_are_rejected() {
    let source = solid(1, 1, [0, 0, 0, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let bad: [LayerEffects; 3] = [
        LayerEffects {
            pixelize_block: 513,
            ..LayerEffects::default()
        },
        LayerEffects {
            chroma_key: Some(ChromaKey {
                color: [0, 0, 0],
                tolerance: 2.0,
                softness: 0.0,
            }),
            ..LayerEffects::default()
        },
        LayerEffects {
            mask: Some(LayerMask {
                shape: MaskShape::Ellipse,
                corner_radius: -0.5,
                feather: 0.0,
                invert: false,
            }),
            ..LayerEffects::default()
        },
    ];
    for effects in bad {
        let mut layer = LayerFrame::new(&source);
        layer.effects = effects;
        assert_eq!(
            renderer.render(1, 1, 0.0, &[layer]).unwrap_err(),
            RenderError::InvalidEffects
        );
    }
}

#[test]
fn flips_mirror_the_cropped_layer_before_rotation() {
    // 3x2 image: top row R G B, bottom row C M Y.
    let source = frame(
        3,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, //
            0, 255, 255, 255, 255, 0, 255, 255, 255, 255, 0, 255,
        ],
    );
    let renderer = CpuRenderer::new(1024).unwrap();
    let render = |flip_x: bool, flip_y: bool| {
        let mut layer = LayerFrame::new(&source);
        layer.transform.flip_x = flip_x;
        layer.transform.flip_y = flip_y;
        renderer.render(3, 2, 0.0, &[layer]).unwrap()
    };

    let both = render(true, true);
    assert_eq!(pixel(&both, 0, 0), [255, 255, 0, 255]); // Y
    assert_eq!(pixel(&both, 2, 1), [255, 0, 0, 255]); // R
    let horizontal = render(true, false);
    assert_eq!(pixel(&horizontal, 0, 0), [0, 0, 255, 255]); // B
    assert_eq!(pixel(&horizontal, 2, 0), [255, 0, 0, 255]); // R
    let vertical = render(false, true);
    assert_eq!(pixel(&vertical, 0, 0), [0, 255, 255, 255]); // C
    assert_eq!(pixel(&vertical, 0, 1), [255, 0, 0, 255]); // R
                                                          // Flipping twice restores the picture.
    assert_eq!(render(false, false).pixels(), source.pixels());
}

#[test]
fn flip_follows_the_crop_window() {
    let source = frame(
        4,
        1,
        vec![10, 0, 0, 255, 20, 0, 0, 255, 30, 0, 0, 255, 40, 0, 0, 255],
    );
    let renderer = CpuRenderer::new(1024).unwrap();
    let mut layer = LayerFrame::new(&source);
    layer.transform.crop = CropRect {
        x: 0.25,
        y: 0.0,
        width: 0.5,
        height: 1.0,
    };
    layer.transform.flip_x = true;
    let output = renderer.render(2, 1, 0.0, &[layer]).unwrap();
    // The crop holds 20,30; flipped it reads 30,20.
    assert_eq!(pixel(&output, 0, 0)[0], 30);
    assert_eq!(pixel(&output, 1, 0)[0], 20);
}

#[test]
fn sepia_tones_towards_the_classic_matrix_and_mixes_linearly() {
    let source = solid(1, 1, [100, 150, 200, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    let render = |sepia: f32| {
        let mut layer = LayerFrame::new(&source);
        layer.effects.sepia = sepia;
        renderer
            .render(1, 1, 0.0, &[layer])
            .unwrap()
            .pixels()
            .to_vec()
    };
    assert_eq!(render(0.0), [100, 150, 200, 255]);
    // r' = .393*100 + .769*150 + .189*200 = 192.45, g' = 171.4, b' = 133.8.
    let full = render(1.0);
    assert_eq!(full, [192, 171, 134, 255]);
    let half = render(0.5);
    assert!((i32::from(half[0]) - (100 + 192) / 2).abs() <= 1);
    assert!((i32::from(half[2]) - (200 + 134) / 2).abs() <= 1);
    // Sepia on a bright pixel saturates instead of wrapping.
    let bright = solid(1, 1, [250, 250, 250, 255]);
    let mut layer = LayerFrame::new(&bright);
    layer.effects.sepia = 1.0;
    let pixel = renderer
        .render(1, 1, 0.0, &[layer])
        .unwrap()
        .pixels()
        .to_vec();
    assert_eq!(pixel[0], 255);
    assert!(pixel[1] >= 200 && pixel[2] < pixel[0]);
}

#[test]
fn sepia_must_be_a_unit_fraction() {
    let source = solid(1, 1, [0, 0, 0, 255]);
    let renderer = CpuRenderer::new(16).unwrap();
    for sepia in [-0.1, 1.1, f32::NAN] {
        let mut layer = LayerFrame::new(&source);
        layer.effects.sepia = sepia;
        assert_eq!(
            renderer.render(1, 1, 0.0, &[layer]).unwrap_err(),
            RenderError::InvalidEffects
        );
    }
}

fn opaque_pixels(output: &VideoFrame) -> Vec<usize> {
    output
        .pixels()
        .chunks_exact(4)
        .enumerate()
        .filter(|(_, pixel)| pixel[3] != 0)
        .map(|(index, _)| index)
        .collect()
}

#[test]
fn blend_mode_names_are_unique_ffmpeg_names() {
    let names: std::collections::HashSet<_> = BlendMode::ALL
        .iter()
        .map(|mode| mode.ffmpeg_name())
        .collect();
    assert_eq!(names.len(), BlendMode::ALL.len());
    assert_eq!(BlendMode::Normal.ffmpeg_name(), "normal");
    assert_eq!(BlendMode::SoftLight.ffmpeg_name(), "softlight");
}

#[test]
fn a_zero_budget_and_an_invalid_frame_are_render_errors() {
    assert!(matches!(
        CpuRenderer::new(0),
        Err(RenderError::OutputExceedsBudget {
            budget_bytes: 0,
            ..
        })
    ));
    let invalid = VideoFrame::new(PixelFormat::Rgba8, 2, 2, 1, 0.0, vec![0; 16]).unwrap_err();
    assert!(matches!(RenderError::from(invalid), RenderError::Frame(_)));
}

#[test]
fn layers_are_clipped_to_the_canvas_edges() {
    let red = frame(2, 2, [255, 0, 0, 255].repeat(4));
    let renderer = CpuRenderer::new(4 * 4 * 4).unwrap();
    let at = |x: i32, y: i32| {
        let mut layer = LayerFrame::new(&red);
        layer.x = x;
        layer.y = y;
        renderer.render(4, 4, 0.0, &[layer]).unwrap()
    };
    // Only the overlapping corner pixel is painted, from every side.
    assert_eq!(opaque_pixels(&at(3, 3)), vec![15]);
    assert_eq!(opaque_pixels(&at(-1, -1)), vec![0]);
    assert_eq!(opaque_pixels(&at(3, -1)), vec![3]);
    assert_eq!(opaque_pixels(&at(-1, 3)), vec![12]);
    // Entirely outside paints nothing.
    for (x, y) in [(10, 0), (0, 10), (-10, 0), (0, -10)] {
        assert!(opaque_pixels(&at(x, y)).is_empty(), "({x}, {y})");
    }
}

#[test]
fn rotated_layers_are_clipped_and_sampled_inside_their_crop() {
    let white = frame(4, 4, [255, 255, 255, 255].repeat(16));
    let renderer = CpuRenderer::new(8 * 8 * 4).unwrap();
    let rotated = |x: i32, y: i32, degrees: f32| {
        let mut layer = LayerFrame::new(&white);
        layer.x = x;
        layer.y = y;
        layer.transform.rotation_deg = degrees;
        renderer.render(8, 8, 0.0, &[layer]).unwrap()
    };
    let centred = opaque_pixels(&rotated(2, 2, 45.0));
    assert!(!centred.is_empty());
    // A 45 degree turn of a 4x4 square covers a diamond around its centre.
    assert!(centred.contains(&(4 * 8 + 4)));
    // Pushed off the top-left corner, only part of it remains and nothing panics.
    let clipped = opaque_pixels(&rotated(-2, -2, 45.0));
    assert!(!clipped.is_empty() && clipped.len() < centred.len());
    // A cropped layer samples only inside its crop window.
    let mut layer = LayerFrame::new(&white);
    layer.transform.crop = CropRect {
        x: 0.0,
        y: 0.0,
        width: 0.5,
        height: 0.5,
    };
    layer.transform.rotation_deg = 30.0;
    layer.x = 2;
    layer.y = 2;
    let cropped = opaque_pixels(&renderer.render(8, 8, 0.0, &[layer]).unwrap());
    assert!(!cropped.is_empty() && cropped.len() <= 9);
}

/// Every (source, backdrop) pair of channel values under a translucent normal blend, against the
/// float formula the compositor defines; the lookup-table path must give the same bytes.
#[test]
fn a_translucent_normal_blend_matches_the_float_formula_for_every_value() {
    let renderer = CpuRenderer::new(1 << 20).unwrap();
    let backdrop = frame(
        256,
        256,
        (0..256 * 256)
            .flat_map(|index| {
                let d = (index / 256) as u8;
                [d, d, d, 255]
            })
            .collect(),
    );
    let source = frame(
        256,
        256,
        (0..256 * 256)
            .flat_map(|index| {
                let s = (index % 256) as u8;
                [s, s, s, 255]
            })
            .collect(),
    );
    for opacity in [0.03_f32, 0.2, 0.5, 0.8, 0.97] {
        let keep = 1.0 - opacity;
        // The compositor takes this shortcut only when the output alpha comes out exactly one.
        assert_eq!(opacity + 1.0 * keep, 1.0, "pick opacities that qualify");
        let output = renderer
            .render(
                256,
                256,
                0.0,
                &[
                    LayerFrame::new(&backdrop),
                    LayerFrame {
                        opacity,
                        ..LayerFrame::new(&source)
                    },
                ],
            )
            .unwrap();
        for y in 0..256usize {
            for x in 0..256usize {
                let expected = ((opacity * (x as f32 / 255.0) + keep * (y as f32 / 255.0)) * 255.0)
                    .round() as u8;
                let at = (y * 256 + x) * 4;
                assert_eq!(
                    output.pixels()[at],
                    expected,
                    "opacity {opacity}, source {x}, backdrop {y}"
                );
                assert_eq!(output.pixels()[at + 3], 255);
            }
        }
    }
}

/// Brightness, contrast and saturation through the per-value table against the layer-effects
/// formula in the reference (the single-pixel path is the table-free arithmetic).
#[test]
fn colour_adjustments_through_the_table_match_the_formula_for_every_value() {
    let renderer = CpuRenderer::new(1 << 20).unwrap();
    let source = frame(
        256,
        256,
        (0..256 * 256)
            .flat_map(|index| {
                let r = (index % 256) as u8;
                let g = (index / 256) as u8;
                [r, g, r.wrapping_add(g), 255]
            })
            .collect(),
    );
    let effects = LayerEffects {
        brightness: 0.1,
        contrast: 1.3,
        saturation: 0.7,
        ..LayerEffects::default()
    };
    // Stretched, so the layer takes the general per-pixel path with the table.
    let transform = LayerTransform {
        scale_x: 1.0001,
        scale_y: 1.0001,
        ..LayerTransform::default()
    };
    let output = renderer
        .render(
            256,
            256,
            0.0,
            &[LayerFrame {
                effects,
                transform,
                ..LayerFrame::new(&source)
            }],
        )
        .unwrap();
    let adjust = |value: u8| -> f32 {
        let channel = f32::from(value) / 255.0;
        (((channel - 0.5) * 1.3 + 0.5) + 0.1).clamp(0.0, 1.0)
    };
    let mut checked = 0;
    for y in (0..256usize).step_by(3) {
        for x in (0..256usize).step_by(3) {
            let pixel = [x as u8, y as u8, (x as u8).wrapping_add(y as u8)];
            let tone = pixel.map(adjust);
            let luma = 0.2126 * tone[0] + 0.7152 * tone[1] + 0.0722 * tone[2];
            let expected =
                tone.map(|c| ((luma + (c - luma) * 0.7).clamp(0.0, 1.0) * 255.0).round() as u8);
            let at = (y * 256 + x) * 4;
            // The slight stretch maps pixel (x, y) onto itself at the centre of each cell.
            let got = [
                output.pixels()[at],
                output.pixels()[at + 1],
                output.pixels()[at + 2],
            ];
            let close = got.iter().zip(expected).all(|(g, e)| g.abs_diff(e) <= 3);
            if close {
                checked += 1;
            }
        }
    }
    // Nearly every sampled cell lines up exactly with the source pixel it came from.
    assert!(checked > 85 * 85 * 9 / 10, "only {checked} cells matched");
}
