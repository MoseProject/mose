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
