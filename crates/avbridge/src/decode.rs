// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

use std::ffi::{CString, NulError};
use std::os::raw::c_int;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::ffi::{
    avbridge_decode_advance, avbridge_decode_close, avbridge_decode_free_frame,
    avbridge_decode_has_held, avbridge_decode_next_rgba, avbridge_decode_open_ex,
    avbridge_decode_seek, avbridge_decode_take_rgba, avbridge_hw_device_cache_clear, RawDecodeInfo,
    RawDecodeSession, RawDecodedRgbaFrame,
};

/// Stream metadata for one opened video decode session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DecodeInfo {
    pub width: u32,
    pub height: u32,
    pub fps: Option<f64>,
    pub duration_secs: Option<f64>,
    /// Whether frames are decoded on a hardware device (always `false` under
    /// [`DecodePolicy::Software`], or when the hardware path was unavailable).
    pub hardware: bool,
}

/// Whether to try hardware decode. Hardware is opt-in because it is only a win when measured:
/// transfers back to system memory can make it slower than software on small or low-bitrate
/// streams.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DecodePolicy {
    /// Always decode in software.
    #[default]
    Software,
    /// Try the first hardware device the codec supports; any failure falls back to software.
    HardwareWithFallback,
}

/// Releases the process-wide hardware decode devices kept alive between sessions (creating one
/// costs hundreds of milliseconds, so [`DecodePolicy::HardwareWithFallback`] shares them).
/// Sessions still open keep their own reference; call this after closing them to free the device.
pub fn release_cached_hardware_devices() {
    // SAFETY: the C side guards its table with a lock and only unreferences buffers it owns.
    unsafe { avbridge_hw_device_cache_clear() };
}

/// One packed RGBA frame copied out of the native decode session.
#[derive(Clone, Debug, PartialEq)]
pub struct DecodedRgbaFrame {
    pub width: u32,
    pub height: u32,
    pub stride_bytes: usize,
    pub timestamp_secs: f64,
    pub pixels: Vec<u8>,
}

/// A worker-owned FFmpeg video decode session.
///
/// This type deliberately has no `Sync` implementation. Open, seek, decode, and teardown must
/// stay on one Bush worker thread; cancellation may be signalled from another thread through the
/// supplied [`AtomicBool`].
#[derive(Debug)]
pub struct DecodeSession {
    raw: *mut RawDecodeSession,
}

// SAFETY: the native session has no thread affinity; it may move between threads as long as one
// thread uses it at a time, which `&mut self` on every operation enforces.
unsafe impl Send for DecodeSession {}

/// Failure reported by the narrow decode-session C surface.
#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    #[error("input has no video stream")]
    NoVideoStream,
    #[error("failed to initialize video decoder")]
    Decoder,
    #[error("seek timestamp must be finite and non-negative")]
    InvalidSeekTimestamp,
    #[error("failed to seek video stream")]
    Seek,
    #[error("decoded frame dimensions exceed the native RGBA allocation limit")]
    FrameSize,
    #[error("decode pipeline failed")]
    Pipeline,
    #[error("unknown decode status code: {0}")]
    Unknown(c_int),
}

impl DecodeSession {
    /// Opens the first video stream at `path` with software decode.
    pub fn open(path: &Path) -> Result<(Self, DecodeInfo), DecodeError> {
        Self::open_with_policy(path, DecodePolicy::Software)
    }

    /// Opens the first video stream at `path` under `policy`. `DecodeInfo::hardware` reports
    /// which path was actually taken.
    pub fn open_with_policy(
        path: &Path,
        policy: DecodePolicy,
    ) -> Result<(Self, DecodeInfo), DecodeError> {
        let c_path =
            CString::new(path.to_string_lossy().as_bytes()).map_err(DecodeError::InvalidPath)?;
        let mut raw = std::ptr::null_mut();
        let mut info = RawDecodeInfo {
            width: 0,
            height: 0,
            fps_num: 0,
            fps_den: 0,
            duration_secs: 0.0,
            hardware: 0,
        };
        let hardware = c_int::from(policy == DecodePolicy::HardwareWithFallback);
        // SAFETY: c_path is NUL-terminated for this call; raw/info point to writable storage.
        let status =
            unsafe { avbridge_decode_open_ex(c_path.as_ptr(), hardware, &mut raw, &mut info) };
        decode_open_status(status)?;
        debug_assert!(!raw.is_null());
        // From here the session owns `raw`, so every early return below closes it via `Drop`.
        let session = Self { raw };
        let (width, height) = decode_dimensions(info.width, info.height)?;
        Ok((
            session,
            DecodeInfo {
                width,
                height,
                fps: (info.fps_num > 0 && info.fps_den > 0)
                    .then_some(info.fps_num as f64 / info.fps_den as f64),
                duration_secs: (info.duration_secs.is_finite() && info.duration_secs >= 0.0)
                    .then_some(info.duration_secs),
                hardware: info.hardware != 0,
            },
        ))
    }

