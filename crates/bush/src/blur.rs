//! Neighbourhood effects applied to a layer frame before compositing: Gaussian blur and
//! unsharp-mask sharpening.
//!
//! The blur approximates a Gaussian with three successive box blurs (running sums), so cost does
//! not grow with the radius, and runs on all cores in row bands. Frames are blurred
//! premultiplied by alpha so translucent edges do not gain dark halos. Because the effect works
//! on the frame, not inside the compositor, the CPU and GPU renderers share it unchanged.

use crate::{PixelFormat, VideoFrame};

/// Largest supported blur sigma, in source pixels.
pub const MAX_BLUR_SIGMA: f32 = 64.0;

/// Unsharp-mask differences at or below this many levels are treated as rounding noise.
const SHARPEN_NOISE_FLOOR: f32 = 2.0;

/// Frames below this many pixels blur on the calling thread.
const PARALLEL_MIN_PIXELS: usize = 128 * 128;
const MAX_WORKERS: usize = 8;

/// Gaussian blur with standard deviation `sigma` pixels. `sigma <= 0` returns the frame as is.
pub fn gaussian_blur(frame: &VideoFrame, sigma: f32) -> VideoFrame {
    if !(sigma > 0.0) {
        return frame.clone();
    }
    let sigma = sigma.min(MAX_BLUR_SIGMA);
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let mut data = tight(frame);
    // Video frames are opaque, where premultiplying is the identity: skip both passes.
    let opaque = data.chunks_exact(4).all(|pixel| pixel[3] == 255);
    if !opaque {
        premultiply(&mut data);
    }
    let radii = box_radii(sigma);

    let mut scratch = vec![0u8; data.len()];
    for radius in radii {
        blur_rows(&mut data, width, height, radius);
    }
    transpose(&data, &mut scratch, width, height);
    for radius in radii {
        blur_rows(&mut scratch, height, width, radius);
    }
    transpose(&scratch, &mut data, height, width);

    if !opaque {
        unpremultiply(&mut data);
    }
    rebuild(frame, data)
}

