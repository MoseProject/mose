// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! RGBA-to-JPEG for frames leaving the backend. The webview decodes JPEG natively off its own
//! thread, so shipping a compressed, display-sized frame is far cheaper end to end than raw
//! pixels over IPC.

use avcore::SampledFrame;

/// Longest side of a preview frame sent to the webview. Matches a typical program monitor on a
/// 1440p display; the source keeps full resolution for export.
pub const PREVIEW_MAX_DIM: u32 = 1280;
/// Smallest longest side the preview is asked to composite at, however small the monitor.
pub const PREVIEW_MIN_DIM: u32 = 480;
/// Longest side of a media-library thumbnail.
pub const THUMBNAIL_MAX_DIM: u32 = 192;
/// The media library's poster frame: shown larger than a timeline strip tile.
pub const POSTER_MAX_DIM: u32 = 320;

const PREVIEW_JPEG_QUALITY: u8 = 82;

/// Nearest-neighbour downscale to `max_dim` on the longest side, dropping alpha, then JPEG.
/// Returns `None` for an empty or malformed frame.
pub fn encode_jpeg(frame: &SampledFrame, max_dim: u32) -> Option<Vec<u8>> {
    encode_jpeg_rgba(frame.width, frame.height, &frame.rgba, max_dim)
}

/// [`encode_jpeg`] over a borrowed tightly packed RGBA buffer, so a caller that already owns the
/// pixels (Bush) does not copy them into a `SampledFrame` first.
pub fn encode_jpeg_rgba(width: u32, height: u32, rgba: &[u8], max_dim: u32) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() < width as usize * height as usize * 4 {
        return None;
    }
    // Already at or under the target size: encode the RGBA buffer directly (the encoder skips
    // alpha), no intermediate RGB copy.
    if width.max(height) <= max_dim {
        return encode_with(
            &rgba[..width as usize * height as usize * 4],
            width,
            height,
            true,
        );
    }
    let (width, height, rgb) = downscale_to_rgb(width, height, rgba, max_dim)?;
    encode_with(&rgb, width, height, false)
}

/// A tightly packed RGBA image as a high-quality JPEG without chroma subsampling, for graphics
/// made of thin coloured lines (the colour scopes), which the 4:2:0 encoding of a photo smears
/// into a dim blur.
pub fn encode_jpeg_sharp(width: u32, height: u32, rgba: &[u8]) -> Option<Vec<u8>> {
    if width == 0 || height == 0 || rgba.len() < width as usize * height as usize * 4 {
        return None;
    }
    let (width, height) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    let mut out = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, 92);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    encoder
        .encode(
            &rgba[..usize::from(width) * usize::from(height) * 4],
            width,
            height,
            jpeg_encoder::ColorType::Rgba,
        )
        .ok()?;
    Some(out)
}

fn encode_with(data: &[u8], width: u32, height: u32, has_alpha: bool) -> Option<Vec<u8>> {
    let (width, height) = (u16::try_from(width).ok()?, u16::try_from(height).ok()?);
    let mut out = Vec::with_capacity(data.len() / 10);
    let mut encoder = jpeg_encoder::Encoder::new(&mut out, PREVIEW_JPEG_QUALITY);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_2_2);
    let color = if has_alpha {
        jpeg_encoder::ColorType::Rgba
    } else {
        jpeg_encoder::ColorType::Rgb
    };
    encoder.encode(data, width, height, color).ok()?;
    Some(out)
}