    /// Flushes decoder state and seeks to the nearest preceding keyframe.
    pub fn seek(&mut self, timestamp_secs: f64) -> Result<(), DecodeError> {
        if !timestamp_secs.is_finite() || timestamp_secs < 0.0 {
            return Err(DecodeError::InvalidSeekTimestamp);
        }
        // SAFETY: self.raw is owned by this live session and C does not retain the call inputs.
        decode_seek_status(unsafe { avbridge_decode_seek(self.raw, timestamp_secs) })
    }

    /// Decodes the next packed RGBA frame, or `None` at end of stream.
    pub fn next_rgba(
        &mut self,
        cancel: &AtomicBool,
    ) -> Result<Option<DecodedRgbaFrame>, DecodeError> {
        let mut raw_frame = RawDecodedRgbaFrame {
            pixels: std::ptr::null_mut(),
            pixel_len: 0,
            width: 0,
            height: 0,
            stride_bytes: 0,
            timestamp_secs: 0.0,
        };
        // SAFETY: self.raw is live, `cancel.as_ptr()` is documented to have bool-compatible
        // size/alignment/bit validity, and raw_frame is writable for this call. Successful
        // frames are copied and released with the paired C allocator below.
        let status = unsafe {
            avbridge_decode_next_rgba(self.raw, cancel.as_ptr() as *const u8, &mut raw_frame)
        };
        if next_frame_status(status)? {
            copy_out_frame(&mut raw_frame).map(Some)
        } else {
            Ok(None)
        }
    }

    /// Decodes the next frame **without** converting it to RGBA and returns its timestamp.
    ///
    /// The previously advanced-to frame is kept as the held frame. This is the cheap way to seek
    /// to an exact frame: advance until the timestamp passes the target, then convert only the
    /// frame you present with [`Self::take_rgba`]. A frame left unconverted is returned by the
    /// next [`Self::next_rgba`].
    pub fn advance(&mut self, cancel: &AtomicBool) -> Result<Advance, DecodeError> {
        let mut timestamp_secs = 0.0_f64;
        // SAFETY: self.raw is live, `cancel.as_ptr()` has bool-compatible layout, and
        // timestamp_secs is writable for this call.
        let status = unsafe {
            avbridge_decode_advance(self.raw, cancel.as_ptr() as *const u8, &mut timestamp_secs)
        };
        decode_advance_status(status, timestamp_secs)
    }

    /// Whether [`Self::advance`] has kept a frame before the newest one.
    pub fn has_held_frame(&self) -> bool {
        // SAFETY: self.raw is a live session.
        unsafe { avbridge_decode_has_held(self.raw) != 0 }
    }

    /// Converts and removes an advanced-to frame: the newest one, or the one held before it.
    /// `Ok(None)` if that slot is empty.
    pub fn take_rgba(&mut self, slot: FrameSlot) -> Result<Option<DecodedRgbaFrame>, DecodeError> {
        let mut raw_frame = RawDecodedRgbaFrame {
            pixels: std::ptr::null_mut(),
            pixel_len: 0,
            width: 0,
            height: 0,
            stride_bytes: 0,
            timestamp_secs: 0.0,
        };
        // SAFETY: self.raw is live and raw_frame is writable; a successful frame is copied and
        // released with the paired C allocator in `copy_out_frame`.
        let status = unsafe {
            avbridge_decode_take_rgba(
                self.raw,
                c_int::from(slot == FrameSlot::Newest),
                &mut raw_frame,
            )
        };
        if take_frame_status(status)? {
            copy_out_frame(&mut raw_frame).map(Some)
        } else {
            Ok(None)
        }
    }
}

/// Outcome of [`DecodeSession::advance`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Advance {
    /// A frame was decoded (not converted); its presentation time in seconds.
    Frame(f64),
    EndOfStream,
    Cancelled,
}

/// Which advanced-to frame [`DecodeSession::take_rgba`] converts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameSlot {
    /// The frame most recently advanced to.
    Newest,
    /// The frame advanced to before the newest one.
    Held,
}

