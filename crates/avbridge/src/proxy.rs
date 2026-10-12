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

use crate::ffi::avbridge_generate_proxy_ex;

/// What [`generate_proxy`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum ProxyError {
    /// `in_path`/`out_path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to read stream info")]
    StreamInfo,
    /// `in_path` has no video stream to make a proxy of.
    #[error("input has no video stream")]
    NoVideoStream,
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
    /// Couldn't find/open the video or audio decoder.
    #[error("failed to open a decoder")]
    Decoder,
    /// Couldn't find/open a supported CPU H.264 video encoder or the AAC audio encoder.
    #[error("failed to open an encoder")]
    Encoder,
    /// Couldn't build the video scaler.
    #[error("failed to build the video scaler")]
    Scaler,
    /// Couldn't build the (filterless, format-conversion-only) audio graph.
    #[error("failed to build the audio format-match graph")]
    FilterGraph,
    /// A decode/scale/encode call failed mid-stream (not at setup).
    #[error("decode/scale/encode pipeline failed mid-stream")]
    Pipeline,
    /// The C side returned a status code this crate doesn't know about.
    #[error("unknown proxy status code: {0}")]
    Unknown(c_int),
}

/// Generates a downscaled editing proxy of `in_path`'s video (height = `target_height`, width
/// computed to preserve the source's aspect ratio) via the available CPU H.264 encoder
/// (`libopenh264` when present, otherwise `libx264`). Any audio stream is re-encoded to AAC 128kbps
/// unchanged otherwise. Fails with [`ProxyError::NoVideoStream`] if `in_path` has no video
/// stream; a video-only source produces a video-only proxy, no error.
pub fn generate_proxy(
    in_path: &Path,
    out_path: &Path,
    target_height: u32,
) -> Result<(), ProxyError> {
    generate_proxy_with(in_path, out_path, target_height, true)
}

/// [`generate_proxy`] choosing what happens to an HDR source: with `tone_map_hdr` it becomes an SDR
/// proxy, without it the proxy is tagged SDR and the pixel values are left as they are (the
/// "treat as SDR" choice).
pub fn generate_proxy_with(
    in_path: &Path,
    out_path: &Path,
    target_height: u32,
    tone_map_hdr: bool,
) -> Result<(), ProxyError> {
    let c_in =
        CString::new(in_path.to_string_lossy().as_bytes()).map_err(ProxyError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(ProxyError::InvalidPath)?;

    // SAFETY: c_in/c_out are valid NUL-terminated C strings for the duration of this call.
    // `bridge.c` frees the decoder/encoder/scaler/filter-graph contexts, output I/O, and
    // output context on every exit path.
    let status = unsafe {
        avbridge_generate_proxy_ex(
            c_in.as_ptr(),
            c_out.as_ptr(),
            target_height as c_int,
            c_int::from(tone_map_hdr),
        )
    };

    generate_proxy_status(status)
}

/// Maps the status code the C side returned from `generate_proxy` to its result.
pub(crate) fn generate_proxy_status(status: c_int) -> Result<(), ProxyError> {
    match status {
        0 => Ok(()),
        1 => Err(ProxyError::OpenInput),
        2 => Err(ProxyError::StreamInfo),
        3 => Err(ProxyError::NoVideoStream),
        4 => Err(ProxyError::AllocOutput),
        5 => Err(ProxyError::NewStream),
        6 => Err(ProxyError::OpenOutput),
        7 => Err(ProxyError::WriteHeader),
        8 => Err(ProxyError::WriteFrame),
        9 => Err(ProxyError::Decoder),
        10 => Err(ProxyError::Encoder),
        11 => Err(ProxyError::Scaler),
        12 => Err(ProxyError::FilterGraph),
        13 => Err(ProxyError::Pipeline),
        other => Err(ProxyError::Unknown(other)),
    }
}
