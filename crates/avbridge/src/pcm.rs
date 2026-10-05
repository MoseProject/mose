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

use crate::ffi::{avbridge_extract_pcm_16k_mono, avbridge_free_pcm_buffer};

/// What [`extract_pcm_16k_mono`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum PcmError {
    /// `path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    /// `path` has no audio stream to decode.
    #[error("input has no audio stream")]
    NoAudioStream,
    /// Couldn't find/open the audio decoder.
    #[error("failed to open the audio decoder")]
    Decoder,
    /// Couldn't build the resample/mono-downmix filter graph.
    #[error("failed to build the resample/mono-downmix filter graph")]
    FilterGraph,
    /// A decode/filter call failed mid-stream (not at setup).
    #[error("decode/filter pipeline failed mid-stream")]
    Pipeline,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown pcm status code: {0}")]
    Unknown(c_int),
}

/// Decodes `path`'s first audio stream to 16kHz mono 32-bit float PCM samples — the exact
/// input format `whisper-rs`/whisper.cpp requires for transcription (see [`crate::transcribe`]
/// in `core`, the only caller). Fails with [`PcmError::NoAudioStream`] if `path` has no audio
/// stream.
pub fn extract_pcm_16k_mono(path: &Path) -> Result<Vec<f32>, PcmError> {
    let c_path = CString::new(path.to_string_lossy().as_bytes()).map_err(PcmError::InvalidPath)?;

    let mut out_samples: *mut f32 = std::ptr::null_mut();
    let mut out_sample_count: i64 = 0;

    // SAFETY: c_path is a valid NUL-terminated C string for the duration of this call.
    // out_samples/out_sample_count are valid, writable locations for the C side to fill on
    // success — bridge.c leaves them untouched on any error status, matching the null/0 they're
    // initialized to here. The malloc'd buffer the C side may allocate is copied into a Vec and
    // freed via avbridge_free_pcm_buffer before returning, so no allocation crosses the FFI
    // boundary uncopied.
    let status = unsafe {
        avbridge_extract_pcm_16k_mono(c_path.as_ptr(), &mut out_samples, &mut out_sample_count)
    };

    extract_pcm_16k_mono_status(status)?;
    // SAFETY: on OK, out_samples points to a malloc'd buffer of out_sample_count valid
    // f32s (or is null with out_sample_count == 0 for a zero-length result) — copied
    // into a Vec, then the C-owned buffer is freed via the paired allocator.
    let samples = unsafe { copy_samples(out_samples, out_sample_count as usize) };
    unsafe { avbridge_free_pcm_buffer(out_samples) };
    Ok(samples)
}

/// Maps the status code the C side returned from `extract_pcm_16k_mono`; success carries no data.
pub(crate) fn extract_pcm_16k_mono_status(status: c_int) -> Result<(), PcmError> {
    match status {
        0 => Ok(()),
        1 => Err(PcmError::OpenInput),
        2 => Err(PcmError::StreamInfo),
        3 => Err(PcmError::NoAudioStream),
        4 => Err(PcmError::Decoder),
        5 => Err(PcmError::FilterGraph),
        6 => Err(PcmError::Pipeline),
        other => Err(PcmError::Unknown(other)),
    }
}

/// Copies `count` samples out of a native buffer; a null or empty buffer is an empty result.
///
/// # Safety
/// When `count > 0`, `samples` must point to `count` initialized `f32`s.
pub(crate) unsafe fn copy_samples(samples: *const f32, count: usize) -> Vec<f32> {
    if count > 0 {
        unsafe { std::slice::from_raw_parts(samples, count) }.to_vec()
    } else {
        Vec::new()
    }
}
