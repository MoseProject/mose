//! A mixer strip for the preview: high-pass, low-pass, three-band EQ and a compressor, with the
//! same corner frequencies and shapes as the export mix's filter chain (`audio_mix.c`), so what
//! is heard while editing is what the export writes.

use crate::ContributorId;
use std::collections::HashMap;

const LOW_SHELF_HZ: f64 = 120.0;
const MID_BELL_HZ: f64 = 1_000.0;
const HIGH_SHELF_HZ: f64 = 6_000.0;
/// Shelf quality factor (width 0.5 in Q terms) and bell Q, as the export filters are set.
const SHELF_Q: f64 = 0.5;
const BELL_Q: f64 = 1.0;
const PASS_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
const ATTACK_SECS: f64 = 0.010;
const RELEASE_SECS: f64 = 0.250;
/// The compressor's soft knee, 2.82843 linear (about 9 dB) as `acompressor`'s default.
const KNEE_DB: f64 = 9.03;

/// The tone and dynamics of one track. The neutral value changes nothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Strip {
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    /// Corner in Hz; `0.0` leaves that end open.
    pub highpass_hz: f32,
    pub lowpass_hz: f32,
    pub comp_threshold_db: f32,
    /// `1.0` or less is off.
    pub comp_ratio: f32,
}

impl Default for Strip {
    fn default() -> Self {
        Self {
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            highpass_hz: 0.0,
            lowpass_hz: 0.0,
            comp_threshold_db: -18.0,
            comp_ratio: 1.0,
        }
    }
}

