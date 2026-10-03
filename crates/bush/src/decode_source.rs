//! Bush's standalone adapter over avbridge's worker-owned FFmpeg decode session.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use avbridge::{Advance, DecodeError, DecodePolicy, DecodeSession, FrameSlot};

use crate::{PixelFormat, VideoFrame, VideoFrameError};

/// Stable metadata for an opened preview source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInfo {
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub duration_secs: Option<f64>,
    /// Whether frames are decoded on a hardware device.
    pub hardware: bool,
}

/// Upper bound on frames decoded forward from a key unit to reach an exact target. Bounds
/// worst-case latency on hostile or very long-GOP media instead of decoding to EOF.
pub const MAX_SEEK_SEARCH_FRAMES: u32 = 600;

/// A target at most this far ahead of the presented frame is reached by decoding forward
/// instead of seeking: sequential playback never pays a key-unit seek per frame.
const FORWARD_WINDOW_SECS: f64 = 1.0;

/// Tolerance when comparing a frame timestamp against a requested target.
const TIMESTAMP_EPSILON_SECS: f64 = 1e-6;

/// A one-video-source decoder owned by one Bush worker.
#[derive(Debug)]
pub struct DecodeSource {
    session: DecodeSession,
    info: SourceInfo,
    /// The frame most recently returned by [`Self::frame_at`].
    current: Option<Arc<VideoFrame>>,
    /// Timestamp of a frame the native session decoded past `current` but has not converted
    /// yet; it is converted only if a later request presents it.
    pending_timestamp: Option<f64>,
}

/// Decode failure or invalid native frame data rejected at Bush's boundary.
#[derive(Debug)]
pub enum DecodeSourceError {
    Decode(DecodeError),
    InvalidFrame(VideoFrameError),
    /// Frame-accurate seek decoded `limit` frames without reaching the target.
    SeekSearchLimit {
        limit: u32,
    },
}

impl From<DecodeError> for DecodeSourceError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

impl From<VideoFrameError> for DecodeSourceError {
    fn from(error: VideoFrameError) -> Self {
        Self::InvalidFrame(error)
    }
}

impl DecodeSource {
    /// Opens `path`; native validation errors are surfaced, never hidden by a fallback.
    pub fn open(path: &Path) -> Result<Self, DecodeSourceError> {
        Self::open_with_policy(path, DecodePolicy::Software)
    }

    /// Like [`Self::open`], choosing software or hardware-with-fallback decode.
    pub fn open_with_policy(path: &Path, policy: DecodePolicy) -> Result<Self, DecodeSourceError> {
        let (session, info) = DecodeSession::open_with_policy(path, policy)?;
        Ok(Self {
            session,
            info: SourceInfo {
                width: info.width,
                height: info.height,
                fps: info.fps,
                duration_secs: info.duration_secs,
                hardware: info.hardware,
            },
            current: None,
            pending_timestamp: None,
        })
    }

    pub const fn info(&self) -> SourceInfo {
        self.info
    }

    /// Seeks to the key unit at or before `timestamp_secs`.
    pub fn seek(&mut self, timestamp_secs: f64) -> Result<(), DecodeSourceError> {
        self.current = None;
        self.pending_timestamp = None;
        self.session.seek(timestamp_secs)?;
        Ok(())
    }

    /// Decodes the next frame in stream order. `Ok(None)` means cancelled or end of stream.
    pub fn next_frame(
        &mut self,
        cancel: &AtomicBool,
    ) -> Result<Option<VideoFrame>, DecodeSourceError> {
        // The native session returns a frame it decoded ahead before decoding a new one.
        self.current = None;
        self.pending_timestamp = None;
        self.decode_next(cancel)
    }

    fn decode_next(
        &mut self,
        cancel: &AtomicBool,
    ) -> Result<Option<VideoFrame>, DecodeSourceError> {
        self.session
            .next_rgba(cancel)?
            .map(|frame| {
                VideoFrame::new(
                    PixelFormat::Rgba8,
                    frame.width,
                    frame.height,
                    frame.stride_bytes,
                    frame.timestamp_secs,
                    frame.pixels,
                )
                .map_err(Into::into)
            })
            .transpose()
    }

    /// Frame-accurate access: the frame presented at `target_secs`, i.e. the last decoded frame
    /// with `timestamp <= target`. A target shortly after the previously presented frame is
    /// reached by decoding forward (and a repeated target is free); anything else seeks to the
    /// preceding key unit and decodes forward. Intermediate frames are decoded but never
    /// converted to RGBA: only the frame that is finally presented is. If the first frame is
    /// already past the target (a target before the stream start) it is returned.
    /// `Ok(None)` means cancelled, or the stream ended before producing any frame.
    pub fn frame_at(
        &mut self,
        target_secs: f64,
        cancel: &AtomicBool,
    ) -> Result<Option<Arc<VideoFrame>>, DecodeSourceError> {
        let continues_forward = self.current.as_ref().is_some_and(|current| {
            let ahead = target_secs - current.timestamp_secs();
            (-TIMESTAMP_EPSILON_SECS..=FORWARD_WINDOW_SECS).contains(&ahead)
        });
        if !continues_forward {
            self.seek(target_secs)?;
        }

        // Whether the native newest frame is a candidate (timestamp <= target) rather than an
        // overshoot or nothing.
        let mut newest_is_candidate = false;
        for _ in 0..MAX_SEEK_SEARCH_FRAMES {
            let timestamp = match self.pending_timestamp {
                Some(timestamp) => timestamp,
                None => match self.session.advance(cancel)? {
                    Advance::Frame(timestamp) => timestamp,
                    Advance::Cancelled => return Ok(None),
                    Advance::EndOfStream => {
                        // The newest decoded frame is the last one: present it if it was a
                        // candidate, else whatever was presented before.
                        if newest_is_candidate {
                            self.current = self.take(FrameSlot::Newest)?;
                        }
                        return Ok(self.current.clone());
                    }
                },
            };
            if timestamp > target_secs + TIMESTAMP_EPSILON_SECS {
                // The newest frame overshoots, so the frame before it is the answer.
                if self.session.has_held_frame() {
                    self.current = self.take(FrameSlot::Held)?;
                    self.pending_timestamp = Some(timestamp);
                } else if self.current.is_some() {
                    self.pending_timestamp = Some(timestamp);
                } else {
                    // Target precedes the first decodable frame: present that frame.
                    self.current = self.take(FrameSlot::Newest)?;
                    self.pending_timestamp = None;
                }
                return Ok(self.current.clone());
            }
            // A candidate: leave it unconverted and look at the next frame.
            self.pending_timestamp = None;
            newest_is_candidate = true;
        }
        Err(DecodeSourceError::SeekSearchLimit {
            limit: MAX_SEEK_SEARCH_FRAMES,
        })
    }

    /// Converts one native frame slot into a shared validated frame.
    fn take(&mut self, slot: FrameSlot) -> Result<Option<Arc<VideoFrame>>, DecodeSourceError> {
        self.session
            .take_rgba(slot)?
            .map(|frame| {
                VideoFrame::new(
                    PixelFormat::Rgba8,
                    frame.width,
                    frame.height,
                    frame.stride_bytes,
                    frame.timestamp_secs,
                    frame.pixels,
                )
                .map(Arc::new)
                .map_err(Into::into)
            })
            .transpose()
    }
}
