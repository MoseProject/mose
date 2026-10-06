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

use std::ffi::{CStr, CString, NulError};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::sync::Mutex;

use crate::ffi::avbridge_measure_loudness;

/// What [`measure_loudness_json`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum LoudnessError {
    /// `path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    /// `path` has no audio stream to measure.
    #[error("input has no audio stream")]
    NoAudioStream,
    /// Couldn't find/open the audio decoder.
    #[error("failed to open the audio decoder")]
    Decoder,
    /// Couldn't build the loudnorm filter graph.
    #[error("failed to build the loudnorm filter graph")]
    FilterGraph,
    /// A decode/filter call failed mid-stream (not at setup).
    #[error("decode/filter pipeline failed mid-stream")]
    Pipeline,
    /// The pipeline ran to completion but no loudnorm JSON report was captured.
    #[error("no loudnorm report was captured")]
    NoReport,
    /// The captured report wasn't valid UTF-8 (shouldn't happen — loudnorm's JSON output is
    /// ASCII — but the buffer is truncated if it doesn't fit, which could in principle split
    /// a multi-byte sequence).
    #[error("loudnorm report wasn't valid UTF-8: {0}")]
    InvalidUtf8(std::str::Utf8Error),
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown loudness status code: {0}")]
    Unknown(c_int),
}

/// Process-global mutex serializing access to `avbridge_measure_loudness()`. Required because
/// the C implementation installs a process-global FFmpeg `av_log` callback and stores the
/// caller's output buffer in process-global variables for the duration of each call. Concurrent
/// measurements would replace one another's capture state. The C callback separately protects
/// buffer access against logs from other FFmpeg workers. This mutex enforces the documented
/// single-measurement contract at the Rust API boundary.
pub(crate) static LOUDNESS_MUTEX: Mutex<()> = Mutex::new(());

/// Measures integrated loudness / true peak / loudness range via a single-pass `loudnorm`
/// analysis (`I=-16:TP=-1.5:LRA=11`). Returns the raw JSON report text — parsing it is the
/// caller's job (e.g. `avcore::loudness::parse_loudnorm_stderr`, which works on this
/// text unchanged since it never cared whether the text came from a subprocess or here).
///
/// Thread-safe: this function internally serializes all calls via a process-global mutex,
/// since the underlying C implementation uses process-global state (FFmpeg's `av_log` callback
/// and associated buffer pointers). Concurrent calls from multiple threads will block until
/// the active measurement completes.
pub fn measure_loudness_json(path: &Path) -> Result<String, LoudnessError> {
    // Acquire the lock before calling into the non-thread-safe C code. The lock is held for
    // the entire duration of the FFmpeg decode/filter pipeline to prevent concurrent calls
    // from racing the process-global log callback state.
    let _guard = LOUDNESS_MUTEX.lock().unwrap();

    let c_path =
        CString::new(path.to_string_lossy().as_bytes()).map_err(LoudnessError::InvalidPath)?;

    let mut buf = vec![0u8; 4096];

    // SAFETY: c_path is a valid NUL-terminated C string for the duration of this call. `buf`
    // is a valid, writable buffer of `buf.len()` bytes; the C side never writes past that
    // length and always NUL-terminates within it on success. `bridge.c` closes the decoder
    // and filter graph contexts on every exit path. The LOUDNESS_MUTEX ensures no other thread
    // can enter this unsafe block concurrently. The C capture lock synchronizes callbacks
    // with buffer writes and detachment before returning.
    let status = unsafe {
        avbridge_measure_loudness(c_path.as_ptr(), buf.as_mut_ptr() as *mut c_char, buf.len())
    };

    loudness_status(status)?;

    // SAFETY: on success the C side NUL-terminates the report within `buf`.
    let json = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) };
    json.to_str()
        .map(str::to_owned)
        .map_err(LoudnessError::InvalidUtf8)
}

/// Maps the status code the C side returned from the loudness call; success carries no data.
pub(crate) fn loudness_status(status: c_int) -> Result<(), LoudnessError> {
    match status {
        0 => Ok(()),
        1 => Err(LoudnessError::OpenInput),
        2 => Err(LoudnessError::StreamInfo),
        3 => Err(LoudnessError::NoAudioStream),
        4 => Err(LoudnessError::Decoder),
        5 => Err(LoudnessError::FilterGraph),
        6 => Err(LoudnessError::Pipeline),
        7 => Err(LoudnessError::NoReport),
        other => Err(LoudnessError::Unknown(other)),
    }
}