/// Copies a successful native frame into owned memory and releases the native allocation.
pub(crate) fn copy_out_frame(
    raw_frame: &mut RawDecodedRgbaFrame,
) -> Result<DecodedRgbaFrame, DecodeError> {
    let width = u32::try_from(raw_frame.width).map_err(|_| DecodeError::FrameSize)?;
    let height = u32::try_from(raw_frame.height).map_err(|_| DecodeError::FrameSize)?;
    let stride_bytes =
        usize::try_from(raw_frame.stride_bytes).map_err(|_| DecodeError::FrameSize)?;
    if raw_frame.pixels.is_null()
        || width == 0
        || height == 0
        || stride_bytes == 0
        || !raw_frame.timestamp_secs.is_finite()
    {
        // SAFETY: safe with null or a successful C allocation; clears raw_frame.
        unsafe { avbridge_decode_free_frame(raw_frame) };
        return Err(DecodeError::FrameSize);
    }
    // SAFETY: DECODE_OK guarantees `pixel_len` initialized bytes at `pixels`.
    let pixels =
        unsafe { std::slice::from_raw_parts(raw_frame.pixels, raw_frame.pixel_len).to_vec() };
    let timestamp_secs = raw_frame.timestamp_secs;
    // SAFETY: releases only the C allocation copied above.
    unsafe { avbridge_decode_free_frame(raw_frame) };
    Ok(DecodedRgbaFrame {
        width,
        height,
        stride_bytes,
        timestamp_secs,
        pixels,
    })
}

impl Drop for DecodeSession {
    fn drop(&mut self) {
        // SAFETY: self uniquely owns the non-null raw session returned by avbridge_decode_open.
        unsafe { avbridge_decode_close(self.raw) };
    }
}

pub(crate) fn decode_open_status(status: c_int) -> Result<(), DecodeError> {
    match status {
        0 => Ok(()),
        3 => Err(DecodeError::OpenInput),
        4 => Err(DecodeError::StreamInfo),
        5 => Err(DecodeError::NoVideoStream),
        6 => Err(DecodeError::Decoder),
        9 => Err(DecodeError::FrameSize),
        other => Err(DecodeError::Unknown(other)),
    }
}

/// Maps the status of a native seek.
pub(crate) fn decode_seek_status(status: c_int) -> Result<(), DecodeError> {
    match status {
        0 => Ok(()),
        7 => Err(DecodeError::Seek),
        other => Err(DecodeError::Unknown(other)),
    }
}

/// The error for a frame-conversion status other than success, end of stream or cancellation.
pub(crate) fn decode_frame_failure(status: c_int) -> DecodeError {
    match status {
        8 => DecodeError::Pipeline,
        9 => DecodeError::FrameSize,
        other => DecodeError::Unknown(other),
    }
}

/// Maps the status of a native advance, with the timestamp the C side wrote on success.
pub(crate) fn decode_advance_status(
    status: c_int,
    timestamp_secs: f64,
) -> Result<Advance, DecodeError> {
    match status {
        0 if timestamp_secs.is_finite() => Ok(Advance::Frame(timestamp_secs)),
        0 => Err(DecodeError::Pipeline),
        1 => Ok(Advance::EndOfStream),
        2 => Ok(Advance::Cancelled),
        8 => Err(DecodeError::Pipeline),
        other => Err(DecodeError::Unknown(other)),
    }
}

/// Validates the native frame size reported when a decode session opens.
pub(crate) fn decode_dimensions(width: c_int, height: c_int) -> Result<(u32, u32), DecodeError> {
    let width = u32::try_from(width).map_err(|_| DecodeError::FrameSize)?;
    let height = u32::try_from(height).map_err(|_| DecodeError::FrameSize)?;
    if width == 0 || height == 0 {
        return Err(DecodeError::FrameSize);
    }
    Ok((width, height))
}

/// Status of a native `next_rgba`: `true` when a frame is ready to copy out, `false` at the end
/// of the stream or on cancellation.
pub(crate) fn next_frame_status(status: c_int) -> Result<bool, DecodeError> {
    match status {
        0 => Ok(true),
        1 | 2 => Ok(false),
        other => Err(decode_frame_failure(other)),
    }
}

/// Status of a native `take_rgba`: `true` when a frame is ready, `false` when the slot is empty.
pub(crate) fn take_frame_status(status: c_int) -> Result<bool, DecodeError> {
    match status {
        0 => Ok(true),
        1 => Ok(false),
        other => Err(decode_frame_failure(other)),
    }
}
