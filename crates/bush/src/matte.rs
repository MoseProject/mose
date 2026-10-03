//! Mattes from precomputed matte videos (grayscale luma per pixel, white is "inside").
//!
//! Two uses, both mirroring the exporter: an *alpha matte* multiplies a layer's alpha (AI
//! background removal), and a *privacy blur* blurs the composited canvas wherever the matte is
//! non-zero. A matte frame has its own resolution (the clip's source frames), so it is sampled
//! nearest-neighbour at the size of the frame it applies to.

use crate::{gaussian_blur, PixelFormat, VideoFrame};

/// Matte value (0..=255) at pixel `(x, y)` of a `width` x `height` target.
fn sampler(matte: &VideoFrame, width: usize, height: usize) -> impl Fn(usize, usize) -> u32 + '_ {
    let (mw, mh) = (matte.width() as usize, matte.height() as usize);
    let stride = matte.stride_bytes();
    let pixels = matte.pixels();
    move |x, y| {
        let mx = (x * mw / width).min(mw - 1);
        let my = (y * mh / height).min(mh - 1);
        u32::from(pixels[my * stride + mx * 4])
    }
}

fn tight(frame: &VideoFrame) -> Vec<u8> {
    let row_bytes = frame.width() as usize * 4;
    let mut data = Vec::with_capacity(row_bytes * frame.height() as usize);
    for row in 0..frame.height() as usize {
        let start = row * frame.stride_bytes();
        data.extend_from_slice(&frame.pixels()[start..start + row_bytes]);
    }
    data
}

fn rebuild(frame: &VideoFrame, data: Vec<u8>) -> VideoFrame {
    VideoFrame::new(
        PixelFormat::Rgba8,
        frame.width(),
        frame.height(),
        frame.width() as usize * 4,
        frame.timestamp_secs(),
        data,
    )
    .expect("matte preserves the validated frame geometry")
}

/// The frame with its alpha multiplied by the matte.
pub(crate) fn apply_alpha_matte(frame: &VideoFrame, matte: &VideoFrame) -> VideoFrame {
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let value = sampler(matte, width, height);
    let mut data = tight(frame);
    for (index, pixel) in data.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let matte_value = value(index % width, index / width);
        pixel[3] = ((u32::from(pixel[3]) * matte_value + 127) / 255) as u8;
    }
    rebuild(frame, data)
}

/// The canvas blurred with `sigma` wherever the matte is set, blended by the matte value.
pub(crate) fn blur_through_matte(
    canvas: &VideoFrame,
    matte: &VideoFrame,
    sigma: f32,
) -> VideoFrame {
    let blurred = gaussian_blur(canvas, sigma);
    let (width, height) = (canvas.width() as usize, canvas.height() as usize);
    let value = sampler(matte, width, height);
    let mut data = tight(canvas);
    let soft = tight(&blurred);
    for (index, (pixel, blurred)) in data
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip(soft.as_chunks::<4>().0)
        .enumerate()
    {
        let weight = value(index % width, index / width);
        if weight == 0 {
            continue;
        }
        for channel in 0..3 {
            let mixed =
                u32::from(blurred[channel]) * weight + u32::from(pixel[channel]) * (255 - weight);
            pixel[channel] = ((mixed + 127) / 255) as u8;
        }
    }
    rebuild(canvas, data)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> VideoFrame {
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&pixel(x, y));
            }
        }
        VideoFrame::new(
            PixelFormat::Rgba8,
            width,
            height,
            width as usize * 4,
            0.0,
            data,
        )
        .unwrap()
    }

    fn gray(value: u8) -> [u8; 4] {
        [value, value, value, 255]
    }

    #[test]
    fn the_alpha_matte_scales_alpha_and_is_sampled_at_the_frame_size() {
        // A 2x1 matte (left white, right black) over a 4x2 frame: left half kept, right cut.
        let matte = frame(2, 1, |x, _| gray(if x == 0 { 255 } else { 0 }));
        let layer = frame(4, 2, |_, _| [10, 20, 30, 255]);
        let out = apply_alpha_matte(&layer, &matte);
        for y in 0..2usize {
            for x in 0..4usize {
                let pixel = &out.pixels()[(y * 4 + x) * 4..][..4];
                assert_eq!(&pixel[..3], &[10, 20, 30]);
                assert_eq!(pixel[3], if x < 2 { 255 } else { 0 }, "pixel {x},{y}");
            }
        }
        // A half-grey matte halves an opaque alpha and keeps a transparent one transparent.
        let half = frame(1, 1, |_, _| gray(128));
        let mixed = frame(2, 1, |x, _| [1, 2, 3, if x == 0 { 255 } else { 0 }]);
        let out = apply_alpha_matte(&mixed, &half);
        assert_eq!(out.pixels()[3], 128);
        assert_eq!(out.pixels()[7], 0);
    }

    #[test]
    fn the_privacy_blur_changes_only_where_the_matte_is_set() {
        // A hard vertical edge: blurring must soften it inside the matte and not outside.
        let canvas = frame(32, 8, |x, _| gray(if x < 16 { 0 } else { 255 }));
        let matte = frame(32, 8, |x, _| gray(if x < 24 { 255 } else { 0 }));
        let out = blur_through_matte(&canvas, &matte, 3.0);
        let at = |x: usize| out.pixels()[(4 * 32 + x) * 4];
        assert!(
            at(15) > 0 && at(16) < 255,
            "edge inside the matte is softened"
        );
        assert_eq!(at(28), 255, "untouched outside the matte");
        assert_eq!(at(2), 0);
    }
}
