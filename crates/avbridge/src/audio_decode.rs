//! Safe wrapper over the worker-owned FFmpeg audio decode session (`audio_decode_session.c`).
//!
//! Output is always packed 32-bit float at the requested sample rate and channel count; raw
//! `AV*` values never cross this boundary.

use std::ffi::{c_char, c_int, CString, NulError};
use std::path::Path;
use std::sync::atomic::AtomicBool;

#[repr(C)]
struct RawAudioInfo {
    sample_rate: c_int,
    channels: c_int,
    source_sample_rate: c_int,
    source_channels: c_int,
    duration_secs: f64,
}

#[repr(C)]
struct RawAudioBlock {
    samples: *mut f32,
    frame_count: i64,
    timestamp_secs: f64,
}

#[repr(C)]
struct RawAudioSession {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn avbridge_audio_open(
        path: *const c_char,
        target_sample_rate: c_int,
        target_channels: c_int,
        out_session: *mut *mut RawAudioSession,
        out_info: *mut RawAudioInfo,
    ) -> c_int;
    fn avbridge_audio_seek(session: *mut RawAudioSession, timestamp_secs: f64) -> c_int;
    fn avbridge_audio_next_block(
        session: *mut RawAudioSession,
        cancel: *const u8,
        out_block: *mut RawAudioBlock,
    ) -> c_int;
    fn avbridge_audio_free_block(block: *mut RawAudioBlock);
    fn avbridge_audio_close(session: *mut RawAudioSession);
}

/// Stream metadata for one opened audio decode session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioDecodeInfo {
    /// Output sample rate (the requested one).
    pub sample_rate: u32,
    /// Output channel count (the requested one).
    pub channels: u16,
    pub source_sample_rate: u32,
    pub source_channels: u16,
    pub duration_secs: Option<f64>,
}

/// One interleaved block copied out of the native session.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudioBlock {
    /// Interleaved samples, `frame_count * channels` long.
    pub samples: Vec<f32>,
    pub frame_count: usize,
    /// Presentation time of the first frame; may be slightly negative (codec priming).
    pub timestamp_secs: f64,
}

/// A worker-owned FFmpeg audio decode session. Not `Sync`: open, seek, decode and teardown must
/// stay on one thread at a time.
#[derive(Debug)]
pub struct AudioDecodeSession {
    raw: *mut RawAudioSession,
    channels: usize,
}

// SAFETY: the native session has no thread affinity; it may move between threads as long as one
// thread uses it at a time, which `&mut self` on every operation enforces.
unsafe impl Send for AudioDecodeSession {}

#[derive(Debug, thiserror::Error)]
pub enum AudioDecodeError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("unsupported output format: {sample_rate} Hz, {channels} channel(s)")]
    UnsupportedFormat { sample_rate: u32, channels: u16 },
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    #[error("input has no audio stream")]
    NoAudioStream,
    #[error("failed to initialize audio decoder")]
    Decoder,
    #[error("seek timestamp must be finite and non-negative")]
    InvalidSeekTimestamp,
    #[error("failed to seek audio stream")]
    Seek,
    #[error("decoded audio block exceeds the native allocation limit")]
    BlockSize,
    #[error("audio decode pipeline failed")]
    Pipeline,
    #[error("unknown audio decode status code: {0}")]
    Unknown(c_int),
}

impl AudioDecodeSession {
    /// Opens the best audio stream of `path`, resampling and remixing to
    /// `sample_rate`/`channels` (1 or 2) packed float.
    pub fn open(
        path: &Path,
        sample_rate: u32,
        channels: u16,
    ) -> Result<(Self, AudioDecodeInfo), AudioDecodeError> {
        if !(8_000..=192_000).contains(&sample_rate) || !(1..=2).contains(&channels) {
            return Err(AudioDecodeError::UnsupportedFormat {
                sample_rate,
                channels,
            });
        }
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(AudioDecodeError::InvalidPath)?;
        let mut raw = std::ptr::null_mut();
        let mut info = RawAudioInfo {
            sample_rate: 0,
            channels: 0,
            source_sample_rate: 0,
            source_channels: 0,
            duration_secs: 0.0,
        };
        // SAFETY: c_path is NUL-terminated for this call; raw/info point to writable storage.
        let status = unsafe {
            avbridge_audio_open(
                c_path.as_ptr(),
                sample_rate as c_int,
                c_int::from(channels),
                &mut raw,
                &mut info,
            )
        };
        open_status(status)?;
        debug_assert!(!raw.is_null());
        let session = Self {
            raw,
            channels: usize::from(channels),
        };
        let info = AudioDecodeInfo {
            sample_rate,
            channels,
            source_sample_rate: u32::try_from(info.source_sample_rate).unwrap_or(0),
            source_channels: u16::try_from(info.source_channels).unwrap_or(0),
            duration_secs: (info.duration_secs.is_finite() && info.duration_secs > 0.0)
                .then_some(info.duration_secs),
        };
        Ok((session, info))
    }

