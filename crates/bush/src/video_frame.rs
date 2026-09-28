//! Validated decoded-video output passed from a decoder to Bush's renderer.

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

/// An owned decoded video frame safe to retain in Bush's bounded resource cache.
#[derive(Clone, Debug, PartialEq)]
pub struct VideoFrame {
    format: PixelFormat,
    width: u32,
    height: u32,
    stride_bytes: usize,
    timestamp_secs: f64,
    pixels: Vec<u8>,
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
            format,
            width,
            height,
            stride_bytes,
            timestamp_secs,
            pixels,
        })
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
