//! Video stabilization: estimate global camera motion between consecutive frames, smooth the
//! camera path, and report the translation that moves each frame back onto the smooth path.
//!
//! Motion is found by sum-of-absolute-differences block matching on a downscaled luma plane with
//! a parabolic sub-pixel refinement. State is the previous luma plane plus the raw and smoothed
//! path, so memory is constant. A repeated timestamp reuses the previous correction (a paused
//! frame never drifts) and any discontinuity resets the path.

use crate::temporal_effect::TemporalEffect;
use crate::VideoFrame;

/// Zoom applied to a stabilized layer (about its centre) to hide the moving borders.
pub const STABILIZE_ZOOM: f32 = 1.06;

/// Longest side of the analysis plane, in pixels.
const ANALYSIS_SIDE: usize = 160;
/// Search radius on the analysis plane, in pixels.
const SEARCH_RADIUS: i32 = 8;
/// Frames further apart than this (or going backwards) restart the path.
const CONTINUITY_SECS: f64 = 0.5;

#[derive(Clone, Debug)]
struct Luma {
    width: usize,
    height: usize,
    data: Vec<f32>,
}

#[derive(Clone, Debug)]
pub struct Stabilizer {
    /// Weight of each new sample in the smoothed path, `0.0 < w <= 1.0` (lower is steadier).
    smoothing: f32,
    previous: Option<Luma>,
    path: (f32, f32),
    smoothed: (f32, f32),
    last_secs: f64,
    last_correction: (f32, f32),
    started: bool,
}

impl Stabilizer {
    pub fn new(smoothing: f32) -> Option<Self> {
        (smoothing.is_finite() && smoothing > 0.0 && smoothing <= 1.0).then_some(Self {
            smoothing,
            previous: None,
            path: (0.0, 0.0),
            smoothed: (0.0, 0.0),
            last_secs: 0.0,
            last_correction: (0.0, 0.0),
            started: false,
        })
    }

    pub fn reset(&mut self) {
        self.previous = None;
        self.path = (0.0, 0.0);
        self.smoothed = (0.0, 0.0);
        self.last_correction = (0.0, 0.0);
        self.started = false;
    }

    /// The translation, in frame pixels, to apply to this frame so it follows the smoothed
    /// camera path. The first frame of a continuous run needs none.
    pub fn analyze(&mut self, frame: &VideoFrame) -> (f32, f32) {
        let timestamp = frame.timestamp_secs();
        if self.started && timestamp == self.last_secs {
            return self.last_correction;
        }
        if !self.started
            || timestamp < self.last_secs
            || timestamp - self.last_secs > CONTINUITY_SECS
        {
            self.reset();
        }
        self.last_secs = timestamp;
        self.started = true;

        let (luma, scale) = downscale(frame);
        if let Some(previous) = &self.previous {
            if previous.width == luma.width && previous.height == luma.height {
                let (dx, dy) = estimate_motion(previous, &luma);
                self.path.0 += dx * scale;
                self.path.1 += dy * scale;
            }
        }
        self.previous = Some(luma);

        let weight = self.smoothing;
        self.smoothed.0 += (self.path.0 - self.smoothed.0) * weight;
        self.smoothed.1 += (self.path.1 - self.smoothed.1) * weight;
        self.last_correction = (self.smoothed.0 - self.path.0, self.smoothed.1 - self.path.1);
        self.last_correction
    }
}

impl TemporalEffect for Stabilizer {
    fn smoothing_mut(&mut self) -> &mut f32 {
        &mut self.smoothing
    }

    fn reset_history(&mut self) {
        self.reset();
    }
}

