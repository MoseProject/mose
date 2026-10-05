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

use crate::ffi::avbridge_encode_matte_video;

/// What [`encode_matte_video`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum MatteError {
    /// `out_path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to allocate output context")]
    AllocOutput,
    /// Couldn't find/open a supported CPU H.264 encoder.
    #[error("failed to open an encoder")]
    Encoder,
    #[error("failed to create an output stream")]
    NewStream,
    #[error("failed to open output for writing")]
    OpenOutput,
    #[error("failed to write output header")]
    WriteHeader,
    /// An encode/write call failed mid-stream (not at setup).
    #[error("encode/write pipeline failed mid-stream")]
    Pipeline,
    /// `frames` was empty, or `width`/`height`/`fps_num`/`fps_den` was zero.
    #[error("no frames, or zero width/height/fps supplied")]
    Empty,
    /// `width` or `height` is odd — invalid for yuv420p's 2x-subsampled chroma planes.
    #[error("width and height must both be even")]
    OddDimensions,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown matte-encode status code: {0}")]
    Unknown(c_int),
}

/// Encodes `frames` — one grayscale-as-luma alpha matte per source frame (see
/// `avcore::background_removal::segment_person`), each exactly `width * height` bytes
/// (row-major, one byte per pixel) — into a plain H.264 video at `out_path`, via
/// the available CPU H.264 encoder. See [`avbridge_encode_matte_video`]'s doc comment in `bridge.h` for the
/// grayscale-as-luma/neutral-chroma encoding this produces. Fails with
/// [`MatteError::Empty`] if `frames` is empty, and [`MatteError::OddDimensions`] if `width`
/// or `height` is odd.
///
/// Every entry in `frames` must be exactly `(width * height) as usize` bytes — this isn't
/// checked here (a length mismatch would misalign every later frame against the wrong byte
/// range without corrupting memory, since the C side reads exactly `frame_count * width *
/// height` bytes total from one contiguous buffer built by concatenating `frames` below); the
/// caller is expected to always supply frames from `avcore::background_removal::segment_person`
/// against the same decoded frame's own `width`/`height`, which already guarantees this.
pub fn encode_matte_video(
    frames: &[Vec<u8>],
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    out_path: &Path,
) -> Result<(), MatteError> {
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(MatteError::InvalidPath)?;

    let mut luma = Vec::with_capacity(frames.len() * (width as usize) * (height as usize));
    for frame in frames {
        luma.extend_from_slice(frame);
    }

    // SAFETY: c_out is a valid NUL-terminated C string for the duration of this call. `luma`
    // is a contiguous buffer of exactly `frames.len() * width * height` bytes, read-only for
    // the C side, valid for the duration of this call. `matte_encode.c` frees the encoder
    // context, output I/O, and output context on every exit path.
    let status = unsafe {
        avbridge_encode_matte_video(
            luma.as_ptr(),
            frames.len() as c_int,
            width as c_int,
            height as c_int,
            fps_num as c_int,
            fps_den as c_int,
            c_out.as_ptr(),
        )
    };

    encode_matte_video_status(status)
}

/// Maps the status code the C side returned from `encode_matte_video` to its result.
pub(crate) fn encode_matte_video_status(status: c_int) -> Result<(), MatteError> {
    match status {
        0 => Ok(()),
        1 => Err(MatteError::AllocOutput),
        2 => Err(MatteError::Encoder),
        3 => Err(MatteError::NewStream),
        4 => Err(MatteError::OpenOutput),
        5 => Err(MatteError::WriteHeader),
        6 => Err(MatteError::Pipeline),
        7 => Err(MatteError::Empty),
        8 => Err(MatteError::OddDimensions),
        other => Err(MatteError::Unknown(other)),
    }
}
