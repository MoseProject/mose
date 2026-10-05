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

use std::ffi::{c_void, CString, NulError};
use std::os::raw::c_int;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::ffi::avbridge_encode_export;
use crate::ffi::progress_trampoline;

/// What [`encode_export`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum EncodeError {
    /// `in_path`/`out_path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    #[error("failed to allocate output context")]
    AllocOutput,
    #[error("failed to create an output stream")]
    NewStream,
    #[error("failed to open output for writing")]
    OpenOutput,
    #[error("failed to write output header")]
    WriteHeader,
    #[error("failed to write a frame")]
    WriteFrame,
    /// `in_path` has no audio stream to normalize.
    #[error("input has no audio stream")]
    NoAudioStream,
    /// Couldn't find/open the audio decoder.
    #[error("failed to open the audio decoder")]
    Decoder,
    /// Couldn't build the loudnorm/limiter filter graph.
    #[error("failed to build the audio filter graph")]
    FilterGraph,
    /// Couldn't find/open the AAC encoder.
    #[error("failed to open the AAC encoder")]
    Encoder,
    /// A decode/filter/encode call failed mid-stream (not at setup).
    #[error("decode/filter/encode pipeline failed mid-stream")]
    Pipeline,
    /// (`encode_timeline_export` only) a segment has no video stream.
    #[error("a segment has no video stream")]
    NoVideoStream,
    /// (`encode_timeline_export` only) a later segment's audio format (sample rate/format/
    /// channel layout) doesn't match the first segment's.
    #[error("a segment's audio format doesn't match the first segment's")]
    AudioFormatMismatch,
    /// (`encode_timeline_export` only) `segments` was empty.
    #[error("no segments to render")]
    EmptyTimeline,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown encode status code: {0}")]
    Unknown(c_int),
}

/// How [`encode_export`] ended: all the way through, or stopped early because `cancel` was
/// set. Mirrors `avcore::render::RenderOutcome`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeOutcome {
    Completed,
    Cancelled,
}

/// Renders `in_path` to `out_path`: video passthrough-copied, audio decoded, normalized
/// (`loudnorm` to `target_lufs` + a true-peak safety limiter) and re-encoded to AAC 192kbps.
/// Fails with [`EncodeError::NoAudioStream`] if `in_path` has no audio stream.
///
/// Calls `on_progress(seconds_processed)` as packets are read, and checks `cancel` between
/// them — if another thread sets it, encoding stops (without writing a trailer, so `out_path`
/// is left truncated/invalid) and this returns `Ok(EncodeOutcome::Cancelled)` rather than
/// treating the cancellation as a failure.
pub fn encode_export<F: FnMut(f64)>(
    in_path: &Path,
    out_path: &Path,
    target_lufs: f32,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> Result<EncodeOutcome, EncodeError> {
    let c_in =
        CString::new(in_path.to_string_lossy().as_bytes()).map_err(EncodeError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(EncodeError::InvalidPath)?;

    // SAFETY: c_in/c_out are valid NUL-terminated C strings for the duration of this call.
    // `bridge.c` frees the decoder/encoder/filter-graph contexts, output I/O, and output
    // context on every exit path. `progress_trampoline::<F>` matches `on_progress`'s captured
    // type F, and `&mut on_progress` outlives the call (it's this function's own stack frame).
    // `cancel.as_ptr()` is documented to have the same size/alignment/bit-validity as `bool`
    // (0/1), which the C side reads as `uint8_t`.
    let status = unsafe {
        avbridge_encode_export(
            c_in.as_ptr(),
            c_out.as_ptr(),
            target_lufs,
            Some(progress_trampoline::<F>),
            &mut on_progress as *mut _ as *mut c_void,
            cancel.as_ptr() as *const u8,
        )
    };

    encode_export_status(status)
}

/// Maps the status code the C side returned from `encode_export` to its result.
pub(crate) fn encode_export_status(status: c_int) -> Result<EncodeOutcome, EncodeError> {
    match status {
        0 => Ok(EncodeOutcome::Completed),
        1 => Err(EncodeError::OpenInput),
        2 => Err(EncodeError::StreamInfo),
        3 => Err(EncodeError::AllocOutput),
        4 => Err(EncodeError::NewStream),
        5 => Err(EncodeError::OpenOutput),
        6 => Err(EncodeError::WriteHeader),
        7 => Err(EncodeError::WriteFrame),
        8 => Err(EncodeError::NoAudioStream),
        9 => Err(EncodeError::Decoder),
        10 => Err(EncodeError::FilterGraph),
        11 => Err(EncodeError::Encoder),
        12 => Err(EncodeError::Pipeline),
        13 => Ok(EncodeOutcome::Cancelled),
        other => Err(EncodeError::Unknown(other)),
    }
}
