//! Where mixed audio goes: the [AudioSink] trait, its error type and a deterministic in-memory
//! [FakeSink] for tests. The real device implementation lives in device_sink (feature cpal).

use std::collections::VecDeque;

use crate::AudioBlock;

/// Destination for mixed audio. Implemented by a device callback bridge in production.
pub trait AudioSink {
    /// Queues one block. Must not block; a full sink reports [`SinkError::Overrun`].
    fn write(&mut self, block: &AudioBlock) -> Result<(), SinkError>;
    /// Frames queued but not yet played.
    fn buffered_frames(&self) -> usize;
    /// Drops everything queued (seek, scene replacement).
    fn clear(&mut self);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SinkError {
    /// The device was lost or the sink shut down; further writes will not succeed.
    Closed,
    Overrun {
        capacity_frames: usize,
        requested_frames: usize,
    },
}

/// Deterministic in-memory sink with a bounded queue and explicit underflow accounting.
#[derive(Debug)]
pub struct FakeSink {
    capacity_frames: usize,
    channels: u16,
    queued: VecDeque<f32>,
    played: Vec<f32>,
    underflows: u64,
    closed: bool,
}

impl FakeSink {
    pub fn new(capacity_frames: usize, channels: u16) -> Self {
        Self {
            capacity_frames,
            channels,
            queued: VecDeque::new(),
            played: Vec::new(),
            underflows: 0,
            closed: false,
        }
    }

    /// Simulates the device pulling `frames`. Missing frames are played as silence and counted
    /// as one underflow. Returns the frames that were real audio.
    pub fn consume(&mut self, frames: usize) -> usize {
        let wanted = frames * usize::from(self.channels);
        let available = self.queued.len().min(wanted);
        self.played.extend(self.queued.drain(..available));
        if available < wanted {
            self.underflows += 1;
            self.played
                .resize(self.played.len() + (wanted - available), 0.0);
        }
        available / usize::from(self.channels)
    }

    /// Simulates device loss: every later write fails with [`SinkError::Closed`].
    pub fn lose_device(&mut self) {
        self.closed = true;
    }

    pub fn underflows(&self) -> u64 {
        self.underflows
    }

    /// Every interleaved sample the simulated device has played, silence included.
    pub fn played(&self) -> &[f32] {
        &self.played
    }
}

impl AudioSink for FakeSink {
    fn write(&mut self, block: &AudioBlock) -> Result<(), SinkError> {
        if self.closed {
            return Err(SinkError::Closed);
        }
        if self.buffered_frames() + block.frame_count() > self.capacity_frames {
            return Err(SinkError::Overrun {
                capacity_frames: self.capacity_frames,
                requested_frames: block.frame_count(),
            });
        }
        self.queued.extend(block.samples().iter().copied());
        Ok(())
    }

    fn buffered_frames(&self) -> usize {
        self.queued.len() / usize::from(self.channels)
    }

    fn clear(&mut self) {
        self.queued.clear();
    }
}
