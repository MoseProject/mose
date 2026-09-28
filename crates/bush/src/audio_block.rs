//! Validated PCM audio passed from a decoder to Bush's preview mixer.

/// An interleaved `f32` PCM block with an explicit timeline start.
#[derive(Clone, Debug, PartialEq)]
pub struct AudioBlock {
    sample_rate: u32,
    channels: u16,
    timestamp_secs: f64,
    samples: Vec<f32>,
}

impl AudioBlock {
    /// Validates decoder output before the real-time audio path consumes it.
    pub fn new(
        sample_rate: u32,
        channels: u16,
        timestamp_secs: f64,
        samples: Vec<f32>,
    ) -> Result<Self, AudioBlockError> {
        if sample_rate == 0 {
            return Err(AudioBlockError::ZeroSampleRate);
        }
        if channels == 0 {
            return Err(AudioBlockError::ZeroChannels);
        }
        if !timestamp_secs.is_finite() || timestamp_secs < 0.0 {
            return Err(AudioBlockError::InvalidTimestamp(timestamp_secs));
        }
        if !samples.len().is_multiple_of(usize::from(channels)) {
            return Err(AudioBlockError::PartialInterleavedFrame {
                sample_count: samples.len(),
                channels,
            });
        }
        if samples.iter().any(|sample| !sample.is_finite()) {
            return Err(AudioBlockError::NonFiniteSample);
        }

        Ok(Self {
            sample_rate,
            channels,
            timestamp_secs,
            samples,
        })
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub const fn channels(&self) -> u16 {
        self.channels
    }

    pub const fn timestamp_secs(&self) -> f64 {
        self.timestamp_secs
    }

    /// Number of complete PCM frames in this block.
    pub fn frame_count(&self) -> usize {
        self.samples.len() / usize::from(self.channels)
    }

    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    pub fn retained_bytes(&self) -> usize {
        self.samples.len() * std::mem::size_of::<f32>()
    }
}

/// PCM data rejected before mixing, cache accounting, or a native boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum AudioBlockError {
    ZeroSampleRate,
    ZeroChannels,
    InvalidTimestamp(f64),
    PartialInterleavedFrame { sample_count: usize, channels: u16 },
    NonFiniteSample,
}
