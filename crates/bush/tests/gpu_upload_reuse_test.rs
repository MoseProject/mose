//! GPU source upload reuse: an unchanged layer frame is not uploaded again, and the output stays
//! identical to a fresh render. Requires `--features gpu` and an adapter (skips without one).
#![cfg(feature = "gpu")]

use bush::{GpuRenderer, LayerFrame, PixelFormat, Renderer, VideoFrame};

fn solid(width: u32, height: u32, rgb: [u8; 3]) -> VideoFrame {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for _ in 0..width * height {
        pixels.extend_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
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

fn renderer() -> Option<GpuRenderer> {
    match GpuRenderer::new(1 << 22) {
        Ok(renderer) => Some(renderer),
        Err(error) => {
            eprintln!("skipped: no usable GPU adapter ({error:?})");
            None
        }
    }
}

#[test]
fn frame_ids_follow_content_identity() {
    let first = solid(8, 8, [1, 2, 3]);
    let second = solid(8, 8, [1, 2, 3]);
    assert_ne!(first.id(), second.id());
    assert_eq!(first, second, "equality compares pixels, not identity");
    assert_eq!(first.clone().id(), first.id());
}

#[test]
fn an_unchanged_layer_is_not_uploaded_again() {
    let Some(mut gpu) = renderer() else { return };
    let frame = solid(64, 48, [200, 40, 90]);

    let first = gpu.render(64, 48, 0.0, &[LayerFrame::new(&frame)]).unwrap();
    assert_eq!(gpu.uploads_skipped(), 0);
    let second = gpu.render(64, 48, 0.0, &[LayerFrame::new(&frame)]).unwrap();
    assert_eq!(gpu.uploads_skipped(), 1);
    assert_eq!(first.pixels(), second.pixels());

    // Per-frame parameters still apply on a reused source.
    let mut faded = LayerFrame::new(&frame);
    faded.opacity = 0.5;
    let half = gpu.render(64, 48, 0.0, &[faded]).unwrap();
    assert_eq!(gpu.uploads_skipped(), 2);
    assert_ne!(half.pixels(), first.pixels());
}

#[test]
fn a_different_frame_in_the_same_slot_is_uploaded() {
    let Some(mut gpu) = renderer() else { return };
    let red = solid(64, 48, [255, 0, 0]);
    let blue = solid(64, 48, [0, 0, 255]);

    let first = gpu.render(64, 48, 0.0, &[LayerFrame::new(&red)]).unwrap();
    let second = gpu.render(64, 48, 0.0, &[LayerFrame::new(&blue)]).unwrap();
    assert_eq!(gpu.uploads_skipped(), 0);
    assert_eq!(&first.pixels()[..4], &[255, 0, 0, 255]);
    assert_eq!(&second.pixels()[..4], &[0, 0, 255, 255]);

    // Back to the first frame: the slot holds blue now, so red must upload again.
    let third = gpu.render(64, 48, 0.0, &[LayerFrame::new(&red)]).unwrap();
    assert_eq!(gpu.uploads_skipped(), 0);
    assert_eq!(first.pixels(), third.pixels());
}
