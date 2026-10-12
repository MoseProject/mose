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

use crate::ffi::{avbridge_remux_audio_stream, avbridge_remux_copy};

/// What [`remux_copy`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum RemuxError {
    /// `in_path`/`out_path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    /// Couldn't open `in_path`.
    #[error("failed to open input")]
    OpenInput,
    /// Couldn't read `in_path`'s streams.
    #[error("failed to read stream info")]
    StreamInfo,
    /// Couldn't guess an output format for `out_path` (unrecognized extension).
    #[error("failed to allocate output context")]
    AllocOutput,
    /// Couldn't create an output stream matching one of the input's streams.
    #[error("failed to create an output stream")]
    NewStream,
    /// Couldn't open `out_path` for writing.
    #[error("failed to open output for writing")]
    OpenOutput,
    /// Failed writing the output container's header.
    #[error("failed to write output header")]
    WriteHeader,
    /// Failed partway through writing packets — `out_path` may be a truncated/invalid file.
    #[error("failed to write a frame")]
    WriteFrame,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown remux status code: {0}")]
    Unknown(c_int),
}

/// Demuxes `in_path` and remuxes every video/audio stream to `out_path` unchanged — no decode,
/// no encode, no filtering. Equivalent to `ffmpeg -i in_path -c copy out_path`.
pub fn remux_copy(in_path: &Path, out_path: &Path) -> Result<(), RemuxError> {
    let c_in =
        CString::new(in_path.to_string_lossy().as_bytes()).map_err(RemuxError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(RemuxError::InvalidPath)?;

    // SAFETY: c_in/c_out are valid NUL-terminated C strings for the duration of this call.
    // `bridge.c` closes the input context, output I/O, and frees the output context on every
    // exit path.
    let status = unsafe { avbridge_remux_copy(c_in.as_ptr(), c_out.as_ptr()) };

    remux_copy_status(status)
}

/// Copies the single audio stream `stream_index` (the container's index, as [`crate::probe`]
/// lists it) of `in_path` to `out_path` unchanged, as a standalone audio file. Use a `.mka`
/// extension: Matroska holds any codec. Equivalent to `ffmpeg -i in -map 0:N -c copy out.mka`.
pub fn extract_audio_stream(
    in_path: &Path,
    out_path: &Path,
    stream_index: u32,
) -> Result<(), RemuxError> {
    let c_in =
        CString::new(in_path.to_string_lossy().as_bytes()).map_err(RemuxError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(RemuxError::InvalidPath)?;
    let index = c_int::try_from(stream_index).map_err(|_| RemuxError::NewStream)?;

    // SAFETY: as for `remux_copy`.
    let status = unsafe { avbridge_remux_audio_stream(c_in.as_ptr(), c_out.as_ptr(), index) };

    remux_copy_status(status)
}

/// Maps the status code the C side returned from `remux_copy` to its result.
pub(crate) fn remux_copy_status(status: c_int) -> Result<(), RemuxError> {
    match status {
        0 => Ok(()),
        1 => Err(RemuxError::OpenInput),
        2 => Err(RemuxError::StreamInfo),
        3 => Err(RemuxError::AllocOutput),
        4 => Err(RemuxError::NewStream),
        5 => Err(RemuxError::OpenOutput),
        6 => Err(RemuxError::WriteHeader),
        7 => Err(RemuxError::WriteFrame),
        other => Err(RemuxError::Unknown(other)),
    }
}