impl Strip {
    pub fn is_neutral(&self) -> bool {
        self.eq_low_db == 0.0
            && self.eq_mid_db == 0.0
            && self.eq_high_db == 0.0
            && self.highpass_hz <= 0.0
            && self.lowpass_hz <= 0.0
            && self.comp_ratio <= 1.0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct Coefficients {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
}

#[derive(Debug, Clone, Copy, Default)]
struct Biquad {
    coefficients: Coefficients,
    s1: f64,
    s2: f64,
}

impl Biquad {
    fn process(&mut self, x: f64) -> f64 {
        let c = &self.coefficients;
        let y = c.b0 * x + self.s1;
        self.s1 = c.b1 * x - c.a1 * y + self.s2;
        self.s2 = c.b2 * x - c.a2 * y;
        y
    }
}

fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Coefficients {
    Coefficients {
        b0: b0 / a0,
        b1: b1 / a0,
        b2: b2 / a0,
        a1: a1 / a0,
        a2: a2 / a0,
    }
}

fn high_pass(hz: f64, rate: f64) -> Coefficients {
    let w0 = 2.0 * std::f64::consts::PI * hz / rate;
    let alpha = w0.sin() / (2.0 * PASS_Q);
    let c = w0.cos();
    normalized(
        (1.0 + c) / 2.0,
        -(1.0 + c),
        (1.0 + c) / 2.0,
        1.0 + alpha,
        -2.0 * c,
        1.0 - alpha,
    )
}

fn low_pass(hz: f64, rate: f64) -> Coefficients {
    let w0 = 2.0 * std::f64::consts::PI * hz / rate;
    let alpha = w0.sin() / (2.0 * PASS_Q);
    let c = w0.cos();
    normalized(
        (1.0 - c) / 2.0,
        1.0 - c,
        (1.0 - c) / 2.0,
        1.0 + alpha,
        -2.0 * c,
        1.0 - alpha,
    )
}

fn bell(hz: f64, gain_db: f64, rate: f64) -> Coefficients {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * hz / rate;
    let alpha = w0.sin() / (2.0 * BELL_Q);
    let c = w0.cos();
    normalized(
        1.0 + alpha * a,
        -2.0 * c,
        1.0 - alpha * a,
        1.0 + alpha / a,
        -2.0 * c,
        1.0 - alpha / a,
    )
}

fn shelf(hz: f64, gain_db: f64, rate: f64, high: bool) -> Coefficients {
    let a = 10f64.powf(gain_db / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * hz / rate;
    let alpha = w0.sin() / (2.0 * SHELF_Q);
    let c = w0.cos();
    let beta = 2.0 * a.sqrt() * alpha;
    if high {
        normalized(
            a * ((a + 1.0) + (a - 1.0) * c + beta),
            -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
            a * ((a + 1.0) + (a - 1.0) * c - beta),
            (a + 1.0) - (a - 1.0) * c + beta,
            2.0 * ((a - 1.0) - (a + 1.0) * c),
            (a + 1.0) - (a - 1.0) * c - beta,
        )
    } else {
        normalized(
            a * ((a + 1.0) - (a - 1.0) * c + beta),
            2.0 * a * ((a - 1.0) - (a + 1.0) * c),
            a * ((a + 1.0) - (a - 1.0) * c - beta),
            (a + 1.0) + (a - 1.0) * c + beta,
            -2.0 * ((a - 1.0) + (a + 1.0) * c),
            (a + 1.0) + (a - 1.0) * c - beta,
        )
    }
}

/// The running state of one contributor's strip: filter memories per channel and the compressor's
/// level follower. Kept between blocks so there is no click at block edges.
#[derive(Debug, Clone)]
pub struct StripState {
    rate: f64,
    channels: usize,
    applied: Option<Strip>,
    /// One chain of filters per channel, in processing order.
    chains: Vec<Vec<Biquad>>,
    /// Smoothed mean square of the detector (the loudest-channel average).
    mean_square: f64,
    /// Timeline frame the next block must start at for the state to be continuous.
    pub next_frame: u64,
}

impl StripState {
    pub fn new(rate: u32, channels: usize) -> Self {
        Self {
            rate: f64::from(rate),
            channels: channels.max(1),
            applied: None,
            chains: vec![Vec::new(); channels.max(1)],
            mean_square: 0.0,
            next_frame: 0,
        }
    }

    fn configure(&mut self, strip: Strip) {
        if self.applied == Some(strip) {
            return;
        }
        let rate = self.rate;
        let nyquist = rate / 2.0 - 1.0;
        let mut stages: Vec<Coefficients> = Vec::new();
        if strip.highpass_hz > 0.0 {
            stages.push(high_pass(f64::from(strip.highpass_hz).min(nyquist), rate));
        }
        if strip.lowpass_hz > 0.0 {
            stages.push(low_pass(f64::from(strip.lowpass_hz).min(nyquist), rate));
        }
        if strip.eq_low_db != 0.0 {
            stages.push(shelf(LOW_SHELF_HZ, f64::from(strip.eq_low_db), rate, false));
        }
        if strip.eq_mid_db != 0.0 {
            stages.push(bell(MID_BELL_HZ, f64::from(strip.eq_mid_db), rate));
        }
        if strip.eq_high_db != 0.0 {
            stages.push(shelf(
                HIGH_SHELF_HZ,
                f64::from(strip.eq_high_db),
                rate,
                true,
            ));
        }
        // Keep the memory of stages that stay when only a value moves; a changed stage count
        // starts the chain clean (a knob move never lands on silence for long).
        for chain in &mut self.chains {
            if chain.len() == stages.len() {
                for (biquad, coefficients) in chain.iter_mut().zip(&stages) {
                    biquad.coefficients = *coefficients;
                }
            } else {
                *chain = stages
                    .iter()
                    .map(|coefficients| Biquad {
                        coefficients: *coefficients,
                        ..Biquad::default()
                    })
                    .collect();
            }
        }
        self.applied = Some(strip);
    }

    /// Forgets the filter and follower memory, for a discontinuity.
    pub fn reset(&mut self) {
        for chain in &mut self.chains {
            for biquad in chain {
                biquad.s1 = 0.0;
                biquad.s2 = 0.0;
            }
        }
        self.mean_square = 0.0;
    }

    /// Runs the strip over interleaved `samples` in place.
    pub fn process(&mut self, samples: &mut [f32], strip: Strip) {
        if strip.is_neutral() {
            return;
        }
        self.configure(strip);
        let channels = self.channels;
        for frame in samples.chunks_exact_mut(channels) {
            for (sample, chain) in frame.iter_mut().zip(&mut self.chains) {
                let mut x = f64::from(*sample);
                for biquad in chain.iter_mut() {
                    x = biquad.process(x);
                }
                *sample = x as f32;
            }
            if strip.comp_ratio > 1.0 {
                let gain = self.compressor_gain(frame, strip);
                for sample in frame.iter_mut() {
                    *sample *= gain;
                }
            }
        }
    }

    fn compressor_gain(&mut self, frame: &[f32], strip: Strip) -> f32 {
        let power = frame
            .iter()
            .map(|s| f64::from(*s) * f64::from(*s))
            .sum::<f64>()
            / frame.len() as f64;
        let attack = (-1.0 / (ATTACK_SECS * self.rate)).exp();
        let release = (-1.0 / (RELEASE_SECS * self.rate)).exp();
        let coefficient = if power > self.mean_square {
            attack
        } else {
            release
        };
        self.mean_square = coefficient * self.mean_square + (1.0 - coefficient) * power;
        let level_db = 10.0 * self.mean_square.max(1e-12).log10();
        let threshold = f64::from(strip.comp_threshold_db);
        let ratio = f64::from(strip.comp_ratio);
        let over = level_db - threshold;
        let output_db = if 2.0 * over < -KNEE_DB {
            level_db
        } else if 2.0 * over.abs() <= KNEE_DB {
            level_db + (1.0 / ratio - 1.0) * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
        } else {
            threshold + over / ratio
        };
        10f64.powf((output_db - level_db) / 20.0) as f32
    }
}

/// Strip states by contributor, so every clip keeps its own filter memory.
#[derive(Debug, Default)]
pub struct StripBank {
    states: HashMap<ContributorId, StripState>,
}

impl StripBank {
    /// Runs `strip` over one contributor's block starting at timeline frame `start_frame`.
    pub fn process(
        &mut self,
        id: ContributorId,
        rate: u32,
        channels: usize,
        start_frame: u64,
        samples: &mut [f32],
        strip: Strip,
    ) {
        if strip.is_neutral() {
            self.states.remove(&id);
            return;
        }
        let frames = (samples.len() / channels.max(1)) as u64;
        let state = self
            .states
            .entry(id)
            .or_insert_with(|| StripState::new(rate, channels));
        if state.next_frame != start_frame {
            state.reset();
        }
        state.process(samples, strip);
        state.next_frame = start_frame + frames;
    }

    /// Drops a contributor that left the scene.
    pub fn forget(&mut self, id: ContributorId) {
        self.states.remove(&id);
    }
}
