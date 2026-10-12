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

use crate::ffi::{avbridge_probe, RawAudioStream, RawProbeInfo, PROBE_MAX_AUDIO_STREAMS};

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
    /// How the video stream's pixels are meant to be read; `None` for audio-only files.
    pub color: Option<ColorDescription>,
    /// Every audio stream in stream order (at most eight are described).
    pub audio_streams: Vec<AudioStreamInfo>,
    /// How many audio streams the file has, which can exceed `audio_streams.len()`.
    pub audio_stream_count: usize,
}

/// The colour description of a video stream, as FFmpeg's own enum numbers (`AVPixelFormat`,
/// `AVColorSpace`, `AVColorPrimaries`, `AVColorTransferCharacteristic`); 2 means unspecified in
/// the last three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorDescription {
    /// `-1` when unknown.
    pub pixel_format: i32,
    pub bit_depth: u32,
    pub space: i32,
    pub primaries: i32,
    pub transfer: i32,
    pub full_range: bool,
}

/// One audio stream of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioStreamInfo {
    /// Index of the stream within the container.
    pub stream_index: u32,
    pub channels: u32,
    pub sample_rate_hz: u32,
    pub codec_name: String,
    /// The stream's `title` tag (OBS names its tracks), empty when absent.
    pub title: String,
    /// The stream's `language` tag, empty when absent.
    pub language: String,
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
        pixel_format: -1,
        bit_depth: 0,
        color_space: 0,
        color_primaries: 0,
        color_transfer: 0,
        color_full_range: 0,
        audio_stream_count: 0,
        audio_streams: [RawAudioStream {
            stream_index: 0,
            channels: 0,
            sample_rate_hz: 0,
            codec_name: [0; 32],
            title: [0; 64],
            language: [0; 16],
        }; PROBE_MAX_AUDIO_STREAMS],
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
        color: (kind == StreamKind::Video).then_some(ColorDescription {
            pixel_format: raw.pixel_format,
            bit_depth: raw.bit_depth.max(0) as u32,
            space: raw.color_space,
            primaries: raw.color_primaries,
            transfer: raw.color_transfer,
            full_range: raw.color_full_range != 0,
        }),
        audio_streams: raw
            .audio_streams
            .iter()
            .take((raw.audio_stream_count.max(0) as usize).min(PROBE_MAX_AUDIO_STREAMS))
            .map(|stream| AudioStreamInfo {
                stream_index: stream.stream_index.max(0) as u32,
                channels: stream.channels.max(0) as u32,
                sample_rate_hz: stream.sample_rate_hz.max(0) as u32,
                // SAFETY: the C side NUL-terminates each buffer within its bounds.
                codec_name: unsafe { CStr::from_ptr(stream.codec_name.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
                title: unsafe { CStr::from_ptr(stream.title.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
                language: unsafe { CStr::from_ptr(stream.language.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
            })
            .collect(),
        audio_stream_count: raw.audio_stream_count.max(0) as usize,
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
