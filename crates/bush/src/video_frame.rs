//! Validated decoded-video output passed from a decoder to Bush's renderer.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

/// The pixel layout currently accepted by Bush's renderer boundary.
///
/// Starting with one owned format avoids implicit per-frame conversions. Additional formats must
/// be introduced with an explicit renderer and benchmark contract.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PixelFormat {
    Rgba8,
}

impl PixelFormat {
    const fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgba8 => 4,
        }
    }
}

static NEXT_FRAME_ID: AtomicU64 = AtomicU64::new(1);

/// An owned decoded video frame safe to retain in Bush's bounded resource cache.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    /// Identifies the pixel content: clones share it, every newly built frame gets a fresh one.
    /// Pixels are immutable, so a renderer can skip re-uploading a frame it already holds.
    id: u64,
    format: PixelFormat,
    width: u32,
    height: u32,
    stride_bytes: usize,
    timestamp_secs: f64,
    pixels: Vec<u8>,
    /// Whether every pixel's alpha is 255, found the first time it is asked.
    opaque: OnceLock<bool>,
}

impl PartialEq for VideoFrame {
    /// Compares content only; the id is an identity, not part of the value.
    fn eq(&self, other: &Self) -> bool {
        self.format == other.format
            && self.width == other.width
            && self.height == other.height
            && self.stride_bytes == other.stride_bytes
            && self.timestamp_secs == other.timestamp_secs
            && self.pixels == other.pixels
    }
}

impl VideoFrame {
    /// Validates the decoder output before it reaches rendering or cache accounting.
    pub fn new(
        format: PixelFormat,
        width: u32,
        height: u32,
        stride_bytes: usize,
        timestamp_secs: f64,
        pixels: Vec<u8>,
    ) -> Result<Self, VideoFrameError> {
        if width == 0 || height == 0 {
            return Err(VideoFrameError::EmptyDimensions { width, height });
        }
        if !timestamp_secs.is_finite() || timestamp_secs < 0.0 {
            return Err(VideoFrameError::InvalidTimestamp(timestamp_secs));
        }

        let row_bytes = (width as usize)
            .checked_mul(format.bytes_per_pixel())
            .ok_or(VideoFrameError::SizeOverflow)?;
        if stride_bytes < row_bytes {
            return Err(VideoFrameError::StrideTooSmall {
                stride_bytes,
                minimum_bytes: row_bytes,
            });
        }
        let required_bytes = stride_bytes
            .checked_mul(height as usize)
            .ok_or(VideoFrameError::SizeOverflow)?;
        if pixels.len() < required_bytes {
            return Err(VideoFrameError::BufferTooShort {
                actual_bytes: pixels.len(),
                required_bytes,
            });
        }

        Ok(Self {
            id: NEXT_FRAME_ID.fetch_add(1, Ordering::Relaxed),
            format,
            width,
            height,
            stride_bytes,
            timestamp_secs,
            pixels,
            opaque: OnceLock::new(),
        })
    }

    /// Whether no pixel is see-through (alpha 255 everywhere). Decoded video always is; the answer
    /// is kept, so asking again costs nothing.
    pub fn is_opaque(&self) -> bool {
        *self.opaque.get_or_init(|| {
            let row_bytes = self.width as usize * self.format.bytes_per_pixel();
            self.pixels
                .chunks(self.stride_bytes)
                .take(self.height as usize)
                .all(|row| {
                    row[..row_bytes]
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .all(|pixel| pixel[3] == 255)
                })
        })
    }

    /// Content identity; equal ids guarantee equal pixels.
    pub const fn id(&self) -> u64 {
        self.id
    }
    pub const fn format(&self) -> PixelFormat {
        self.format
    }
    pub const fn width(&self) -> u32 {
        self.width
    }
    pub const fn height(&self) -> u32 {
        self.height
    }
    pub const fn stride_bytes(&self) -> usize {
        self.stride_bytes
    }
    pub const fn timestamp_secs(&self) -> f64 {
        self.timestamp_secs
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn retained_bytes(&self) -> usize {
        self.pixels.len()
    }

    /// Copies only pixel bytes, excluding any row padding.
    pub(crate) fn tight_pixels(&self) -> Vec<u8> {
        let row_bytes = self.width as usize * self.format.bytes_per_pixel();
        if self.stride_bytes == row_bytes {
            return self.pixels[..row_bytes * self.height as usize].to_vec();
        }

        let mut pixels = Vec::with_capacity(row_bytes * self.height as usize);
        for row in 0..self.height as usize {
            let start = row * self.stride_bytes;
            pixels.extend_from_slice(&self.pixels[start..start + row_bytes]);
        }
        pixels
    }

    /// Creates a frame with this frame's format, dimensions, and timestamp but new storage.
    pub(crate) fn with_pixels(
        &self,
        stride_bytes: usize,
        pixels: Vec<u8>,
    ) -> Result<Self, VideoFrameError> {
        Self::new(
            self.format,
            self.width,
            self.height,
            stride_bytes,
            self.timestamp_secs,
            pixels,
        )
    }

    /// Creates a tightly packed frame with this frame's metadata.
    pub(crate) fn with_tight_pixels(&self, pixels: Vec<u8>) -> Result<Self, VideoFrameError> {
        self.with_pixels(self.width as usize * self.format.bytes_per_pixel(), pixels)
    }
}

/// Decoded-frame data rejected before native or GPU work.
#[derive(Clone, Debug, PartialEq)]
pub enum VideoFrameError {
    EmptyDimensions {
        width: u32,
        height: u32,
    },
    InvalidTimestamp(f64),
    StrideTooSmall {
        stride_bytes: usize,
        minimum_bytes: usize,
    },
    BufferTooShort {
        actual_bytes: usize,
        required_bytes: usize,
    },
    SizeOverflow,
}
