//! Sample-accurate audio access over avbridge's seekable decode session.

use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use avbridge::{AudioDecodeError, AudioDecodeSession};

use crate::{AudioBlock, AudioBlockError};

/// A request this far past the decoded range is reached by seeking, not by decoding through.
const FORWARD_WINDOW_SECS: f64 = 1.0;

/// Decoding starts this far before a seek target and the lead-in is discarded: lossy codecs
/// (AAC) output inaccurate samples for the first frames after a seek until their overlap state
/// is rebuilt.
const SEEK_PREROLL_SECS: f64 = 0.1;

/// Upper bound on blocks decoded for one read, so a hostile stream cannot spin a read forever.
const MAX_BLOCKS_PER_READ: u32 = 4096;

/// Failure reading audio, with the underlying native or validation error preserved.
#[derive(Debug)]
pub enum AudioSourceError {
    Decode(AudioDecodeError),
    InvalidBlock(AudioBlockError),
    /// The request would allocate more than the source's per-read frame budget.
    ReadTooLarge {
        frames: usize,
        budget_frames: usize,
    },
    /// A read decoded `MAX_BLOCKS_PER_READ` blocks without reaching the target.
    DecodeLimit,
}

impl From<AudioDecodeError> for AudioSourceError {
    fn from(error: AudioDecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<AudioBlockError> for AudioSourceError {
    fn from(error: AudioBlockError) -> Self {
        Self::InvalidBlock(error)
    }
}

/// One audio stream at a fixed output format. Reads are frame-indexed (`frame / sample_rate` is
/// the stream time); frames outside the stream are silence, so every read returns exactly the
/// requested length.
#[derive(Debug)]
pub struct AudioSource {
    session: AudioDecodeSession,
    sample_rate: u32,
    channels: u16,
    max_read_frames: usize,
    /// Decoded frames from `queue_start_frame` on, interleaved.
    queue: VecDeque<f32>,
    queue_start_frame: i64,
    /// Whether `queue_start_frame` is established (a block was decoded since the last seek).
    positioned: bool,
    end_of_stream: bool,
}

impl AudioSource {
    /// Opens `path`, resampling/remixing to `sample_rate`/`channels`. A single read may return
    /// at most `max_read_frames` frames.
    pub fn open(
        path: &Path,
        sample_rate: u32,
        channels: u16,
        max_read_frames: usize,
    ) -> Result<Self, AudioSourceError> {
        let (session, _) = AudioDecodeSession::open(path, sample_rate, channels)?;
        Ok(Self {
            session,
            sample_rate,
            channels,
            max_read_frames,
            queue: VecDeque::new(),
            queue_start_frame: 0,
            positioned: false,
            end_of_stream: false,
        })
    }

    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub const fn channels(&self) -> u16 {
        self.channels
    }

    /// Exactly `frames` frames starting at stream frame `start_frame`. `Ok(None)` means the
    /// read was cancelled.
    pub fn read(
        &mut self,
        start_frame: u64,
        frames: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<AudioBlock>, AudioSourceError> {
        if frames > self.max_read_frames {
            return Err(AudioSourceError::ReadTooLarge {
                frames,
                budget_frames: self.max_read_frames,
            });
        }
        let start = i64::try_from(start_frame).unwrap_or(i64::MAX / 2);
        let end = start.saturating_add(frames as i64);
        let channels = usize::from(self.channels);

        let forward_window = (FORWARD_WINDOW_SECS * f64::from(self.sample_rate)) as i64;
        if !self.positioned
            || start < self.queue_start_frame
            || start > self.decoded_end() + forward_window
        {
            let preroll_frames = (SEEK_PREROLL_SECS * f64::from(self.sample_rate)) as i64;
            self.session
                .seek((start - preroll_frames).max(0) as f64 / f64::from(self.sample_rate))?;
            self.queue.clear();
            self.positioned = false;
            self.end_of_stream = false;
        }

        let mut blocks = 0;
        while !self.end_of_stream && (!self.positioned || self.decoded_end() < end) {
            blocks += 1;
            if blocks > MAX_BLOCKS_PER_READ {
                return Err(AudioSourceError::DecodeLimit);
            }
            match self.session.next_block(cancel)? {
                Some(block) => self.append(block.timestamp_secs, &block.samples),
                None if cancel.load(Ordering::Acquire) => return Ok(None),
                None => self.end_of_stream = true,
            }
        }

        // Frames before `start` are never needed again by sequential playback.
        if self.positioned && start > self.queue_start_frame {
            let drop_frames =
                ((start - self.queue_start_frame) as usize).min(self.queue.len() / channels);
            self.queue.drain(..drop_frames * channels);
            self.queue_start_frame += drop_frames as i64;
        }

        let mut samples = vec![0.0_f32; frames * channels];
        if self.positioned {
            let available = (self.queue.len() / channels) as i64;
            let first = (start - self.queue_start_frame).max(0);
            for frame in first..(end - self.queue_start_frame).min(available) {
                let out_frame = (self.queue_start_frame + frame - start) as usize;
                for channel in 0..channels {
                    samples[out_frame * channels + channel] =
                        self.queue[frame as usize * channels + channel];
                }
            }
        }
        Ok(Some(AudioBlock::new(
            self.sample_rate,
            self.channels,
            start as f64 / f64::from(self.sample_rate),
            samples,
        )?))
    }

    fn decoded_end(&self) -> i64 {
        self.queue_start_frame + (self.queue.len() / usize::from(self.channels)) as i64
    }

    /// Appends one decoded block, aligning it to the stream by timestamp: leading frames before
    /// time zero (codec priming) are dropped, gaps are zero-filled, overlaps skipped.
    fn append(&mut self, timestamp_secs: f64, samples: &[f32]) {
        let channels = usize::from(self.channels);
        let mut block_start = (timestamp_secs * f64::from(self.sample_rate)).round() as i64;
        let mut samples = samples;
        if block_start < 0 {
            let skip = ((-block_start) as usize).min(samples.len() / channels);
            samples = &samples[skip * channels..];
            block_start = 0;
        }
        if !self.positioned {
            self.queue_start_frame = block_start;
            self.positioned = true;
        }
        let expected = self.decoded_end();
        if block_start > expected {
            let gap = (block_start - expected) as usize;
            self.queue.extend(std::iter::repeat_n(0.0, gap * channels));
        } else if block_start < expected {
            let overlap = ((expected - block_start) as usize).min(samples.len() / channels);
            samples = &samples[overlap * channels..];
        }
        self.queue.extend(samples.iter().copied());
    }
}
