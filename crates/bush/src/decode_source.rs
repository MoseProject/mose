//! Bush's standalone adapter over avbridge's worker-owned FFmpeg decode session.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use avbridge::{DecodeError, DecodeSession};

use crate::{PixelFormat, VideoFrame, VideoFrameError};

/// Stable metadata for an opened preview source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SourceInfo {
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub duration_secs: Option<f64>,
}

/// A one-video-source decoder owned by one Bush worker.
#[derive(Debug)]
pub struct DecodeSource {
    session: DecodeSession,
    info: SourceInfo,
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

/// Upper bound on frames decoded forward from a key unit to reach an exact target. Bounds
/// worst-case latency on hostile or very long-GOP media instead of decoding to EOF.
pub const MAX_SEEK_SEARCH_FRAMES: u32 = 600;

impl DecodeSource {
    /// Opens one local video source. No project, UI, or timeline state is retained.
    pub fn open(path: &Path) -> Result<Self, DecodeSourceError> {
        let (session, info) = DecodeSession::open(path)?;
        Ok(Self {
            session,
            info: SourceInfo {
                width: info.width,
                height: info.height,
                fps: info.fps,
                duration_secs: info.duration_secs,
            },
        })
    }

    pub const fn info(&self) -> SourceInfo {
        self.info
    }

    /// Seeks to the preceding keyframe; caller then polls until it reaches the desired frame.
    pub fn seek(&mut self, timestamp_secs: f64) -> Result<(), DecodeSourceError> {
        self.session.seek(timestamp_secs)?;
        Ok(())
    }

    /// Decodes one frame in Bush's fixed RGBA contract, or returns `None` for EOF/cancellation.
    pub fn next_frame(
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

    /// Frame-accurate seek: the frame presented at `target_secs`, i.e. the last decoded frame
    /// with `timestamp <= target`. Seeks to the preceding key unit, then decodes forward,
    /// discarding intermediate frames. If the first frame is already past the target (a
    /// target before the stream start) it is returned. `Ok(None)` means cancelled, or the
    /// stream ended before producing any frame.
    pub fn frame_at(
        &mut self,
        target_secs: f64,
        cancel: &AtomicBool,
    ) -> Result<Option<VideoFrame>, DecodeSourceError> {
        self.seek(target_secs)?;
        let mut held: Option<VideoFrame> = None;
        for _ in 0..MAX_SEEK_SEARCH_FRAMES {
            let Some(frame) = self.next_frame(cancel)? else {
                return Ok(if cancel.load(std::sync::atomic::Ordering::Acquire) {
                    None
                } else {
                    held
                });
            };
            if frame.timestamp_secs() > target_secs + 1e-6 {
                return Ok(Some(held.unwrap_or(frame)));
            }
            held = Some(frame);
        }
        Err(DecodeSourceError::SeekSearchLimit {
            limit: MAX_SEEK_SEARCH_FRAMES,
        })
    }
}