    /// Seeks to the packet at or before `timestamp_secs` and resets resampler state; following
    /// blocks may start before the target.
    pub fn seek(&mut self, timestamp_secs: f64) -> Result<(), AudioDecodeError> {
        if !timestamp_secs.is_finite() || timestamp_secs < 0.0 {
            return Err(AudioDecodeError::InvalidSeekTimestamp);
        }
        // SAFETY: self.raw is owned by this live session and C does not retain the inputs.
        match unsafe { avbridge_audio_seek(self.raw, timestamp_secs) } {
            0 => Ok(()),
            7 => Err(AudioDecodeError::Seek),
            8 => Err(AudioDecodeError::Pipeline),
            other => Err(AudioDecodeError::Unknown(other)),
        }
    }

    /// Next block in stream order. `Ok(None)` means cancelled or end of stream.
    pub fn next_block(
        &mut self,
        cancel: &AtomicBool,
    ) -> Result<Option<DecodedAudioBlock>, AudioDecodeError> {
        let mut raw_block = RawAudioBlock {
            samples: std::ptr::null_mut(),
            frame_count: 0,
            timestamp_secs: 0.0,
        };
        // SAFETY: self.raw is live, `cancel.as_ptr()` has bool-compatible layout, and raw_block
        // is writable. Successful blocks are copied and released with the paired C allocator.
        let status = unsafe {
            avbridge_audio_next_block(self.raw, cancel.as_ptr() as *const u8, &mut raw_block)
        };
        match status {
            0 => {
                let frames = usize::try_from(raw_block.frame_count).unwrap_or(0);
                let sample_count = frames.checked_mul(self.channels);
                let valid = !raw_block.samples.is_null()
                    && frames > 0
                    && sample_count.is_some()
                    && raw_block.timestamp_secs.is_finite();
                if !valid {
                    // SAFETY: safe with null or a successful C allocation; clears raw_block.
                    unsafe { avbridge_audio_free_block(&mut raw_block) };
                    return Err(AudioDecodeError::BlockSize);
                }
                // SAFETY: DECODE_OK guarantees `frame_count * channels` initialized floats.
                let samples = unsafe {
                    std::slice::from_raw_parts(raw_block.samples, sample_count.unwrap_or(0))
                        .to_vec()
                };
                let timestamp_secs = raw_block.timestamp_secs;
                // SAFETY: releases only the C allocation copied above.
                unsafe { avbridge_audio_free_block(&mut raw_block) };
                Ok(Some(DecodedAudioBlock {
                    samples,
                    frame_count: frames,
                    timestamp_secs,
                }))
            }
            1 | 2 => Ok(None),
            8 => Err(AudioDecodeError::Pipeline),
            9 => Err(AudioDecodeError::BlockSize),
            other => Err(AudioDecodeError::Unknown(other)),
        }
    }
}

impl Drop for AudioDecodeSession {
    fn drop(&mut self) {
        // SAFETY: self uniquely owns the non-null raw session returned by avbridge_audio_open.
        unsafe { avbridge_audio_close(self.raw) };
    }
}

fn open_status(status: c_int) -> Result<(), AudioDecodeError> {
    match status {
        0 => Ok(()),
        3 => Err(AudioDecodeError::OpenInput),
        4 => Err(AudioDecodeError::StreamInfo),
        6 => Err(AudioDecodeError::Decoder),
        8 => Err(AudioDecodeError::Pipeline),
        10 => Err(AudioDecodeError::NoAudioStream),
        other => Err(AudioDecodeError::Unknown(other)),
    }
}
