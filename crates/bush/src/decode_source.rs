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
}