/// Unsharp mask: `frame + amount * (frame - blur(frame, sigma))`, `amount` in `0.0..=4.0`.
pub fn sharpen(frame: &VideoFrame, amount: f32, sigma: f32) -> VideoFrame {
    if !(amount > 0.0) {
        return frame.clone();
    }
    let blurred = gaussian_blur(frame, sigma);
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let mut data = tight(frame);
    let blurred = tight(&blurred);
    for (pixel, soft) in data.chunks_exact_mut(4).zip(blurred.chunks_exact(4)) {
        for channel in 0..3 {
            let difference = f32::from(pixel[channel]) - f32::from(soft[channel]);
            // Differences within the 8-bit rounding noise of the blur are not detail: leaving
            // them alone keeps flat regions bit-exact instead of amplifying the noise.
            if difference.abs() > SHARPEN_NOISE_FLOOR {
                let value = f32::from(pixel[channel]) + amount * difference;
                pixel[channel] = value.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    debug_assert_eq!(data.len(), width * height * 4);
    rebuild(frame, data)
}

/// Box radii whose three successive blurs approximate a Gaussian of `sigma` (Kutskir).
fn box_radii(sigma: f32) -> [usize; 3] {
    let passes = 3.0_f32;
    let ideal = (12.0 * sigma * sigma / passes + 1.0).sqrt();
    let mut lower = ideal.floor() as i32;
    if lower % 2 == 0 {
        lower -= 1;
    }
    let lower = lower.max(1);
    let upper = lower + 2;
    let split = ((12.0 * sigma * sigma
        - passes * (lower * lower) as f32
        - 4.0 * passes * lower as f32
        - 3.0 * passes)
        / (-4.0 * lower as f32 - 4.0))
        .round() as i32;
    let size = |pass: i32| if pass < split { lower } else { upper };
    [
        ((size(0) - 1) / 2) as usize,
        ((size(1) - 1) / 2) as usize,
        ((size(2) - 1) / 2) as usize,
    ]
}

/// Tightly packed RGBA bytes of a frame (drops any row padding).
fn tight(frame: &VideoFrame) -> Vec<u8> {
    let row_bytes = frame.width() as usize * 4;
    if frame.stride_bytes() == row_bytes {
        return frame.pixels()[..row_bytes * frame.height() as usize].to_vec();
    }
    let mut data = Vec::with_capacity(row_bytes * frame.height() as usize);
    for row in 0..frame.height() as usize {
        let start = row * frame.stride_bytes();
        data.extend_from_slice(&frame.pixels()[start..start + row_bytes]);
    }
    data
}

fn premultiply(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
}

fn unpremultiply(data: &mut [u8]) {
    for pixel in data.chunks_exact_mut(4) {
        let alpha = u32::from(pixel[3]);
        if alpha != 255 && alpha != 0 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
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
    .expect("blur preserves the validated frame geometry")
}

fn worker_count(width: usize, height: usize) -> usize {
    if width * height < PARALLEL_MIN_PIXELS {
        return 1;
    }
    std::thread::available_parallelism()
        .map_or(1, std::num::NonZero::get)
        .min(MAX_WORKERS)
        .min(height.max(1))
}

/// One box blur of radius `radius` along every row, edges clamped, rows in parallel bands.
fn blur_rows(data: &mut [u8], width: usize, height: usize, radius: usize) {
    if radius == 0 || width == 0 {
        return;
    }
    let workers = worker_count(width, height);
    let rows_per_band = height.div_ceil(workers).max(1);
    if workers <= 1 {
        let mut line = vec![0u8; width * 4];
        for row in data.chunks_exact_mut(width * 4) {
            blur_line(row, &mut line, width, radius);
        }
        return;
    }
    std::thread::scope(|scope| {
        for band in data.chunks_mut(rows_per_band * width * 4) {
            scope.spawn(move || {
                let mut line = vec![0u8; width * 4];
                for row in band.chunks_exact_mut(width * 4) {
                    blur_line(row, &mut line, width, radius);
                }
            });
        }
    });
}

/// Running-sum box blur of one RGBA row.
fn blur_line(row: &mut [u8], line: &mut [u8], width: usize, radius: usize) {
    let window = (2 * radius + 1) as u64;
    // Division by the window is the hot spot: multiply by a rounded-up reciprocal instead. The
    // error is far below half a level, and a flat region stays exactly unchanged.
    let reciprocal = ((1_u64 << 24) + window - 1) / window;
    let mut sums = [0u32; 4];
    // Prime the window for pixel 0 with clamped edge samples.
    for offset in 0..=2 * radius {
        let index = offset.saturating_sub(radius).min(width - 1);
        for (channel, sum) in sums.iter_mut().enumerate() {
            *sum += u32::from(row[index * 4 + channel]);
        }
    }
    for pixel in 0..width {
        for (channel, sum) in sums.iter().enumerate() {
            line[pixel * 4 + channel] = ((u64::from(*sum) * reciprocal + (1 << 23)) >> 24) as u8;
        }
        let leaving = pixel.saturating_sub(radius).min(width - 1);
        let entering = (pixel + radius + 1).min(width - 1);
        for (channel, sum) in sums.iter_mut().enumerate() {
            *sum = *sum + u32::from(row[entering * 4 + channel])
                - u32::from(row[leaving * 4 + channel]);
        }
    }
    row.copy_from_slice(line);
}

/// `destination[x * height + y] = source[y * width + x]` for RGBA pixels. Destination rows are
/// split into parallel bands and each band is walked in square tiles so both the reads and the
/// writes stay cache friendly.
fn transpose(source: &[u8], destination: &mut [u8], width: usize, height: usize) {
    const TILE: usize = 16;
    let workers = worker_count(width, height);
    let rows_per_band = width.div_ceil(workers).max(1);
    let fill = |first_row: usize, band: &mut [u8]| {
        let rows = band.len() / (height * 4);
        for tile_x in (0..rows).step_by(TILE) {
            for tile_y in (0..height).step_by(TILE) {
                for offset in tile_x..(tile_x + TILE).min(rows) {
                    let x = first_row + offset;
                    for y in tile_y..(tile_y + TILE).min(height) {
                        let from = (y * width + x) * 4;
                        let to = (offset * height + y) * 4;
                        band[to..to + 4].copy_from_slice(&source[from..from + 4]);
                    }
                }
            }
        }
    };
    if workers <= 1 {
        fill(0, destination);
        return;
    }
    std::thread::scope(|scope| {
        for (index, band) in destination
            .chunks_mut(rows_per_band * height * 4)
            .enumerate()
        {
            let fill = &fill;
            scope.spawn(move || fill(index * rows_per_band, band));
        }
    });
}
