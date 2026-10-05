//! Temporal deflicker: steadies frame-to-frame brightness pumping.
//!
//! Keeps one exponential moving average of mean luma per stream and scales each frame towards
//! it. State is a single `f32` plus the last timestamp, so memory is constant; a seek or jump
//! resets it rather than blending unrelated frames.

use crate::VideoFrame;

/// Frames further apart than this (or going backwards) restart the average: they are not part
/// of one continuous shot.
const CONTINUITY_SECS: f64 = 0.5;

/// Largest correction applied to a frame, so a genuine fade is not flattened.
const MAX_GAIN: f32 = 2.0;
const MIN_GAIN: f32 = 0.5;

#[derive(Clone, Debug)]
pub struct Deflicker {
    /// Weight of the newest frame in the moving average, `0.0..=1.0`.
    smoothing: f32,
    average_luma: Option<f32>,
    last_secs: f64,
    /// Gain applied to the frame at `last_secs`, reused when that frame is shown again.
    last_gain: f32,
}

impl Deflicker {
    /// `smoothing` is the weight of each new frame: lower values hold the average steadier.
    pub fn new(smoothing: f32) -> Option<Self> {
        (smoothing.is_finite() && smoothing > 0.0 && smoothing <= 1.0).then_some(Self {
            smoothing,
            average_luma: None,
            last_secs: 0.0,
            last_gain: 1.0,
        })
    }

    /// Forgets the average (call after any discontinuity you know about).
    pub fn reset(&mut self) {
        self.average_luma = None;
    }

    /// The frame scaled so its mean luma tracks the running average. The first frame of a
    /// continuous run passes through unchanged.
    pub fn process(&mut self, frame: &VideoFrame) -> VideoFrame {
        let timestamp = frame.timestamp_secs();
        // Showing the same frame again (paused, live edit) must not advance the average, or the
        // picture would drift while nothing changes.
        let repeated = self.average_luma.is_some() && timestamp == self.last_secs;
        let gain = if repeated {
            self.last_gain
        } else {
            let discontinuous =
                timestamp < self.last_secs || timestamp - self.last_secs > CONTINUITY_SECS;
            if discontinuous {
                self.average_luma = None;
            }
            self.last_secs = timestamp;

            let mean = mean_luma(frame);
            let average = match self.average_luma {
                None => mean,
                Some(previous) => previous + (mean - previous) * self.smoothing,
            };
            self.average_luma = Some(average);
            let gain = if mean > 1e-3 {
                (average / mean).clamp(MIN_GAIN, MAX_GAIN)
            } else {
                1.0
            };
            self.last_gain = gain;
            gain
        };
        if (gain - 1.0).abs() < 1e-4 {
            return frame.clone();
        }
        scaled(frame, gain)
    }
}

fn mean_luma(frame: &VideoFrame) -> f32 {
    let width = frame.width() as usize;
    let mut sum = 0.0_f64;
    for row in 0..frame.height() as usize {
        let start = row * frame.stride_bytes();
        for pixel in frame.pixels()[start..start + width * 4].as_chunks::<4>().0 {
            sum += f64::from(pixel[0]) * 0.2126
                + f64::from(pixel[1]) * 0.7152
                + f64::from(pixel[2]) * 0.0722;
        }
    }
    (sum / (width * frame.height() as usize) as f64 / 255.0) as f32
}

fn scaled(frame: &VideoFrame, gain: f32) -> VideoFrame {
    let mut pixels = frame.pixels().to_vec();
    let width = frame.width() as usize;
    for row in 0..frame.height() as usize {
        let start = row * frame.stride_bytes();
        for pixel in pixels[start..start + width * 4].as_chunks_mut::<4>().0 {
            for channel in &mut pixel[..3] {
                *channel = (f32::from(*channel) * gain).round().min(255.0) as u8;
            }
        }
    }
    frame
        .with_pixels(frame.stride_bytes(), pixels)
        .expect("scaling preserves the validated frame geometry")
}
