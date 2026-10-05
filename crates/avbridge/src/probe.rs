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
use std::os::raw::c_int;
use std::path::Path;

use crate::ffi::{avbridge_probe, RawProbeInfo};

/// What [`probe`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// `path` contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    /// `avformat_open_input` failed — bad path, unreadable file, or an unrecognized container.
    #[error("failed to open input")]
    Open,
    /// `avformat_find_stream_info` failed — the container opened but its streams couldn't be
    /// read.
    #[error("failed to read stream info")]
    StreamInfo,
    /// The file has neither a video nor an audio stream.
    #[error("no video or audio stream found")]
    NoMediaStream,
    /// The C side returned a status code this crate doesn't know about (version skew between
    /// `bridge.h` and this file).
    #[error("unknown probe status code: {0}")]
    Unknown(c_int),
}

/// The picked stream's kind — mirrors [`ProbeInfo::has_video`] but as an enum for callers that
/// want to match on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamKind {
    Video,
    Audio,
}

/// Metadata read from a media file's container/stream headers — no decoding.
#[derive(Debug, Clone, PartialEq)]
pub struct ProbeInfo {
    pub kind: StreamKind,
    /// Whether the container has at least one audio stream. Unlike [`Self::kind`], this remains
    /// true for a video file with embedded audio.
    pub has_audio: bool,
    pub duration_secs: f64,
    pub codec_name: String,
    /// Bits per second; `None` if neither the container nor the stream reports one.
    pub bit_rate: Option<u64>,
    /// `None` when [`Self::kind`] is [`StreamKind::Audio`].
    pub resolution: Option<(u32, u32)>,
    /// `None` when [`Self::kind`] is [`StreamKind::Audio`] or the rate is unreported.
    pub fps: Option<f32>,
    /// `None` when [`Self::kind`] is [`StreamKind::Video`] or the rate is unreported.
    pub sample_rate_hz: Option<u32>,
}

/// Probes the media file at `path` for codec, resolution, fps, bitrate and duration.
///
/// Picks the first video stream if there is one, otherwise the first audio stream — matching
/// the ffprobe-based version this replaces.
pub fn probe(path: &Path) -> Result<ProbeInfo, ProbeError> {
    let c_path =
        CString::new(path.to_string_lossy().as_bytes()).map_err(ProbeError::InvalidPath)?;
    let mut raw = RawProbeInfo {
        has_video: 0,
        has_audio: 0,
        duration_secs: 0.0,
        codec_name: [0; 32],
        bit_rate: 0,
        width: 0,
        height: 0,
        fps_num: 0,
        fps_den: 0,
        sample_rate_hz: 0,
    };

    // SAFETY: c_path is a valid NUL-terminated C string for the duration of this call, and
    // `raw` is a valid, fully-initialized `RawProbeInfo` the C side writes into on success (and
    // leaves untouched on error — checked below before reading any field except the status
    // code). `bridge.c` closes the `AVFormatContext` on every exit path.
    let status = unsafe { avbridge_probe(c_path.as_ptr(), &mut raw) };

    probe_status(status)?;

    // SAFETY: on success the C side NUL-terminates codec_name within the 32-byte buffer.
    let codec_name = unsafe { CStr::from_ptr(raw.codec_name.as_ptr()) }
        .to_string_lossy()
        .into_owned();

    let kind = if raw.has_video != 0 {
        StreamKind::Video
    } else {
        StreamKind::Audio
    };

    Ok(ProbeInfo {
        kind,
        has_audio: raw.has_audio != 0,
        duration_secs: raw.duration_secs,
        codec_name,
        bit_rate: (raw.bit_rate > 0).then_some(raw.bit_rate as u64),
        resolution: (kind == StreamKind::Video).then_some((raw.width as u32, raw.height as u32)),
        fps: (kind == StreamKind::Video && raw.fps_den != 0)
            .then_some(raw.fps_num as f32 / raw.fps_den as f32),
        sample_rate_hz: (kind == StreamKind::Audio && raw.sample_rate_hz > 0)
            .then_some(raw.sample_rate_hz as u32),
    })
}

/// Maps the status code the C side returned from the probe call; success carries no data.
pub(crate) fn probe_status(status: c_int) -> Result<(), ProbeError> {
    match status {
        0 => Ok(()),
        1 => Err(ProbeError::Open),
        2 => Err(ProbeError::StreamInfo),
        3 => Err(ProbeError::NoMediaStream),
        other => Err(ProbeError::Unknown(other)),
    }
}
