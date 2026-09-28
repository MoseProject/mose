use bush::{CpuRenderer, LayerFrame, PixelFormat, RenderError, VideoFrame};

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
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 1.0,
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
                },
                LayerFrame {
                    frame: &blue,
                    x: 0,
                    y: 0,
                    opacity: 0.5,
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
                opacity: 1.1
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
