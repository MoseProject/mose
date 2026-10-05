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

use crate::ffi::avbridge_generate_waveform;

/// What [`generate_waveform`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum WaveformError {
    /// `path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    /// `path` has no audio stream to compute a waveform from.
    #[error("input has no audio stream")]
    NoAudioStream,
    /// Couldn't find/open the audio decoder.
    #[error("failed to open the audio decoder")]
    Decoder,
    /// Couldn't build the mono-downmix filter graph.
    #[error("failed to build the mono-downmix filter graph")]
    FilterGraph,
    /// A decode/filter call failed mid-stream (not at setup).
    #[error("decode/filter pipeline failed mid-stream")]
    Pipeline,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown waveform status code: {0}")]
    Unknown(c_int),
}

/// Computes `bucket_count` per-bucket (min, max) amplitude peaks — each in `[-1.0, 1.0]` — of
/// `path`'s audio stream downmixed to mono, for waveform rendering. The full duration is
/// divided into equal-length buckets by sample index, so a bucket with no samples in it (e.g.
/// the duration estimate undershot) comes back as `(0.0, 0.0)` rather than left uninitialized.
pub fn generate_waveform(
    path: &Path,
    bucket_count: usize,
) -> Result<Vec<(f32, f32)>, WaveformError> {
    let c_path =
        CString::new(path.to_string_lossy().as_bytes()).map_err(WaveformError::InvalidPath)?;

    let mut mins = vec![0.0f32; bucket_count];
    let mut maxs = vec![0.0f32; bucket_count];

    // SAFETY: c_path is a valid NUL-terminated C string for the duration of this call.
    // `mins`/`maxs` are valid, writable buffers of `bucket_count` f32s each — the C side never
    // writes past that length. `bridge.c` closes the decoder and filter graph contexts on
    // every exit path.
    let status = unsafe {
        avbridge_generate_waveform(
            c_path.as_ptr(),
            bucket_count as c_int,
            mins.as_mut_ptr(),
            maxs.as_mut_ptr(),
        )
    };

    waveform_status(status)?;

    Ok(mins.into_iter().zip(maxs).collect())
}

/// Maps the status code the C side returned from the waveform call; success carries no data.
pub(crate) fn waveform_status(status: c_int) -> Result<(), WaveformError> {
    match status {
        0 => Ok(()),
        1 => Err(WaveformError::OpenInput),
        2 => Err(WaveformError::StreamInfo),
        3 => Err(WaveformError::NoAudioStream),
        4 => Err(WaveformError::Decoder),
        5 => Err(WaveformError::FilterGraph),
        6 => Err(WaveformError::Pipeline),
        other => Err(WaveformError::Unknown(other)),
    }
}