/// Box-downscales the frame to an analysis plane; returns it with the integer scale factor.
fn downscale(frame: &VideoFrame) -> (Luma, f32) {
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    let factor = (width.max(height) / ANALYSIS_SIDE).max(1);
    let (out_w, out_h) = (width / factor, height / factor);
    let mut data = vec![0.0_f32; out_w * out_h];
    for y in 0..out_h {
        for x in 0..out_w {
            let mut sum = 0.0_f32;
            for dy in 0..factor {
                let row = (y * factor + dy) * frame.stride_bytes();
                for dx in 0..factor {
                    let at = row + (x * factor + dx) * 4;
                    let p = &frame.pixels()[at..at + 4];
                    sum += 0.2126 * f32::from(p[0])
                        + 0.7152 * f32::from(p[1])
                        + 0.0722 * f32::from(p[2]);
                }
            }
            data[y * out_w + x] = sum / (factor * factor) as f32;
        }
    }
    (
        Luma {
            width: out_w,
            height: out_h,
            data,
        },
        factor as f32,
    )
}

/// Content motion from `previous` to `current` on the analysis plane, in plane pixels:
/// `current(x, y) ~ previous(x - dx, y - dy)`.
fn estimate_motion(previous: &Luma, current: &Luma) -> (f32, f32) {
    let radius = SEARCH_RADIUS as usize;
    let (width, height) = (current.width, current.height);
    if width <= 2 * radius + 2 || height <= 2 * radius + 2 {
        return (0.0, 0.0);
    }
    let size = (2 * SEARCH_RADIUS + 1) as usize;
    let mut costs = vec![f32::MAX; size * size];
    for candidate_y in -SEARCH_RADIUS..=SEARCH_RADIUS {
        for candidate_x in -SEARCH_RADIUS..=SEARCH_RADIUS {
            let mut sad = 0.0_f32;
            for y in radius..height - radius {
                let row = y * width;
                let source_row = ((y as i32 - candidate_y) as usize) * width;
                for x in radius..width - radius {
                    let source = (x as i32 - candidate_x) as usize;
                    sad += (current.data[row + x] - previous.data[source_row + source]).abs();
                }
            }
            costs[(candidate_y + SEARCH_RADIUS) as usize * size
                + (candidate_x + SEARCH_RADIUS) as usize] = sad;
        }
    }
    let (best_index, _) = costs
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .expect("search window is not empty");
    let (best_x, best_y) = ((best_index % size) as i32, (best_index / size) as i32);
    let cost = |x: i32, y: i32| costs[y as usize * size + x as usize];
    let fraction_x = if best_x > 0 && best_x < size as i32 - 1 {
        parabolic_offset(
            cost(best_x - 1, best_y),
            cost(best_x, best_y),
            cost(best_x + 1, best_y),
        )
    } else {
        0.0
    };
    let fraction_y = if best_y > 0 && best_y < size as i32 - 1 {
        parabolic_offset(
            cost(best_x, best_y - 1),
            cost(best_x, best_y),
            cost(best_x, best_y + 1),
        )
    } else {
        0.0
    };
    (
        (best_x - SEARCH_RADIUS) as f32 + fraction_x,
        (best_y - SEARCH_RADIUS) as f32 + fraction_y,
    )
}

/// Sub-pixel offset of the minimum of the parabola through three costs; `0` when they are
/// (nearly) collinear, which a real minimum cannot be.
fn parabolic_offset(negative: f32, centre: f32, positive: f32) -> f32 {
    let denominator = negative - 2.0 * centre + positive;
    if denominator.abs() < 1e-6 {
        0.0
    } else {
        (0.5 * (negative - positive) / denominator).clamp(-0.5, 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::parabolic_offset;

    #[test]
    fn the_offset_follows_the_parabola_and_is_clamped() {
        // Symmetric costs have their minimum at the centre.
        assert_eq!(parabolic_offset(2.0, 1.0, 2.0), 0.0);
        // A lower right neighbour pulls the minimum towards it; the shift never exceeds half a pixel.
        assert!((parabolic_offset(3.0, 1.0, 2.0) - 1.0 / 6.0).abs() < 1e-6);
        assert_eq!(parabolic_offset(10.0, 3.0, 1.0), 0.5);
        assert_eq!(parabolic_offset(1.0, 3.0, 10.0), -0.5);
    }

    #[test]
    fn collinear_costs_have_no_offset() {
        assert_eq!(parabolic_offset(3.0, 2.0, 1.0), 0.0);
        assert_eq!(parabolic_offset(1.0, 1.0, 1.0), 0.0);
    }
}
