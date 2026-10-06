// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! ITU-R BS.1770 K-weighting: the pre-filter (a high shelf) followed by the RLB high-pass,
//! applied per channel. The mean square of the weighted signal gives loudness in LUFS, which the
//! preview's level meter shows next to the peaks.

use std::f64::consts::PI;

/// Offset that calibrates the mean square to LUFS, per BS.1770.
const LUFS_OFFSET: f64 = -0.691;

#[derive(Clone, Copy, Debug)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
}

/// Direct form I memory of one biquad on one channel.
#[derive(Clone, Copy, Debug, Default)]
struct State {
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    fn process(&self, state: &mut State, x: f64) -> f64 {
        let y = self.b[0] * x + self.b[1] * state.x[0] + self.b[2] * state.x[1]
            - self.a[0] * state.y[0]
            - self.a[1] * state.y[1];
        state.x = [x, state.x[0]];
        state.y = [y, state.y[0]];
        y
    }
}

/// Stage 1, the shelf that models the head's acoustic effect, for `rate` Hz.
fn pre_filter(rate: f64) -> Biquad {
    let f0 = 1_681.974_450_955_533;
    let gain_db = 3.999_843_853_973_347;
    let q = 0.707_175_236_955_419_6;
    let k = (PI * f0 / rate).tan();
    let vh = 10.0_f64.powf(gain_db / 20.0);
    let vb = vh.powf(0.499_666_774_154_541_6);
    let a0 = 1.0 + k / q + k * k;
    Biquad {
        b: [
            (vh + vb * k / q + k * k) / a0,
            2.0 * (k * k - vh) / a0,
            (vh - vb * k / q + k * k) / a0,
        ],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    }
}

/// Stage 2, the revised low-frequency B-curve (a high-pass), for `rate` Hz.
fn rlb_filter(rate: f64) -> Biquad {
    let f0 = 38.135_470_876_024_44;
    let q = 0.500_327_037_323_877_3;
    let k = (PI * f0 / rate).tan();
    let a0 = 1.0 + k / q + k * k;
    Biquad {
        b: [1.0, -2.0, 1.0],
        a: [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
    }
}

/// K-weights up to two channels; the filters keep their memory between blocks, so consecutive
/// blocks are weighted as one continuous signal.
#[derive(Clone, Debug)]
pub struct KWeighting {
    pre: Biquad,
    rlb: Biquad,
    states: [[State; 2]; 2],
}

impl KWeighting {
    pub fn new(sample_rate: u32) -> Self {
        let rate = f64::from(sample_rate.max(1));
        Self {
            pre: pre_filter(rate),
            rlb: rlb_filter(rate),
            states: [[State::default(); 2]; 2],
        }
    }

    /// Forgets the filter memory, for a jump in the signal (a seek or a scene change).
    pub fn reset(&mut self) {
        self.states = [[State::default(); 2]; 2];
    }

    /// Sum over frames of the weighted left and right squares for an interleaved block. A mono
    /// block counts on both sides, the way it is played; channels past the second are ignored.
    pub fn energy(&mut self, samples: &[f32], channels: usize) -> f64 {
        let channels = channels.max(1);
        let right_index = channels.min(2) - 1;
        let mut total = 0.0;
        for frame in samples.chunks_exact(channels) {
            for (side, index) in [0, right_index].into_iter().enumerate() {
                let staged = self
                    .pre
                    .process(&mut self.states[side][0], f64::from(frame[index]));
                let weighted = self.rlb.process(&mut self.states[side][1], staged);
                total += weighted * weighted;
            }
        }
        total
    }
}

/// Loudness in LUFS of `energy` (a sum from [`KWeighting::energy`]) spread over `frames`;
/// `None` for silence or no audio.
pub fn lufs(energy: f64, frames: u64) -> Option<f32> {
    if frames == 0 || energy <= 0.0 {
        return None;
    }
    Some((LUFS_OFFSET + 10.0 * (energy / frames as f64).log10()) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f64, amplitude: f64, frames: usize) -> Vec<f32> {
        (0..frames)
            .flat_map(|n| {
                let v = (amplitude * (2.0 * PI * hz * n as f64 / f64::from(rate)).sin()) as f32;
                [v, v]
            })
            .collect()
    }

    #[test]
    fn the_filters_match_the_published_48k_coefficients() {
        let pre = pre_filter(48_000.0);
        assert!((pre.b[0] - 1.535_124_859_586_97).abs() < 1e-9);
        assert!((pre.b[1] + 2.691_696_189_406_38).abs() < 1e-9);
        assert!((pre.b[2] - 1.198_392_810_852_85).abs() < 1e-9);
        assert!((pre.a[0] + 1.690_659_293_182_41).abs() < 1e-9);
        assert!((pre.a[1] - 0.732_480_774_215_85).abs() < 1e-9);
        let rlb = rlb_filter(48_000.0);
        assert!((rlb.a[0] + 1.990_047_454_833_98).abs() < 1e-9);
        assert!((rlb.a[1] - 0.990_072_250_366_21).abs() < 1e-9);
    }

    #[test]
    fn a_stereo_sine_reads_close_to_its_known_loudness() {
        // 0 dBFS, 997 Hz, in both channels: -3.01 dB RMS, +0.7 dB K gain, +3.01 dB for two
        // channels, -0.691 offset: about 0 LUFS.
        let mut weighting = KWeighting::new(48_000);
        let samples = sine(48_000, 997.0, 1.0, 48_000);
        let reading = lufs(weighting.energy(&samples, 2), 48_000).unwrap();
        assert!(reading.abs() < 0.2, "got {reading}");
    }

    #[test]
    fn halving_the_amplitude_lowers_the_reading_by_six_db() {
        let loud = KWeighting::new(48_000).energy(&sine(48_000, 997.0, 0.5, 48_000), 2);
        let quiet = KWeighting::new(48_000).energy(&sine(48_000, 997.0, 0.25, 48_000), 2);
        let diff = lufs(loud, 48_000).unwrap() - lufs(quiet, 48_000).unwrap();
        assert!((diff - 6.02).abs() < 0.05, "got {diff}");
    }

    #[test]
    fn silence_and_empty_input_have_no_reading() {
        assert_eq!(lufs(0.0, 1_000), None);
        assert_eq!(lufs(1.0, 0), None);
        let mut weighting = KWeighting::new(44_100);
        assert_eq!(weighting.energy(&[0.0; 2_048], 2), 0.0);
    }

    #[test]
    fn mono_counts_on_both_sides() {
        let stereo = KWeighting::new(48_000).energy(&sine(48_000, 997.0, 0.5, 4_800), 2);
        let mono: Vec<f32> = sine(48_000, 997.0, 0.5, 4_800)
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| pair[0])
            .collect();
        let mono_energy = KWeighting::new(48_000).energy(&mono, 1);
        assert!((stereo - mono_energy).abs() < 1e-6);
    }

    #[test]
    fn reset_forgets_the_filter_memory() {
        let mut weighting = KWeighting::new(48_000);
        let first = weighting.energy(&sine(48_000, 200.0, 0.8, 2_048), 2);
        weighting.reset();
        let again = weighting.energy(&sine(48_000, 200.0, 0.8, 2_048), 2);
        assert!((first - again).abs() < 1e-9);
    }
}
