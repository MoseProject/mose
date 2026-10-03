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
/// Longest side of a media-library thumbnail.
pub const THUMBNAIL_MAX_DIM: u32 = 320;

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
    for y in 0..height {
        let src_y = (y as u64 * src_height as u64 / height as u64) as u32;
        let src_row = (src_y as usize * src_width as usize) * 4;
        let dst_row = (y * width) as usize * 3;
        for x in 0..width {
            let src_x = (x as u64 * src_width as u64 / width as u64) as usize;
            let src = src_row + src_x * 4;
            let dst = dst_row + x as usize * 3;
            rgb[dst..dst + 3].copy_from_slice(&rgba[src..src + 3]);
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
