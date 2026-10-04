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

/// Blur radius of the unsharp mask behind `sharpen`, in source pixels.
pub const SHARPEN_SIGMA: f32 = 1.5;

/// The frame with `blur_sigma` blur then `sharpen` unsharp mask applied; `None` when both are
/// off. Shared by the CPU renderer (and mirrored by the GPU shaders).
pub(crate) fn apply_effects(
    frame: &VideoFrame,
    blur_sigma: f32,
    sharpen_amount: f32,
) -> Option<VideoFrame> {
    let blurred = (blur_sigma > 0.0).then(|| gaussian_blur(frame, blur_sigma));
    let source = blurred.as_ref().unwrap_or(frame);
    let sharpened = (sharpen_amount > 0.0).then(|| sharpen(source, sharpen_amount, SHARPEN_SIGMA));
    sharpened.or(blurred)
}

/// Gaussian blur with standard deviation `sigma` pixels. `sigma <= 0` returns the frame as is.
pub fn gaussian_blur(frame: &VideoFrame, sigma: f32) -> VideoFrame {
    if sigma.is_nan() || sigma <= 0.0 {
        return frame.clone();
    }
    let sigma = sigma.min(MAX_BLUR_SIGMA);
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let mut data = tight(frame);
    // Video frames are opaque, where premultiplying is the identity: skip both passes.
    let opaque = data.as_chunks::<4>().0.iter().all(|pixel| pixel[3] == 255);
    if !opaque {
        premultiply(&mut data);
    }
    let radii = box_radii(sigma);

    blur_horizontal(&mut data, width, height, radii);
    blur_vertical(&mut data, width, height, radii);

    if !opaque {
        unpremultiply(&mut data);
    }
    rebuild(frame, data)
}

/// Unsharp mask: `frame + amount * (frame - blur(frame, sigma))`, `amount` in `0.0..=4.0`.
pub fn sharpen(frame: &VideoFrame, amount: f32, sigma: f32) -> VideoFrame {
    if amount.is_nan() || amount <= 0.0 {
        return frame.clone();
    }
    let blurred = gaussian_blur(frame, sigma);
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let mut data = tight(frame);
    // `gaussian_blur` hands back tightly packed pixels, so they can be read in place.
    let soft = blurred.pixels();
    let row_bytes = width * 4;
    let sharpen_rows = |first_byte: usize, rows: &mut [u8]| {
        let reference = &soft[first_byte..first_byte + rows.len()];
        for (pixel, soft) in rows
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(reference.as_chunks::<4>().0)
        {
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
    };
    let workers = worker_count(width, height);
    if workers <= 1 {
        sharpen_rows(0, &mut data);
    } else {
        let rows_per_band = height.div_ceil(workers).max(1);
        std::thread::scope(|scope| {
            for (index, rows) in data.chunks_mut(rows_per_band * row_bytes).enumerate() {
                let sharpen_rows = &sharpen_rows;
                scope.spawn(move || sharpen_rows(index * rows_per_band * row_bytes, rows));
            }
        });
    }
    debug_assert_eq!(data.len(), width * height * 4);
    rebuild(frame, data)
}

/// Box radii whose three successive blurs approximate a Gaussian of `sigma` (Kutskir).
pub(crate) fn box_radii(sigma: f32) -> [usize; 3] {
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
    for pixel in data.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha != 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
            }
        }
    }
}

fn unpremultiply(data: &mut [u8]) {
    for pixel in data.as_chunks_mut::<4>().0 {
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

/// Divides a window sum by the window size, rounded to the nearest level. A flat region stays
/// exactly as it was: `255 * window` comes back as 255.
fn mean(sum: u32, reciprocal: f32) -> u8 {
    (sum as f32 * reciprocal + 0.5) as u8
}

/// The three box blurs along every row, in parallel bands of rows. A band does all three passes
/// on one row before the next, so the row stays in cache and threads start once per direction.
fn blur_horizontal(data: &mut [u8], width: usize, height: usize, radii: [usize; 3]) {
    if width == 0 || radii.iter().all(|radius| *radius == 0) {
        return;
    }
    let band = |rows: &mut [u8]| {
        let mut padded = Vec::new();
        for row in rows.chunks_exact_mut(width * 4) {
            for radius in radii.into_iter().filter(|radius| *radius > 0) {
                box_row(row, &mut padded, radius);
            }
        }
    };
    let workers = worker_count(width, height);
    if workers <= 1 {
        band(data);
        return;
    }
    let rows_per_band = height.div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        for rows in data.chunks_mut(rows_per_band * width * 4) {
            let band = &band;
            scope.spawn(move || band(rows));
        }
    });
}

