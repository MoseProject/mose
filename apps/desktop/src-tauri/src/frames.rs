// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! RGBA-to-JPEG for frames leaving the backend. The webview decodes JPEG natively off its own
//! thread, so shipping a compressed, display-sized frame is far cheaper end to end than raw
//! pixels over IPC.

use avcore::preview::VideoFrame;
use image::codecs::jpeg::JpegEncoder;
use image::ExtendedColorType;

/// Longest side of a preview frame sent to the webview. Matches a typical program monitor on a
/// 1440p display; the source keeps full resolution for export.
pub const PREVIEW_MAX_DIM: u32 = 1280;
/// Longest side of a media-library thumbnail.
pub const THUMBNAIL_MAX_DIM: u32 = 320;

const PREVIEW_JPEG_QUALITY: u8 = 82;

/// Nearest-neighbour downscale to `max_dim` on the longest side, dropping alpha, then JPEG.
/// Returns `None` for an empty or malformed frame.
pub fn encode_jpeg(frame: &VideoFrame, max_dim: u32) -> Option<Vec<u8>> {
    let (width, height, rgb) = downscale_to_rgb(frame, max_dim)?;
    let mut out = Vec::with_capacity(rgb.len() / 8);
    JpegEncoder::new_with_quality(&mut out, PREVIEW_JPEG_QUALITY)
        .encode(&rgb, width, height, ExtendedColorType::Rgb8)
        .ok()?;
    Some(out)
}

fn downscale_to_rgb(frame: &VideoFrame, max_dim: u32) -> Option<(u32, u32, Vec<u8>)> {
    if frame.width == 0
        || frame.height == 0
        || frame.rgba.len() < (frame.width * frame.height * 4) as usize
    {
        return None;
    }
    let scale = (max_dim as f32 / frame.width.max(frame.height) as f32).min(1.0);
    let width = ((frame.width as f32 * scale) as u32).max(1);
    let height = ((frame.height as f32 * scale) as u32).max(1);
    let mut rgb = vec![0u8; (width * height * 3) as usize];
    for y in 0..height {
        let src_y = (y as u64 * frame.height as u64 / height as u64) as u32;
        let src_row = (src_y * frame.width) as usize * 4;
        let dst_row = (y * width) as usize * 3;
        for x in 0..width {
            let src_x = (x as u64 * frame.width as u64 / width as u64) as usize;
            let src = src_row + src_x * 4;
            let dst = dst_row + x as usize * 3;
            rgb[dst..dst + 3].copy_from_slice(&frame.rgba[src..src + 3]);
        }
    }
    Some((width, height, rgb))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32) -> VideoFrame {
        VideoFrame {
            width,
            height,
            rgba: [10u8, 20, 30, 255].repeat((width * height) as usize),
        }
    }

    #[test]
    fn downscale_keeps_aspect_and_caps_longest_side() {
        let (w, h, rgb) = downscale_to_rgb(&solid(1920, 1080), 1280).unwrap();
        assert_eq!((w, h), (1280, 720));
        assert_eq!(rgb.len(), (1280 * 720 * 3) as usize);
        assert_eq!(&rgb[..3], &[10, 20, 30]);
    }

    #[test]
    fn small_frames_are_never_upscaled() {
        let (w, h, _) = downscale_to_rgb(&solid(320, 180), 1280).unwrap();
        assert_eq!((w, h), (320, 180));
    }

    #[test]
    fn truncated_frame_is_rejected() {
        let mut frame = solid(4, 4);
        frame.rgba.truncate(10);
        assert!(encode_jpeg(&frame, 1280).is_none());
    }

    #[test]
    fn encodes_a_jpeg_with_soi_marker() {
        let jpeg = encode_jpeg(&solid(64, 36), 1280).unwrap();
        assert_eq!(&jpeg[..2], &[0xFF, 0xD8]);
    }
}
