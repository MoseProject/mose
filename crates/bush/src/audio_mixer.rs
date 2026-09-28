//! Bounded, headless PCM mixer for Bush preview audio.

use crate::{AudioBlock, AudioBlockError};

/// One PCM contributor and its timeline mix controls.
#[derive(Clone, Copy, Debug)]
pub struct MixInput<'a> {
    pub block: &'a AudioBlock,
    pub gain: f32,
    pub muted: bool,
}

/// A fixed-format audio mixer that needs no device to produce deterministic preview PCM.
#[derive(Clone, Copy, Debug)]
pub struct AudioMixer {
    sample_rate: u32,
    channels: u16,
    max_output_frames: usize,
}

/// Input incompatible with this mixer or output rejected by its frame budget.
#[derive(Clone, Debug, PartialEq)]
pub enum MixError {
    ZeroOutputBudget,
    InvalidGain(f32),
    SampleRateMismatch {
        expected: u32,
        actual: u32,
    },
    ChannelMismatch {
        expected: u16,
        actual: u16,
    },
    NonFrameAlignedTimestamp(f64),
    OutputExceedsBudget {
        requested_frames: usize,
        budget_frames: usize,
    },
    Block(AudioBlockError),
}

impl From<AudioBlockError> for MixError {
    fn from(error: AudioBlockError) -> Self {
        Self::Block(error)
    }
}

impl AudioMixer {
    /// Creates a fixed-format mixer with a hard per-mix frame bound.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        max_output_frames: usize,
    ) -> Result<Self, MixError> {
        if max_output_frames == 0 {
            return Err(MixError::ZeroOutputBudget);
        }
        AudioBlock::new(sample_rate, channels, 0.0, Vec::new())?;
        Ok(Self {
            sample_rate,
            channels,
            max_output_frames,
        })
    }

    /// Mixes timeline-aligned interleaved PCM blocks with a hard limiter at `[-1.0, 1.0]`.
    pub fn mix(&self, inputs: &[MixInput<'_>]) -> Result<AudioBlock, MixError> {
        for input in inputs {
            if !input.gain.is_finite() {
                return Err(MixError::InvalidGain(input.gain));
            }
            if input.block.sample_rate() != self.sample_rate {
                return Err(MixError::SampleRateMismatch {
                    expected: self.sample_rate,
                    actual: input.block.sample_rate(),
                });
            }
            if input.block.channels() != self.channels {
                return Err(MixError::ChannelMismatch {
                    expected: self.channels,
                    actual: input.block.channels(),
                });
            }
        }
        let start_secs = inputs
            .iter()
            .map(|input| input.block.timestamp_secs())
            .min_by(f64::total_cmp)
            .unwrap_or(0.0);
        let offsets: Vec<usize> = inputs
            .iter()
            .map(|input| {
                timestamp_offset(input.block.timestamp_secs(), start_secs, self.sample_rate)
            })
            .collect::<Result<_, _>>()?;
        let requested_frames = inputs
            .iter()
            .zip(&offsets)
            .map(|(input, offset)| offset.saturating_add(input.block.frame_count()))
            .max()
            .unwrap_or(0);
        if requested_frames > self.max_output_frames {
            return Err(MixError::OutputExceedsBudget {
                requested_frames,
                budget_frames: self.max_output_frames,
            });
        }

        let channels = usize::from(self.channels);
        let mut output = vec![0.0; requested_frames * channels];
        for (input, offset) in inputs.iter().zip(offsets) {
            if input.muted {
                continue;
            }
            for (sample_index, sample) in input.block.samples().iter().enumerate() {
                let output_index = offset * channels + sample_index;
                output[output_index] =
                    (output[output_index] + sample * input.gain).clamp(-1.0, 1.0);
            }
        }
        AudioBlock::new(self.sample_rate, self.channels, start_secs, output).map_err(Into::into)
    }
}

fn timestamp_offset(
    timestamp_secs: f64,
    start_secs: f64,
    sample_rate: u32,
) -> Result<usize, MixError> {
    let offset = (timestamp_secs - start_secs) * f64::from(sample_rate);
    let rounded = offset.round();
    if (offset - rounded).abs() > 1e-6 || rounded < 0.0 || rounded > usize::MAX as f64 {
        return Err(MixError::NonFrameAlignedTimestamp(timestamp_secs));
    }
    Ok(rounded as usize)
}
