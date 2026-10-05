//! Glitch and shake: deterministic, time-seeded distortion effects.
//!
//! Both are pure functions of the clip time, so a paused frame never flickers and a re-render of
//! the same instant is bit-identical. Glitch splits the colour channels and displaces random
//! horizontal bands; shake is smooth value noise used to jitter a layer position.

use crate::VideoFrame;

/// Glitch pattern changes at this many steps per second (it looks like digital tearing, not
/// like motion).
const GLITCH_STEPS_PER_SECOND: f64 = 12.0;

/// Smooth noise frequency of the shake, in noise cells per second.
const SHAKE_CELLS_PER_SECOND: f64 = 9.0;

fn mix64(mut value: u64) -> u64 {
    value ^= value >> 33;
    value = value.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    value ^= value >> 33;
    value = value.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
    value ^ (value >> 33)
}

/// Uniform value in `-1.0..=1.0` from a hash.
fn unit(hash: u64) -> f32 {
    ((hash >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0) as f32
}

/// Applies glitch of `intensity` (`0.0..=1.0`) to `frame`, seeded by `time_secs`.
pub fn glitch(frame: &VideoFrame, intensity: f32, time_secs: f64) -> VideoFrame {
    if intensity.is_nan() || intensity <= 0.0 {
        return frame.clone();
    }
    let intensity = intensity.min(1.0);
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let step = (time_secs * GLITCH_STEPS_PER_SECOND).floor() as i64 as u64;
    let seed = mix64(step ^ 0x9E37_79B9_7F4A_7C15);

    let row_bytes = width * 4;
    let mut out = vec![0u8; row_bytes * height];
    let split = ((intensity * width as f32 * 0.02).round() as usize).max(1);

    // Displaced bands: heights and offsets come from the seed, the count from the intensity.
    let band_count = 1 + (intensity * 7.0) as usize;
    let mut offsets = vec![0isize; height];
    for band in 0..band_count {
        let hash = mix64(seed ^ (band as u64 + 1).wrapping_mul(0xA24B_AED4_963E_E407));
        let band_height = 2 + (hash % (height / 12).max(3) as u64) as usize;
        let start = (mix64(hash) % height.max(1) as u64) as usize;
        let shift = (unit(mix64(hash ^ 0x55)) * intensity * width as f32 * 0.12) as isize;
        let end = (start + band_height).min(height);
        offsets[start..end].fill(shift);
    }

    for row in 0..height {
        let line_shift = offsets[row];
        let source_row = &frame.pixels()[row * frame.stride_bytes()..][..row_bytes];
        let out_row = &mut out[row * row_bytes..(row + 1) * row_bytes];
        for x in 0..width {
            let sample = |extra: isize, channel: usize| -> u8 {
                let at = (x as isize - line_shift + extra).clamp(0, width as isize - 1) as usize;
                source_row[at * 4 + channel]
            };
            // Red leans left, blue right, green and alpha stay on the (shifted) line.
            out_row[x * 4] = sample(split as isize, 0);
            out_row[x * 4 + 1] = sample(0, 1);
            out_row[x * 4 + 2] = sample(-(split as isize), 2);
            out_row[x * 4 + 3] = sample(0, 3);
        }
    }
    frame
        .with_tight_pixels(out)
        .expect("glitch preserves the validated frame geometry")
}

/// Smooth pseudo-random offset in `-amplitude..=amplitude` at `time_secs`; `stream` decorrelates
/// the x and y components.
pub fn shake_offset(amplitude: f32, time_secs: f64, stream: u64) -> f32 {
    if amplitude.is_nan() || amplitude <= 0.0 {
        return 0.0;
    }
    let position = time_secs.max(0.0) * SHAKE_CELLS_PER_SECOND;
    let cell = position.floor();
    let fraction = (position - cell) as f32;
    let smooth = fraction * fraction * (3.0 - 2.0 * fraction);
    let at = |index: f64| {
        unit(mix64(
            (index as u64) ^ stream.wrapping_mul(0x9E37_79B9_7F4A_7C15),
        ))
    };
    let value = at(cell) + (at(cell + 1.0) - at(cell)) * smooth;
    value * amplitude
}