fn downscale_to_rgb(
    src_width: u32,
    src_height: u32,
    rgba: &[u8],
    max_dim: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    if src_width == 0
        || src_height == 0
        || rgba.len() < (src_width as usize * src_height as usize * 4)
    {
        return None;
    }
    let scale = (max_dim as f32 / src_width.max(src_height) as f32).min(1.0);
    let width = ((src_width as f32 * scale) as u32).max(1);
    let height = ((src_height as f32 * scale) as u32).max(1);
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    if (width, height) == (src_width, src_height) {
        // No resampling: a straight alpha-dropping copy, much cheaper than per-pixel indexing.
        for (dst, src) in rgb.chunks_exact_mut(3).zip(rgba.chunks_exact(4)) {
            dst.copy_from_slice(&src[..3]);
        }
        return Some((width, height, rgb));
    }
    // Four taps per output pixel: the first and last source pixel of its box on each axis,
    // averaged. Much less aliasing than a single nearest sample, at a fraction of a full box
    // filter's cost.
    let stride = src_width as usize * 4;
    for y in 0..height {
        let y0 = (y as u64 * src_height as u64 / height as u64) as usize;
        let y1 = (((y as u64 + 1) * src_height as u64 / height as u64) as usize)
            .saturating_sub(1)
            .clamp(y0, src_height as usize - 1);
        let dst_row = (y * width) as usize * 3;
        for x in 0..width {
            let x0 = (x as u64 * src_width as u64 / width as u64) as usize;
            let x1 = (((x as u64 + 1) * src_width as u64 / width as u64) as usize)
                .saturating_sub(1)
                .clamp(x0, src_width as usize - 1);
            let taps = [
                y0 * stride + x0 * 4,
                y0 * stride + x1 * 4,
                y1 * stride + x0 * 4,
                y1 * stride + x1 * 4,
            ];
            let dst = dst_row + x as usize * 3;
            for channel in 0..3 {
                let sum: u32 = taps.iter().map(|&at| u32::from(rgba[at + channel])).sum();
                rgb[dst + channel] = ((sum + 2) / 4) as u8;
            }
        }
    }
    Some((width, height, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> SampledFrame {
        SampledFrame {
            width,
            height,
            rgba: [10u8, 20, 30, 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn empty_or_truncated_frames_cannot_be_downscaled() {
        assert!(downscale_to_rgb(0, 4, &[0; 64], 100).is_none());
        assert!(downscale_to_rgb(4, 0, &[0; 64], 100).is_none());
        assert!(downscale_to_rgb(4, 4, &[0; 10], 100).is_none());
    }
    #[test]
    fn downscale_averages_instead_of_picking_one_pixel() {
        // Columns alternate black and white: a nearest sample would give pure black or white.
        let mut rgba = Vec::new();
        for _row in 0..2 {
            for column in 0..4u32 {
                let value = if column % 2 == 0 { 0 } else { 255 };
                rgba.extend_from_slice(&[value, value, value, 255]);
            }
        }
        let (width, height, rgb) = downscale_to_rgb(4, 2, &rgba, 2).unwrap();
        assert_eq!((width, height), (2, 1));
        assert_eq!(&rgb[..3], &[128, 128, 128]);
        assert_eq!(&rgb[3..6], &[128, 128, 128]);
    }

    #[test]
    fn downscale_keeps_aspect_and_caps_longest_side() {
        let frame = solid(1920, 1080);
        let (w, h, rgb) = downscale_to_rgb(frame.width, frame.height, &frame.rgba, 1280).unwrap();
        assert_eq!((w, h), (1280, 720));
        assert_eq!(rgb.len(), (1280 * 720 * 3) as usize);
        assert_eq!(&rgb[..3], &[10, 20, 30]);
    }

    #[test]
    fn small_frames_are_never_upscaled() {
        let frame = solid(320, 180);
        let (w, h, _) = downscale_to_rgb(frame.width, frame.height, &frame.rgba, 1280).unwrap();
        assert_eq!((w, h), (320, 180));
    }

    #[test]
    fn truncated_frame_is_rejected() {
        let mut frame = solid(4, 4);
        frame.rgba.truncate(10);
        assert!(encode_jpeg(&frame, 1280).is_none());
    }

    #[test]
    fn encoded_jpeg_decodes_to_the_expected_size_and_colour() {
        // At target size the RGBA buffer is encoded directly.
        let direct = encode_jpeg_rgba(64, 36, &solid(64, 36).rgba, 1280).unwrap();
        let decoded = image::load_from_memory(&direct).unwrap().to_rgb8();
        assert_eq!(decoded.dimensions(), (64, 36));
        let pixel = decoded.get_pixel(32, 18).0;
        assert!(
            pixel
                .iter()
                .zip([10u8, 20, 30])
                .all(|(got, want)| got.abs_diff(want) <= 6),
            "decoded {pixel:?}"
        );

        // Over the target size it is downscaled first.
        let big = solid(2000, 1000);
        let shrunk = encode_jpeg_rgba(big.width, big.height, &big.rgba, 1280).unwrap();
        let decoded = image::load_from_memory(&shrunk).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1280, 640));
    }

    #[test]
    fn encodes_a_jpeg_with_soi_marker() {
        let jpeg = encode_jpeg(&solid(64, 36), 1280).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }
}

#[cfg(test)]
mod encode_cost {
    use super::*;

    /// Cost of one preview frame: `cargo test --release -p desktop jpeg_cost -- --ignored
    /// --nocapture`. A busy gradient with noise, the kind of picture a game frame is.
    #[test]
    #[ignore = "a measurement, not a check"]
    fn jpeg_cost() {
        for (width, height) in [(854u32, 480u32), (1280, 720), (1920, 1080)] {
            let mut rgba = vec![0u8; width as usize * height as usize * 4];
            let mut state = 0x9E37_79B9_u32;
            for (index, pixel) in rgba.chunks_exact_mut(4).enumerate() {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let x = (index % width as usize) as u32;
                let y = (index / width as usize) as u32;
                pixel[0] = ((x * 255 / width) as u8).wrapping_add((state >> 28) as u8);
                pixel[1] = ((y * 255 / height) as u8).wrapping_add((state >> 24) as u8 & 15);
                pixel[2] = (((x + y) * 255 / (width + height)) as u8) ^ ((state >> 20) as u8 & 7);
                pixel[3] = 255;
            }
            let mut times = Vec::new();
            let mut size = 0;
            for _ in 0..60 {
                let started = std::time::Instant::now();
                let jpeg = encode_jpeg_rgba(width, height, &rgba, width.max(height)).unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.0);
                size = jpeg.len();
            }
            times.sort_by(f64::total_cmp);
            println!(
                "{width}x{height}: p50 {:.2} ms, p95 {:.2} ms, {} KB",
                times[30],
                times[57],
                size / 1024
            );
        }
    }
}
