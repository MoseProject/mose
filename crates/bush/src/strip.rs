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
/// `acompressor` and `sidechaincompress` follow the level faster than their millisecond settings
/// say (their per-sample coefficient is about four times larger). 0.35 of the setting is the
/// scale that best fits the export's ducking, measured by `tests/duck_parity_test.rs`: the music
/// recovers within 1.6 dB of the export at every point after the commentary ends.
const TIME_CONSTANT_SCALE: f64 = 0.35;
const ATTACK_SECS: f64 = 0.010 * TIME_CONSTANT_SCALE;
const RELEASE_SECS: f64 = 0.250 * TIME_CONSTANT_SCALE;
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
    /// Linear gain after the compressor; `1.0` adds none.
    pub comp_makeup: f32,
    /// Downward gate: below this level in dB the signal is lowered by [`GATE_REDUCTION_DB`], an
    /// approximation of the export's `afftdn` denoise. `0.0` is off.
    pub gate_floor_db: f32,
    /// Hard ceiling on the output, linear; `0.0` is off.
    pub limit: f32,
}

/// How far the gate lowers what is under its floor, the amount `afftdn` removes by default.
pub const GATE_REDUCTION_DB: f64 = 12.0;

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
            comp_makeup: 1.0,
            gate_floor_db: 0.0,
            limit: 0.0,
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
            && self.gate_floor_db >= 0.0
            && self.limit <= 0.0
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
            if strip.gate_floor_db < 0.0 || strip.comp_ratio > 1.0 {
                let gain = self.dynamics_gain(frame, strip);
                for sample in frame.iter_mut() {
                    *sample *= gain;
                }
            }
            if strip.limit > 0.0 {
                for sample in frame.iter_mut() {
                    *sample = sample.clamp(-strip.limit, strip.limit);
                }
            }
        }
    }

    /// Gate and compressor gain for one frame, from the one level follower.
    fn dynamics_gain(&mut self, frame: &[f32], strip: Strip) -> f32 {
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
        let mut gain_db = 0.0;
        if strip.gate_floor_db < 0.0 && level_db < f64::from(strip.gate_floor_db) {
            gain_db -= GATE_REDUCTION_DB;
        }
        if strip.comp_ratio <= 1.0 {
            return 10f64.powf(gain_db / 20.0) as f32;
        }
        gain_db += compression_db(
            level_db,
            f64::from(strip.comp_threshold_db),
            f64::from(strip.comp_ratio),
        );
        (10f64.powf(gain_db / 20.0) * f64::from(strip.comp_makeup.max(0.0))) as f32
    }
}

/// The gain change in dB a soft-knee compressor makes at `level_db`.
fn compression_db(level_db: f64, threshold_db: f64, ratio: f64) -> f64 {
    let over = level_db - threshold_db;
    let output_db = if 2.0 * over < -KNEE_DB {
        level_db
    } else if 2.0 * over.abs() <= KNEE_DB {
        level_db + (1.0 / ratio - 1.0) * (over + KNEE_DB / 2.0).powi(2) / (2.0 * KNEE_DB)
    } else {
        threshold_db + over / ratio
    };
    output_db - level_db
}

/// Sidechain ducking, as the export's `sidechaincompress` is set: threshold 0.05 linear, ratio 8,
/// attack 5 ms, release 250 ms, RMS detection averaged over the channels.
#[derive(Debug, Clone, Default)]
pub struct DuckState {
    mean_square: f64,
    /// Timeline frame the next window must start at for the follower to be continuous.
    pub next_frame: u64,
}

const DUCK_THRESHOLD_DB: f64 = -26.020_599_913_279_625;
const DUCK_RATIO: f64 = 8.0;
const DUCK_ATTACK_SECS: f64 = 0.005 * TIME_CONSTANT_SCALE;
const DUCK_RELEASE_SECS: f64 = RELEASE_SECS;

impl DuckState {
    /// One gain per frame of `trigger` (interleaved), to multiply the ducked signal by.
    pub fn gains(&mut self, trigger: &[f32], channels: usize, rate: u32) -> Vec<f32> {
        let rate = f64::from(rate);
        let attack = (-1.0 / (DUCK_ATTACK_SECS * rate)).exp();
        let release = (-1.0 / (DUCK_RELEASE_SECS * rate)).exp();
        trigger
            .chunks_exact(channels.max(1))
            .map(|frame| {
                let power =
                    frame.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / frame.len() as f64;
                let coefficient = if power > self.mean_square {
                    attack
                } else {
                    release
                };
                self.mean_square = coefficient * self.mean_square + (1.0 - coefficient) * power;
                let level_db = 10.0 * self.mean_square.max(1e-12).log10();
                10f64.powf(compression_db(level_db, DUCK_THRESHOLD_DB, DUCK_RATIO) / 20.0) as f32
            })
            .collect()
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
