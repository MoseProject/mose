//! Pitch-preserving time stretch (WSOLA) for audio clips played at a speed other than 1.
//!
//! Waveform-similarity overlap-add: output is built from Hann-windowed frames taken from the
//! source at `speed` times the output rate, each frame nudged within a small search range so its
//! start lines up with the natural continuation of the previous one. State is a few windows of
//! samples, so memory is constant and sequential blocks join without clicks.

/// Output hop in frames; frames are two hops long (50 % overlap).
const HOP: usize = 512;
const WINDOW: usize = 2 * HOP;
/// Alignment search range either side of the nominal analysis position, in frames.
const TOLERANCE: usize = 256;

/// Speeds the stretcher supports; outside this range artefacts dominate.
pub const MIN_SPEED: f64 = 0.25;
pub const MAX_SPEED: f64 = 4.0;

#[derive(Clone, Debug)]
pub struct TimeStretcher {
    channels: usize,
    speed: f64,
    window: Vec<f32>,
    /// Nominal source position (in frames) of the next analysis frame.
    analysis_position: f64,
    /// Raw source samples the previous frame would have continued with (one hop, interleaved).
    natural: Option<Vec<f32>>,
    /// Overlap-add accumulator: the unfinished second half of the previous frame.
    overlap: Vec<f32>,
    /// Finished output not yet handed out (interleaved).
    ready: Vec<f32>,
}

impl TimeStretcher {
    /// `None` when the speed is outside [`MIN_SPEED`]..=[`MAX_SPEED`] or channels is zero.
    pub fn new(speed: f64, channels: usize) -> Option<Self> {
        if !speed.is_finite() || !(MIN_SPEED..=MAX_SPEED).contains(&speed) || channels == 0 {
            return None;
        }
        let window = (0..WINDOW)
            .map(|index| {
                0.5 - 0.5 * (2.0 * std::f64::consts::PI * index as f64 / WINDOW as f64).cos()
            })
            .map(|value| value as f32)
            .collect();
        Some(Self {
            channels,
            speed,
            window,
            analysis_position: 0.0,
            natural: None,
            overlap: vec![0.0; WINDOW * channels],
            ready: Vec::new(),
        })
    }

    pub const fn speed(&self) -> f64 {
        self.speed
    }

    /// Changes the speed for following output (a speed ramp). `false` if outside the supported
    /// range, in which case the speed is unchanged.
    pub fn set_speed(&mut self, speed: f64) -> bool {
        if !speed.is_finite() || !(MIN_SPEED..=MAX_SPEED).contains(&speed) {
            return false;
        }
        self.speed = speed;
        true
    }

    /// Restarts at `source_frame` (a seek or a first block): all history is discarded.
    pub fn reset(&mut self, source_frame: f64) {
        self.analysis_position = source_frame;
        self.natural = None;
        self.overlap.fill(0.0);
        self.ready.clear();
    }

    /// Produces exactly `frames` interleaved output frames, pulling source audio through
    /// `read(start_frame, frame_count)` (which must return `frame_count * channels` samples,
    /// zero padded outside the stream; `start_frame` may be negative). `None` means `read`
    /// reported cancellation.
    pub fn render<F>(&mut self, frames: usize, read: &mut F) -> Option<Vec<f32>>
    where
        F: FnMut(i64, usize) -> Option<Vec<f32>>,
    {
        let wanted = frames * self.channels;
        while self.ready.len() < wanted {
            self.produce_hop(read)?;
        }
        let output: Vec<f32> = self.ready.drain(..wanted).collect();
        Some(output)
    }

    fn produce_hop<F>(&mut self, read: &mut F) -> Option<()>
    where
        F: FnMut(i64, usize) -> Option<Vec<f32>>,
    {
        let channels = self.channels;
        let nominal = self.analysis_position.round() as i64;
        let region_start = nominal - TOLERANCE as i64;
        let region = read(region_start, WINDOW + 2 * TOLERANCE)?;
        debug_assert_eq!(region.len(), (WINDOW + 2 * TOLERANCE) * channels);

        let offset = match &self.natural {
            None => TOLERANCE,
            Some(natural) => best_alignment(&region, natural, channels),
        };
        let frame_start = offset * channels;

        // Window the chosen source frame and add it onto the overlap accumulator.
        for index in 0..WINDOW {
            let weight = self.window[index];
            for channel in 0..channels {
                self.overlap[index * channels + channel] +=
                    region[frame_start + index * channels + channel] * weight;
            }
        }
        // The first half is now complete; the second half waits for the next frame.
        self.ready
            .extend_from_slice(&self.overlap[..HOP * channels]);
        self.overlap.copy_within(HOP * channels.., 0);
        let tail_start = (WINDOW - HOP) * channels;
        self.overlap[tail_start..].fill(0.0);

        self.natural =
            Some(region[frame_start + HOP * channels..frame_start + 2 * HOP * channels].to_vec());
        self.analysis_position += HOP as f64 * self.speed;
        Some(())
    }
}

/// The offset in `0..=2 * TOLERANCE` (into `region`) whose first hop best matches `natural`,
/// by normalised cross-correlation of the channel mix. A coarse search at a stride of four
/// frames is refined around its best candidate, which costs a quarter of an exhaustive search
/// and finds the same peak for the smooth correlation curves of real audio.
fn best_alignment(region: &[f32], natural: &[f32], channels: usize) -> usize {
    let mix = |samples: &[f32]| -> Vec<f32> {
        samples
            .chunks_exact(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    };
    let target = mix(natural);
    let region = mix(region);
    let score = |candidate: usize| -> f32 {
        let mut dot = 0.0_f32;
        let mut energy = 1e-9_f32;
        for (frame, target_sample) in target.iter().enumerate() {
            let sample = region[candidate + frame];
            dot += sample * target_sample;
            energy += sample * sample;
        }
        dot / energy.sqrt()
    };
    let nominal_distance = |candidate: usize| candidate.abs_diff(TOLERANCE);
    let mut best = (TOLERANCE, f32::MIN);
    let consider = |candidate: usize, best: &mut (usize, f32)| {
        let value = score(candidate);
        // Prefer the nominal position on ties so silence does not wander.
        if value > best.1
            || (value == best.1 && nominal_distance(candidate) < nominal_distance(best.0))
        {
            *best = (candidate, value);
        }
    };
    for candidate in (0..=2 * TOLERANCE).step_by(4) {
        consider(candidate, &mut best);
    }
    let coarse = best.0;
    for candidate in coarse.saturating_sub(3)..=(coarse + 3).min(2 * TOLERANCE) {
        consider(candidate, &mut best);
    }
    best.0
}