/// Running-sum box blur of one RGBA row, edges clamped. The row is copied with `radius` edge
/// pixels on each side so the window never needs a bounds check.
fn box_row(row: &mut [u8], padded: &mut Vec<u8>, radius: usize) {
    let width = row.len() / 4;
    let window = 2 * radius + 1;
    let reciprocal = 1.0 / window as f32;
    padded.clear();
    for _ in 0..radius {
        padded.extend_from_slice(&row[..4]);
    }
    padded.extend_from_slice(row);
    for _ in 0..radius {
        padded.extend_from_slice(&row[row.len() - 4..]);
    }
    let pixels = padded.as_chunks::<4>().0;
    let mut sums = [0u32; 4];
    for pixel in &pixels[..window] {
        for (sum, value) in sums.iter_mut().zip(pixel) {
            *sum += u32::from(*value);
        }
    }
    for (x, out) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        for (channel, sum) in out.iter_mut().zip(sums) {
            *channel = mean(sum, reciprocal);
        }
        if x + 1 < width {
            let leaving = pixels[x];
            let entering = pixels[x + window];
            for ((sum, add), sub) in sums.iter_mut().zip(entering).zip(leaving) {
                *sum = *sum + u32::from(add) - u32::from(sub);
            }
        }
    }
}

/// Columns a worker takes at a time: wide enough for contiguous reads of each row.
const COLUMN_TILE: usize = 32;

/// The three box blurs down every column. Workers take bands of columns; each copies its band
/// into a contiguous buffer, blurs it with sliding sums over whole rows (so every access is
/// sequential and vectorizes) and writes it back. This replaces two full-frame transposes.
fn blur_vertical(data: &mut [u8], width: usize, height: usize, radii: [usize; 3]) {
    if height == 0 || radii.iter().all(|radius| *radius == 0) {
        return;
    }
    let workers = worker_count(width, height);
    let tiles = width.div_ceil(COLUMN_TILE);
    let tiles_per_band = tiles.div_ceil(workers).max(1);
    let source: &[u8] = data;
    let bands: Vec<(usize, Vec<u8>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..tiles)
            .step_by(tiles_per_band)
            .map(|first_tile| {
                scope.spawn(move || {
                    let first = first_tile * COLUMN_TILE;
                    let last = ((first_tile + tiles_per_band) * COLUMN_TILE).min(width);
                    (
                        first,
                        blur_column_band(source, width, height, first..last, radii),
                    )
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("a blur worker panicked"))
            .collect()
    });
    // Written back by row bands, so the copy is parallel too.
    let rows_per_band = height.div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        for (index, rows) in data.chunks_mut(rows_per_band * width * 4).enumerate() {
            let bands = &bands;
            scope.spawn(move || {
                for (offset, line) in rows.chunks_exact_mut(width * 4).enumerate() {
                    let y = index * rows_per_band + offset;
                    for (first, band) in bands {
                        let band_bytes = band.len() / height;
                        let start = first * 4;
                        line[start..start + band_bytes]
                            .copy_from_slice(&band[y * band_bytes..(y + 1) * band_bytes]);
                    }
                }
            });
        }
    });
}

/// The blurred columns `columns` of `source`, as `height` consecutive rows of those columns.
fn blur_column_band(
    source: &[u8],
    width: usize,
    height: usize,
    columns: std::ops::Range<usize>,
    radii: [usize; 3],
) -> Vec<u8> {
    let stride = columns.len() * 4;
    let mut current = Vec::with_capacity(stride * height);
    for y in 0..height {
        let start = (y * width + columns.start) * 4;
        current.extend_from_slice(&source[start..start + stride]);
    }
    let mut next = vec![0u8; stride * height];
    let mut sums = vec![0u32; stride];
    for radius in radii.into_iter().filter(|radius| *radius > 0) {
        let reciprocal = 1.0 / (2 * radius + 1) as f32;
        sums.fill(0);
        for offset in 0..=2 * radius {
            let row = offset.saturating_sub(radius).min(height - 1);
            for (sum, value) in sums
                .iter_mut()
                .zip(&current[row * stride..(row + 1) * stride])
            {
                *sum += u32::from(*value);
            }
        }
        for y in 0..height {
            for (out, sum) in next[y * stride..(y + 1) * stride].iter_mut().zip(&sums) {
                *out = mean(*sum, reciprocal);
            }
            if y + 1 < height {
                let leaving = y.saturating_sub(radius);
                let entering = (y + radius + 1).min(height - 1);
                let (leave, enter) = (
                    &current[leaving * stride..(leaving + 1) * stride],
                    &current[entering * stride..(entering + 1) * stride],
                );
                for ((sum, add), sub) in sums.iter_mut().zip(enter).zip(leave) {
                    *sum = *sum + u32::from(*add) - u32::from(*sub);
                }
            }
        }
        std::mem::swap(&mut current, &mut next);
    }
    current
}
