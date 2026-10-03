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

//! `oca-avbridge` — thin C bridge over libavformat/libavcodec/libavutil (FFmpeg), called via
//! FFI. Replaces spawning `ffprobe`/`ffmpeg` as external processes: `core` links this
//! crate directly, so probing and encoding never shell out or parse subprocess stdout.
//!
//! The C surface (`csrc/bridge.c`) is written for this project — not a full auto-generated
//! FFmpeg binding — so it stays small and auditable. See [`build.rs`](../build.rs) for how the
//! FFmpeg dev libs (`FFMPEG_DIR`) are located and linked.

mod audio_decode;

pub use audio_decode::{AudioDecodeError, AudioDecodeInfo, AudioDecodeSession, DecodedAudioBlock};

use std::ffi::{c_void, CStr, CString, NulError};
use std::os::raw::{c_char, c_int, c_longlong};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

#[repr(C)]
struct RawClipSegment {
    source_path: *const c_char,
    source_in_secs: f64,
    source_out_secs: f64,
    gain_db: f32,
    video_filter: *const c_char,
    frozen: c_int,
    speed_factor: f32,
    /// `0.0` (or negative) means no ramp — see `ClipSegment::smooth_speed_ramp_end_factor`'s
    /// doc comment in `bridge.h`.
    smooth_speed_ramp_end_factor: f32,
    position_x_expr: *const c_char,
    position_y_expr: *const c_char,
    transition_in: c_int,
    transition_duration_secs: f32,
    /// Start position of this clip on the shared timeline in seconds. Used by
    /// `avbridge_encode_timeline_export_multi` to determine which overlay tracks are active at
    /// any given decoded-frame time. Ignored by `avbridge_encode_timeline_export`.
    timeline_start_secs: f64,
    /// Path to a grayscale-as-luma alpha-matte video, or an empty string for none — see
    /// `ClipSegment::mask_video_path`'s doc comment in `bridge.h`.
    mask_video_path: *const c_char,
    /// FFmpeg `blend` filter mode name (`blend=all_mode=<name>`), or an empty string for
    /// "no blend mode, use plain `overlay` compositing" — see `ClipSegment::blend_mode`'s doc
    /// comment in `bridge.h`.
    blend_mode: *const c_char,
    /// Fraction (`0.0..=1.0`) of this clip's own decoded frame that rotation/scale pivot
    /// around — see `ClipSegment::anchor_x`/`anchor_y`'s doc comment in `bridge.h`.
    anchor_x: f64,
    anchor_y: f64,
}

#[repr(C)]
struct RawAudioSegment {
    source_path: *const c_char,
    source_in_secs: f64,
    source_out_secs: f64,
    timeline_start_secs: f64,
    gain_db: f32,
    speed_factor: f32,
    /// NULL or empty means "use gain_db unchanged" — see `AudioSegment::gain_keyframe_expr`.
    gain_keyframe_expr: *const c_char,
    duck_role: c_int,
    /// CF-03 "Gameplay Voice" cleanup — see `AudioSegment::voice_cleanup_enabled`'s doc comment.
    voice_cleanup_enabled: c_int,
    voice_cleanup_noise_floor_db: f32,
    voice_cleanup_compressor_threshold_db: f32,
    voice_cleanup_compressor_ratio: f32,
    voice_cleanup_ceiling_linear: f32,
}

/// Mirror of `TextSegment` in `bridge.h` — one pre-rasterized text overlay image.
#[repr(C)]
struct RawTextSegment {
    start_secs: f64,
    duration_secs: f64,
    overlay_path: *const c_char,
    /// NULL or empty means "always fully opaque" — see `TextOverlaySegment::opacity_keyframe_expr`.
    opacity_keyframe_expr: *const c_char,
    /// NULL or empty means "no offset" — see `TextOverlaySegment::position_keyframe_expr_x`/`_y`.
    position_keyframe_expr_x: *const c_char,
    position_keyframe_expr_y: *const c_char,
    /// NULL, empty, or either-one-empty means "no remap" — see
    /// `TextOverlaySegment::scale_keyframe_expr_x`/`_y`.
    scale_keyframe_expr_x: *const c_char,
    scale_keyframe_expr_y: *const c_char,
    /// NULL, empty, or either-one-empty means "no remap" — see
    /// `TextOverlaySegment::rotation_keyframe_expr_x`/`_y`.
    rotation_keyframe_expr_x: *const c_char,
    rotation_keyframe_expr_y: *const c_char,
}

#[repr(C)]
struct RawShapeSegment {
    filter_desc: *const c_char,
}

#[repr(C)]
struct RawProbeInfo {
    has_video: c_int,
    has_audio: c_int,
    duration_secs: f64,
    codec_name: [c_char; 32],
    bit_rate: c_longlong,
    width: c_int,
    height: c_int,
    fps_num: c_int,
    fps_den: c_int,
    sample_rate_hz: c_int,
}

#[repr(C)]
struct RawDecodeInfo {
    width: c_int,
    height: c_int,
    fps_num: c_int,
    fps_den: c_int,
    duration_secs: f64,
    hardware: c_int,
}

#[repr(C)]
struct RawDecodedRgbaFrame {
    pixels: *mut u8,
    pixel_len: usize,
    width: c_int,
    height: c_int,
    stride_bytes: c_int,
    timestamp_secs: f64,
}

#[repr(C)]
struct RawDecodeSession {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn avbridge_version() -> u32;
    fn avbridge_probe(path: *const c_char, out: *mut RawProbeInfo) -> c_int;
    fn avbridge_remux_copy(in_path: *const c_char, out_path: *const c_char) -> c_int;
    fn avbridge_encode_export(
        in_path: *const c_char,
        out_path: *const c_char,
        target_lufs: f32,
        progress_cb: Option<unsafe extern "C" fn(user_data: *mut c_void, seconds: f64)>,
        progress_user_data: *mut c_void,
        cancel: *const u8,
    ) -> c_int;
    fn avbridge_encode_timeline_export(
        segments: *const RawClipSegment,
        segment_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
        canvas_bit_rate_bps: c_longlong,
        out_path: *const c_char,
        target_lufs: f32,
        gpu_encoder_preference: c_int,
        progress_cb: Option<unsafe extern "C" fn(user_data: *mut c_void, seconds: f64)>,
        progress_user_data: *mut c_void,
        cancel: *const u8,
    ) -> c_int;
    fn avbridge_encode_timeline_export_multi(
        track_segs: *const *const RawClipSegment,
        track_n_segs: *const c_int,
        n_tracks: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
        canvas_bit_rate_bps: c_longlong,
        out_path: *const c_char,
        target_lufs: f32,
        gpu_encoder_preference: c_int,
        progress_cb: Option<unsafe extern "C" fn(user_data: *mut c_void, seconds: f64)>,
        progress_user_data: *mut c_void,
        cancel: *const u8,
    ) -> c_int;
    fn avbridge_mix_audio_timeline(
        segments: *const RawAudioSegment,
        segment_count: c_int,
        timeline_duration_secs: f64,
        out_path: *const c_char,
        target_lufs: f32,
        cancel: *const u8,
    ) -> c_int;
    fn avbridge_mux_video_audio(
        video_path: *const c_char,
        audio_path: *const c_char,
        out_path: *const c_char,
    ) -> c_int;
    fn avbridge_measure_loudness(
        in_path: *const c_char,
        out_json: *mut c_char,
        out_json_len: usize,
    ) -> c_int;
    fn avbridge_generate_proxy(
        in_path: *const c_char,
        out_path: *const c_char,
        target_height: c_int,
    ) -> c_int;
    fn avbridge_generate_waveform(
        in_path: *const c_char,
        bucket_count: c_int,
        out_min: *mut f32,
        out_max: *mut f32,
    ) -> c_int;
    fn avbridge_apply_text_overlays(
        in_path: *const c_char,
        out_path: *const c_char,
        segments: *const RawTextSegment,
        segment_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    fn avbridge_apply_shape_overlays(
        in_path: *const c_char,
        out_path: *const c_char,
        segments: *const RawShapeSegment,
        segment_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    fn avbridge_extract_pcm_16k_mono(
        in_path: *const c_char,
        out_samples: *mut *mut f32,
        out_sample_count: *mut i64,
    ) -> c_int;
    fn avbridge_free_pcm_buffer(samples: *mut f32);
    fn avbridge_encode_matte_video(
        luma_frames: *const u8,
        frame_count: c_int,
        width: c_int,
        height: c_int,
        fps_num: c_int,
        fps_den: c_int,
        out_path: *const c_char,
    ) -> c_int;
    fn avbridge_apply_privacy_blur(
        in_path: *const c_char,
        out_path: *const c_char,
        matte_path: *const c_char,
        blur_sigma: f64,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    fn avbridge_decode_open_ex(
        path: *const c_char,
        hardware: c_int,
        out_session: *mut *mut RawDecodeSession,
        out_info: *mut RawDecodeInfo,
    ) -> c_int;
    fn avbridge_decode_seek(session: *mut RawDecodeSession, timestamp_secs: f64) -> c_int;
    fn avbridge_decode_next_rgba(
        session: *mut RawDecodeSession,
        cancel: *const u8,
        out_frame: *mut RawDecodedRgbaFrame,
    ) -> c_int;
    fn avbridge_decode_free_frame(frame: *mut RawDecodedRgbaFrame);
    fn avbridge_decode_close(session: *mut RawDecodeSession);
    fn avbridge_hw_device_cache_clear();
}

/// Trampoline handed to the C side as `progress_cb`; `user_data` is a `*mut F` for whatever
/// closure [`encode_export`] was called with.
unsafe extern "C" fn progress_trampoline<F: FnMut(f64)>(user_data: *mut c_void, seconds: f64) {
    // SAFETY: `user_data` was set to `&mut on_progress` for the duration of the single
    // `avbridge_encode_export` call this trampoline is only ever invoked from, and the
    // pointee outlives that call (it's a stack local in `encode_export`, held for the call).
    let closure = unsafe { &mut *(user_data as *mut F) };
    closure(seconds);
}

/// libavformat's packed version number (same encoding as `LIBAVFORMAT_VERSION_INT` /
/// `avformat_version()`): `(major << 16) | (minor << 8) | micro`.
pub fn version() -> u32 {
    // SAFETY: avbridge_version() takes no arguments, returns a plain u32, and has no
    // documented failure mode in libavformat — it's a pure accessor.
    unsafe { avbridge_version() }
}

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

    match status {
        0 => {}
        1 => return Err(ProbeError::Open),
        2 => return Err(ProbeError::StreamInfo),
        3 => return Err(ProbeError::NoMediaStream),
        other => return Err(ProbeError::Unknown(other)),
    }

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
        let width = u32::try_from(info.width).map_err(|_| DecodeError::FrameSize)?;
        let height = u32::try_from(info.height).map_err(|_| DecodeError::FrameSize)?;
        if width == 0 || height == 0 {
            // SAFETY: successful open transfers ownership of raw to this call; release it before
            // rejecting an invalid native metadata result.
            unsafe { avbridge_decode_close(raw) };
            return Err(DecodeError::FrameSize);
        }
        Ok((
            Self { raw },
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
        match unsafe { avbridge_decode_seek(self.raw, timestamp_secs) } {
            0 => Ok(()),
            7 => Err(DecodeError::Seek),
            other => Err(DecodeError::Unknown(other)),
        }
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
        match status {
            0 => {
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
                    unsafe { avbridge_decode_free_frame(&mut raw_frame) };
                    return Err(DecodeError::FrameSize);
                }
                // SAFETY: DECODE_OK guarantees `pixel_len` initialized bytes at `pixels`.
                let pixels = unsafe {
                    std::slice::from_raw_parts(raw_frame.pixels, raw_frame.pixel_len).to_vec()
                };
                let timestamp_secs = raw_frame.timestamp_secs;
                // SAFETY: releases only the C allocation copied above.
                unsafe { avbridge_decode_free_frame(&mut raw_frame) };
                Ok(Some(DecodedRgbaFrame {
                    width,
                    height,
                    stride_bytes,
                    timestamp_secs,
                    pixels,
                }))
            }
            1 => Ok(None),
            2 => Ok(None),
            8 => Err(DecodeError::Pipeline),
            9 => Err(DecodeError::FrameSize),
            other => Err(DecodeError::Unknown(other)),
        }
    }
}

impl Drop for DecodeSession {
    fn drop(&mut self) {
        // SAFETY: self uniquely owns the non-null raw session returned by avbridge_decode_open.
        unsafe { avbridge_decode_close(self.raw) };
    }
}

fn decode_open_status(status: c_int) -> Result<(), DecodeError> {
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

/// One trimmed clip in an [`encode_timeline_export`] timeline: a source file, the range of it
/// to use (`source_in_secs..source_out_secs`), a linear gain in dB, and a pre-built avfilter
/// chain description for this clip's own video effects (empty string = none — the segment
/// still gets canvas-conformed and re-encoded).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClipSegment {
    pub source_path: std::path::PathBuf,
    pub source_in_secs: f64,
    pub source_out_secs: f64,
    pub gain_db: f32,
    pub video_filter: String,
    /// `true` holds the first decoded video frame at/after `source_in_secs` for this segment's
    /// whole trimmed duration instead of playing through the range — a freeze frame.
    /// `video_filter` still applies to every held frame. Audio is unaffected either way — still
    /// decoded/played across the full `source_in_secs..source_out_secs` range.
    pub frozen: bool,
    /// Playback speed multiplier — `1.0` is normal speed, `2.0` is double speed,
    /// `0.5` is half speed. Drives a `setpts=PTS/speed` filter before the fps
    /// conform stage (video) and an `atempo=speed` command in the shared audio
    /// filter graph. Audio is clamped to `[0.5, 100.0]` (atempo's range).
    pub speed_factor: f32,
    /// End speed of a smooth, continuous speed ramp across this segment's whole trimmed source
    /// duration. `0.0` (or negative — the default, and what every pre-existing job
    /// deserializes to via `#[serde(default)]`) means no ramp: `speed_factor` above is used
    /// unchanged. When positive, `speed_factor` is instead the ramp's *start* speed and this is
    /// its end speed — drives a `setpts` expression that's the integral of `1/speed(t)` (see
    /// `bridge.h`'s own doc comment on the mirrored C field) instead of the plain
    /// `setpts=PTS/speed_factor` constant-multiplier form. Per `spec/ROADMAP.md` P4 item 29's
    /// "smooth continuous curve" follow-up.
    #[serde(default)]
    pub smooth_speed_ramp_end_factor: f32,
    /// Overlay-compositor `x`/`y` position expressions (avfilter expression syntax), built in
    /// Rust from this clip's position keyframes (`core::keyframe::position_overlay_xy_expr`).
    /// Empty string = no offset. Only consulted by `encode_timeline_export_multi`'s overlay
    /// path — a single/background-track clip has no compositing stage to apply this to.
    /// Scale/rotation/opacity keyframes don't have their own fields here — they're already
    /// folded into `video_filter`, built the same way.
    pub position_x_expr: String,
    pub position_y_expr: String,
    /// Transition effect at this clip's entry. `0` = None/HardCut (no effect), `1` = Fade
    /// (fade in from black), `2` = Slide (reveal from left), `3` = Zoom (scale from 50% to
    /// 100%). Handled in `bridge.c` by appending an animated avfilter expression after
    /// `video_filter` and before the final `format=yuv420p` conform step.
    pub transition_in: u8,
    /// Duration of [`ClipSegment::transition_in`] in seconds. Ignored when `transition_in` is
    /// `0`. Converted to a frame count in `bridge.c` using the canvas fps.
    pub transition_duration_secs: f32,
    /// Start position of this clip on the shared timeline in seconds.  Used by
    /// [`encode_timeline_export_multi`] to determine which overlay tracks are active at any
    /// given decoded-frame time.  Ignored by [`encode_timeline_export`] (single-track function
    /// concatenates in order with no gaps).  Defaults to `0.0` for backwards-compatible
    /// deserialization of jobs saved before this field existed.
    #[serde(default)]
    pub timeline_start_secs: f64,
    /// Path to a grayscale-as-luma alpha-matte video for AI background removal (see
    /// `avcore::background_removal::encode_matte_video`), or an empty string for none.  When
    /// set, [`encode_timeline_export_multi`] decodes it alongside `source_path` and
    /// alphamerges its luma onto this segment's video before compositing — replacing, not
    /// combining with, any alpha `video_filter` already produced (e.g. a mask_shape/chroma_key
    /// alpha on the same clip).  The matte's own internal timeline is 0-based and covers
    /// exactly `[0, source_out_secs - source_in_secs)` — this segment's own trim range, not the
    /// timeline (post-`speed_factor`) duration, since the matte was generated by sampling
    /// `source_path` directly.  Only consulted by [`encode_timeline_export_multi`]'s overlay
    /// path — ignored by [`encode_timeline_export`].  Defaults to an empty string for
    /// backwards-compatible deserialization of jobs saved before this field existed.
    #[serde(default)]
    pub mask_video_path: String,
    /// FFmpeg `blend` filter mode name (e.g. `"multiply"`, `"screen"` — the exact string
    /// `blend`'s `all_mode` option accepts; this crate doesn't depend on `core`, so it takes
    /// the mode as a plain string built by `avcore::timeline::BlendMode::ffmpeg_name` rather
    /// than that enum itself, same convention [`ClipSegment::transition_in`] already uses for
    /// `avcore::timeline::TransitionType`). Empty string means "no blend mode" — this segment
    /// composites via plain alpha-over `overlay` (the behavior before this field existed).
    /// Only consulted by [`encode_timeline_export_multi`]'s overlay path (a single/background-
    /// track clip has no compositing stage to apply this to) — ignored by
    /// [`encode_timeline_export`]. **When set, replaces `overlay` entirely for this layer**:
    /// `position_x_expr`/`position_y_expr` above are ignored for this segment while a blend
    /// mode is active — the layer blends at full canvas size, not at an offset position (see
    /// `avcore::timeline::ClipInstance::blend_mode`'s own doc comment for why combining the two
    /// isn't supported yet). Defaults to an empty string for backwards-compatible
    /// deserialization of jobs saved before this field existed.
    #[serde(default)]
    pub blend_mode: String,
    /// Fraction (`0.0..=1.0`, `x` then `y`) of this clip's own frame that rotation/scale pivot
    /// around, instead of the frame's own center — `(0.5, 0.5)` (the default, and what every
    /// pre-existing job deserializes to via `#[serde(default_anchor)]`) is exactly the previous,
    /// hardcoded-center behavior: the `scale=...,pad=cw:ch:(ow-iw)/2:(oh-ih)/2` stage every
    /// segment already goes through (canvas-conforming) centers the decoded content within the
    /// canvas-sized buffer that `video_filter`'s own `rotate=...` stage (when scale/rotation
    /// keyframes are set) then pivots around — replacing that fixed `/2` offset with
    /// `ow/2-(anchor_x)*iw`/`oh/2-(anchor_y)*ih` moves the pivot to an arbitrary point on the
    /// content instead, while reducing to the identical expression at `(0.5, 0.5)` (verified
    /// against real `ffmpeg` output, byte-identical). Applies uniformly to every clip (unlike
    /// `blend_mode`/`position_x_expr`, this isn't overlay-only — every segment goes through the
    /// same canvas-conforming pad stage in both `encode_timeline_export` and
    /// `encode_timeline_export_multi`).
    #[serde(default = "default_anchor")]
    pub anchor_x: f32,
    #[serde(default = "default_anchor")]
    pub anchor_y: f32,
}

fn default_anchor() -> f32 {
    0.5
}

/// One independently placed audio contributor in a timeline mix. Unlike [`ClipSegment`], this
/// is valid for both video assets with embedded audio and audio-only assets; no video metadata
/// crosses the FFI boundary for this pass.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AudioSegment {
    pub source_path: std::path::PathBuf,
    pub source_in_secs: f64,
    pub source_out_secs: f64,
    pub timeline_start_secs: f64,
    pub gain_db: f32,
    pub speed_factor: f32,
    /// FFmpeg `volume` filter expression (linear multiplier, `t`-keyed from this segment's own
    /// trim start) driving a keyframed gain ramp instead of the constant `gain_db` above — built
    /// by `avcore::keyframe::gain_filter_db_expr` from a `ClipInstance`'s `gain_keyframes`. Empty
    /// string means "use `gain_db` unchanged" (the common case — no keyframed gain). `#[serde(
    /// default)]` so an `.ocqueue` job persisted before this field existed loads with no ramp.
    #[serde(default)]
    pub gain_keyframe_expr: String,
    /// Audio-ducking role (P2 item 6, "Auto Ducking"): `0` mixes in as-is, `1` is the sidechain
    /// trigger (still plays itself, and ducks every `2` branch under it), `2` is ducked under
    /// trigger branches via `sidechaincompress`. Built by `core`'s
    /// `avcore::timeline::AudioRole::to_duck_role_code()` — this crate doesn't depend on `core`,
    /// so it stays a raw code here rather than that enum, same convention
    /// [`ClipSegment::transition_in`] already uses for `TransitionType`. `#[serde(default)]` so
    /// an `.ocqueue` job persisted before this field existed loads as `0` (no ducking) rather
    /// than failing to parse.
    #[serde(default)]
    pub duck_role: u8,
    /// CF-03 "Gameplay Voice" cleanup (`spec/architecture/competitive-feature-plan.md`) — `true`
    /// splices `highpass=f=80,afftdn=nf=<voice_cleanup_noise_floor_db>,acompressor=threshold=
    /// <voice_cleanup_compressor_threshold_db>dB:ratio=<voice_cleanup_compressor_ratio>:
    /// attack=10:release=250:makeup=1.5,alimiter=limit=<voice_cleanup_ceiling_linear>` between
    /// this branch's own `volume` and `delay` stages in `audio_mix.c`'s `build_mix_graph`, ahead
    /// of the whole-mix `afftdn`/`loudnorm`/`alimiter` mastering pass every export already runs.
    /// Values match `scripts/Watch-Gameplay.ps1`'s own proven chain. `#[serde(default)]` so an
    /// `.ocqueue` job persisted before this field existed loads with cleanup off.
    #[serde(default)]
    pub voice_cleanup_enabled: bool,
    #[serde(default = "default_voice_cleanup_noise_floor_db")]
    pub voice_cleanup_noise_floor_db: f32,
    #[serde(default = "default_voice_cleanup_compressor_threshold_db")]
    pub voice_cleanup_compressor_threshold_db: f32,
    #[serde(default = "default_voice_cleanup_compressor_ratio")]
    pub voice_cleanup_compressor_ratio: f32,
    #[serde(default = "default_voice_cleanup_ceiling_linear")]
    pub voice_cleanup_ceiling_linear: f32,
}

fn default_voice_cleanup_noise_floor_db() -> f32 {
    -30.0
}

fn default_voice_cleanup_compressor_threshold_db() -> f32 {
    -18.0
}

fn default_voice_cleanup_compressor_ratio() -> f32 {
    3.0
}

fn default_voice_cleanup_ceiling_linear() -> f32 {
    0.95
}

/// The fixed output frame size/rate every segment in an [`encode_timeline_export`] call is
/// scaled/padded/frame-rate-conformed onto.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub fps_num: u32,
    pub fps_den: u32,
    /// Target video bitrate in bits per second for the whole timeline's single encoder —
    /// unlike [`encode_export`]'s exact source-bitrate passthrough, this is a target the
    /// libopenh264 encoder aims for (there's no way to guarantee an exact output bitrate once
    /// video is re-encoded rather than stream-copied).
    pub bit_rate_bps: i64,
}

/// Which video encoder [`encode_timeline_export`]/[`encode_timeline_export_multi`] should use,
/// mirroring `bridge_internal.h`'s `GpuEncoderPreference` (passed across the FFI boundary as
/// the same plain `int` values). `Auto` tries hardware encoders (NVENC, Quick Sync, VAAPI, then
/// AMF, plus VideoToolbox on macOS) in order and falls back to the CPU (libopenh264) encoder if
/// none open; the specific hardware variants force that one encoder, still falling back to CPU
/// if it can't open (no compatible GPU/driver present) — the C side has no way to report back
/// which one actually got used, so a caller can't currently distinguish "used the GPU I asked
/// for" from "silently fell back to CPU" short of noticing render speed/CPU usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GpuEncoderPreference {
    #[default]
    Auto,
    Cpu,
    Nvenc,
    QuickSync,
    Amf,
    Vaapi,
    VideoToolbox,
}

impl GpuEncoderPreference {
    fn as_c_int(self) -> c_int {
        match self {
            GpuEncoderPreference::Auto => 0,
            GpuEncoderPreference::Cpu => 1,
            GpuEncoderPreference::Nvenc => 2,
            GpuEncoderPreference::QuickSync => 3,
            GpuEncoderPreference::Amf => 4,
            GpuEncoderPreference::Vaapi => 5,
            GpuEncoderPreference::VideoToolbox => 6,
        }
    }
}

/// Renders `segments` as one continuous export onto a `canvas_width`x`canvas_height` canvas
/// at `canvas_fps_num`/`canvas_fps_den`: video is decoded, each segment's own filter chain
/// applied (prefixed with a canvas-conform scale/pad/fps stage), and re-encoded via
/// the available CPU H.264 encoder — unlike [`encode_export`], video is never stream-copied, since each clip may
/// need a different filter chain. Audio across all segments runs through ONE continuous
/// loudnorm+limiter graph (so normalization sees the whole timeline) before being re-encoded
/// to AAC — every segment's audio must therefore share the same sample rate/format/channel
/// layout ([`EncodeError::AudioFormatMismatch`] otherwise), and every segment must have both a
/// video and an audio stream ([`EncodeError::NoVideoStream`] / [`EncodeError::NoAudioStream`]).
///
/// Calls `on_progress(seconds_processed)` with the cumulative timeline position, and checks
/// `cancel` between packets — same cancellation contract as [`encode_export`].
pub fn encode_timeline_export<F: FnMut(f64)>(
    segments: &[ClipSegment],
    canvas: Canvas,
    out_path: &Path,
    target_lufs: f32,
    gpu_encoder: GpuEncoderPreference,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> Result<EncodeOutcome, EncodeError> {
    if segments.is_empty() {
        return Err(EncodeError::EmptyTimeline);
    }

    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(EncodeError::InvalidPath)?;

    let mut c_paths = Vec::with_capacity(segments.len());
    let mut c_filters = Vec::with_capacity(segments.len());
    let mut c_pos_x = Vec::with_capacity(segments.len());
    let mut c_pos_y = Vec::with_capacity(segments.len());
    let mut c_mask_paths = Vec::with_capacity(segments.len());
    let mut c_blend_modes = Vec::with_capacity(segments.len());
    for seg in segments {
        c_paths.push(
            CString::new(seg.source_path.to_string_lossy().as_bytes())
                .map_err(EncodeError::InvalidPath)?,
        );
        c_filters
            .push(CString::new(seg.video_filter.as_bytes()).map_err(EncodeError::InvalidPath)?);
        c_pos_x
            .push(CString::new(seg.position_x_expr.as_bytes()).map_err(EncodeError::InvalidPath)?);
        c_pos_y
            .push(CString::new(seg.position_y_expr.as_bytes()).map_err(EncodeError::InvalidPath)?);
        c_mask_paths
            .push(CString::new(seg.mask_video_path.as_bytes()).map_err(EncodeError::InvalidPath)?);
        c_blend_modes
            .push(CString::new(seg.blend_mode.as_bytes()).map_err(EncodeError::InvalidPath)?);
    }
    let raw_segments: Vec<RawClipSegment> = segments
        .iter()
        .zip(c_paths.iter())
        .zip(c_filters.iter())
        .zip(c_pos_x.iter())
        .zip(c_pos_y.iter())
        .zip(c_mask_paths.iter())
        .zip(c_blend_modes.iter())
        .map(
            |((((((seg, path), filt), pos_x), pos_y), mask_path), blend_mode)| RawClipSegment {
                source_path: path.as_ptr(),
                source_in_secs: seg.source_in_secs,
                source_out_secs: seg.source_out_secs,
                gain_db: seg.gain_db,
                video_filter: filt.as_ptr(),
                frozen: seg.frozen as c_int,
                speed_factor: seg.speed_factor,
                smooth_speed_ramp_end_factor: seg.smooth_speed_ramp_end_factor,
                position_x_expr: pos_x.as_ptr(),
                position_y_expr: pos_y.as_ptr(),
                transition_in: seg.transition_in as c_int,
                transition_duration_secs: seg.transition_duration_secs,
                timeline_start_secs: seg.timeline_start_secs,
                mask_video_path: mask_path.as_ptr(),
                blend_mode: blend_mode.as_ptr(),
                anchor_x: seg.anchor_x as f64,
                anchor_y: seg.anchor_y as f64,
            },
        )
        .collect();

    // SAFETY: raw_segments' pointers stay valid for the call — c_paths/c_filters/c_mask_paths/
    // c_blend_modes (which they point into) and raw_segments itself are all stack locals held
    // alive until this function returns, none of them mutated during the call. c_out,
    // progress_trampoline::<F>, and cancel.as_ptr() carry the same safety argument as
    // encode_export's identical call.
    let status = unsafe {
        avbridge_encode_timeline_export(
            raw_segments.as_ptr(),
            raw_segments.len() as c_int,
            canvas.width as c_int,
            canvas.height as c_int,
            canvas.fps_num as c_int,
            canvas.fps_den as c_int,
            canvas.bit_rate_bps as c_longlong,
            c_out.as_ptr(),
            target_lufs,
            gpu_encoder.as_c_int(),
            Some(progress_trampoline::<F>),
            &mut on_progress as *mut _ as *mut c_void,
            cancel.as_ptr() as *const u8,
        )
    };

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
        14 => Err(EncodeError::NoVideoStream),
        15 => Err(EncodeError::AudioFormatMismatch),
        16 => Err(EncodeError::EmptyTimeline),
        other => Err(EncodeError::Unknown(other)),
    }
}

/// Multi-track variant of [`encode_timeline_export`]: composites N tracks of segments via an
/// avfilter `overlay` chain. Track 0 drives the output (its segments play in order, audio comes
/// from track 0 only). Tracks 1..n_tracks-1 are overlaid on top wherever their
/// `timeline_start_secs`-based windows overlap with the current track-0 frame's timeline
/// position.
///
/// When `n_tracks == 1`, delegates to [`encode_timeline_export`] unchanged — this is the
/// guaranteed backward-compatible path that exercises no new C code. With multiple tracks, all
/// active layers are composited in ascending track order, so later tracks appear above earlier
/// ones.
/// Higher-level callers that need every track's audio use [`mix_audio_timeline`] after this
/// render and [`mux_video_audio`] to replace the background-only stream without re-encoding
/// video; this lower-level compositor intentionally retains its original track-0 contract.
///
/// Same cancellation and progress contract as [`encode_timeline_export`].
pub fn encode_timeline_export_multi<F: FnMut(f64)>(
    tracks: &[Vec<ClipSegment>],
    canvas: Canvas,
    out_path: &Path,
    target_lufs: f32,
    gpu_encoder: GpuEncoderPreference,
    cancel: &AtomicBool,
    mut on_progress: F,
) -> Result<EncodeOutcome, EncodeError> {
    if tracks.is_empty() || tracks[0].is_empty() {
        return Err(EncodeError::EmptyTimeline);
    }
    if tracks.len() == 1 {
        return encode_timeline_export(
            &tracks[0],
            canvas,
            out_path,
            target_lufs,
            gpu_encoder,
            cancel,
            on_progress,
        );
    }

    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(EncodeError::InvalidPath)?;

    // Build CString storage and RawClipSegment vecs for each track.
    let mut per_track_paths: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_filts: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_pos_x: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_pos_y: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_mask_paths: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_blend_modes: Vec<Vec<CString>> = Vec::with_capacity(tracks.len());
    let mut per_track_raw: Vec<Vec<RawClipSegment>> = Vec::with_capacity(tracks.len());
    for segs in tracks {
        let mut paths = Vec::with_capacity(segs.len());
        let mut filts = Vec::with_capacity(segs.len());
        let mut pos_xs = Vec::with_capacity(segs.len());
        let mut pos_ys = Vec::with_capacity(segs.len());
        let mut mask_paths = Vec::with_capacity(segs.len());
        let mut blend_modes = Vec::with_capacity(segs.len());
        for seg in segs {
            paths.push(
                CString::new(seg.source_path.to_string_lossy().as_bytes())
                    .map_err(EncodeError::InvalidPath)?,
            );
            filts
                .push(CString::new(seg.video_filter.as_bytes()).map_err(EncodeError::InvalidPath)?);
            pos_xs.push(
                CString::new(seg.position_x_expr.as_bytes()).map_err(EncodeError::InvalidPath)?,
            );
            pos_ys.push(
                CString::new(seg.position_y_expr.as_bytes()).map_err(EncodeError::InvalidPath)?,
            );
            mask_paths.push(
                CString::new(seg.mask_video_path.as_bytes()).map_err(EncodeError::InvalidPath)?,
            );
            blend_modes
                .push(CString::new(seg.blend_mode.as_bytes()).map_err(EncodeError::InvalidPath)?);
        }
        let raw: Vec<RawClipSegment> = segs
            .iter()
            .zip(paths.iter())
            .zip(filts.iter())
            .zip(pos_xs.iter())
            .zip(pos_ys.iter())
            .zip(mask_paths.iter())
            .zip(blend_modes.iter())
            .map(
                |((((((seg, path), filt), pos_x), pos_y), mask_path), blend_mode)| RawClipSegment {
                    source_path: path.as_ptr(),
                    source_in_secs: seg.source_in_secs,
                    source_out_secs: seg.source_out_secs,
                    gain_db: seg.gain_db,
                    video_filter: filt.as_ptr(),
                    frozen: seg.frozen as c_int,
                    speed_factor: seg.speed_factor,
                    smooth_speed_ramp_end_factor: seg.smooth_speed_ramp_end_factor,
                    position_x_expr: pos_x.as_ptr(),
                    position_y_expr: pos_y.as_ptr(),
                    transition_in: seg.transition_in as c_int,
                    transition_duration_secs: seg.transition_duration_secs,
                    timeline_start_secs: seg.timeline_start_secs,
                    mask_video_path: mask_path.as_ptr(),
                    blend_mode: blend_mode.as_ptr(),
                    anchor_x: seg.anchor_x as f64,
                    anchor_y: seg.anchor_y as f64,
                },
            )
            .collect();
        per_track_paths.push(paths);
        per_track_filts.push(filts);
        per_track_pos_x.push(pos_xs);
        per_track_pos_y.push(pos_ys);
        per_track_mask_paths.push(mask_paths);
        per_track_blend_modes.push(blend_modes);
        per_track_raw.push(raw);
    }

    // Build pointer + count arrays for the C call.
    let track_ptrs: Vec<*const RawClipSegment> = per_track_raw.iter().map(|v| v.as_ptr()).collect();
    let track_counts: Vec<c_int> = per_track_raw.iter().map(|v| v.len() as c_int).collect();

    // SAFETY: all pointer-backing storage (per_track_paths, per_track_filts, per_track_mask_paths,
    // per_track_blend_modes, per_track_raw, track_ptrs, track_counts, c_out) is held alive until
    // after avbridge_encode_timeline_export_multi returns. The function does not retain any
    // pointer after returning. cancel is an AtomicBool aligned to at least 1 byte; its value is
    // read atomically by the C side between frames.
    let status = unsafe {
        avbridge_encode_timeline_export_multi(
            track_ptrs.as_ptr(),
            track_counts.as_ptr(),
            tracks.len() as c_int,
            canvas.width as c_int,
            canvas.height as c_int,
            canvas.fps_num as c_int,
            canvas.fps_den as c_int,
            canvas.bit_rate_bps as c_longlong,
            c_out.as_ptr(),
            target_lufs,
            gpu_encoder.as_c_int(),
            Some(progress_trampoline::<F>),
            &mut on_progress as *mut _ as *mut c_void,
            cancel.as_ptr() as *const u8,
        )
    };

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
        14 => Err(EncodeError::NoVideoStream),
        15 => Err(EncodeError::AudioFormatMismatch),
        16 => Err(EncodeError::EmptyTimeline),
        other => Err(EncodeError::Unknown(other)),
    }
}

/// Result of rendering a standalone multi-track audio mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioMixOutcome {
    Completed,
    Cancelled,
}

#[derive(Debug, thiserror::Error)]
pub enum AudioMixError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open an audio source")]
    OpenInput,
    #[error("failed to allocate the audio output")]
    AllocOutput,
    #[error("no timeline segment contains audio")]
    NoAudio,
    #[error("failed to build the audio mixing graph")]
    FilterGraph,
    #[error("failed to open the AAC encoder")]
    Encoder,
    #[error("failed to open the audio output")]
    OpenOutput,
    #[error("failed to write the audio header")]
    WriteHeader,
    #[error("audio mixing failed mid-stream")]
    Pipeline,
    #[error("unknown audio mix status code: {0}")]
    Unknown(c_int),
}

/// Mixes `segments` into one AAC stream. Inputs without an audio stream are skipped by the C
/// bridge, allowing silent video overlays to coexist with actual audio tracks.
pub fn mix_audio_timeline(
    segments: &[AudioSegment],
    timeline_duration_secs: f64,
    out_path: &Path,
    target_lufs: f32,
    cancel: &AtomicBool,
) -> Result<AudioMixOutcome, AudioMixError> {
    if segments.is_empty() {
        return Err(AudioMixError::NoAudio);
    }
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(AudioMixError::InvalidPath)?;
    let paths: Vec<CString> = segments
        .iter()
        .map(|seg| {
            CString::new(seg.source_path.to_string_lossy().as_bytes())
                .map_err(AudioMixError::InvalidPath)
        })
        .collect::<Result<_, _>>()?;
    let gain_exprs: Vec<CString> = segments
        .iter()
        .map(|seg| {
            CString::new(seg.gain_keyframe_expr.as_bytes()).map_err(AudioMixError::InvalidPath)
        })
        .collect::<Result<_, _>>()?;
    let raw: Vec<RawAudioSegment> = segments
        .iter()
        .zip(&paths)
        .zip(&gain_exprs)
        .map(|((seg, path), gain_expr)| RawAudioSegment {
            source_path: path.as_ptr(),
            source_in_secs: seg.source_in_secs,
            source_out_secs: seg.source_out_secs,
            timeline_start_secs: seg.timeline_start_secs,
            gain_db: seg.gain_db,
            speed_factor: seg.speed_factor,
            gain_keyframe_expr: gain_expr.as_ptr(),
            duck_role: seg.duck_role as c_int,
            voice_cleanup_enabled: seg.voice_cleanup_enabled as c_int,
            voice_cleanup_noise_floor_db: seg.voice_cleanup_noise_floor_db,
            voice_cleanup_compressor_threshold_db: seg.voice_cleanup_compressor_threshold_db,
            voice_cleanup_compressor_ratio: seg.voice_cleanup_compressor_ratio,
            voice_cleanup_ceiling_linear: seg.voice_cleanup_ceiling_linear,
        })
        .collect();

    // SAFETY: raw and every CString backing its pointers remain alive for the whole call; the C
    // function does not retain them. AtomicBool has a byte-addressable 0/1 representation, the
    // same cancellation contract used by the encode functions above.
    let status = unsafe {
        avbridge_mix_audio_timeline(
            raw.as_ptr(),
            raw.len() as c_int,
            timeline_duration_secs,
            c_out.as_ptr(),
            target_lufs,
            cancel.as_ptr() as *const u8,
        )
    };
    match status {
        0 => Ok(AudioMixOutcome::Completed),
        1 => Err(AudioMixError::OpenInput),
        2 => Err(AudioMixError::AllocOutput),
        3 => Err(AudioMixError::NoAudio),
        4 => Err(AudioMixError::FilterGraph),
        5 => Err(AudioMixError::Encoder),
        6 => Err(AudioMixError::OpenOutput),
        7 => Err(AudioMixError::WriteHeader),
        8 => Err(AudioMixError::Pipeline),
        9 => Ok(AudioMixOutcome::Cancelled),
        other => Err(AudioMixError::Unknown(other)),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MediaMuxError {
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open a mux input")]
    OpenInput,
    #[error("failed to allocate the mux output")]
    AllocOutput,
    #[error("video or audio input is missing its required stream")]
    MissingStream,
    #[error("failed to create a mux output stream")]
    NewStream,
    #[error("failed to open the mux output")]
    OpenOutput,
    #[error("failed to write the mux header")]
    WriteHeader,
    #[error("failed while stream-copying mux packets")]
    WriteFrame,
    #[error("unknown media mux status code: {0}")]
    Unknown(c_int),
}

/// Stream-copies the first video stream from `video_path` and first audio stream from
/// `audio_path` into `out_path`.
pub fn mux_video_audio(
    video_path: &Path,
    audio_path: &Path,
    out_path: &Path,
) -> Result<(), MediaMuxError> {
    let c_video = CString::new(video_path.to_string_lossy().as_bytes())
        .map_err(MediaMuxError::InvalidPath)?;
    let c_audio = CString::new(audio_path.to_string_lossy().as_bytes())
        .map_err(MediaMuxError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(MediaMuxError::InvalidPath)?;
    // SAFETY: all three CStrings stay alive for the duration of the synchronous call.
    let status =
        unsafe { avbridge_mux_video_audio(c_video.as_ptr(), c_audio.as_ptr(), c_out.as_ptr()) };
    match status {
        0 => Ok(()),
        1 => Err(MediaMuxError::OpenInput),
        2 => Err(MediaMuxError::AllocOutput),
        3 => Err(MediaMuxError::MissingStream),
        4 => Err(MediaMuxError::NewStream),
        5 => Err(MediaMuxError::OpenOutput),
        6 => Err(MediaMuxError::WriteHeader),
        7 => Err(MediaMuxError::WriteFrame),
        other => Err(MediaMuxError::Unknown(other)),
    }
}

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
/// calls would race these globals, corrupting memory. This mutex enforces the documented
/// "not thread-safe" contract at the Rust API boundary.
static LOUDNESS_MUTEX: Mutex<()> = Mutex::new(());

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
    // can enter this unsafe block concurrently, preventing races on the process-global
    // g_loudness_log_buf/g_loudness_log_cap/g_loudness_log_len variables.
    let status = unsafe {
        avbridge_measure_loudness(c_path.as_ptr(), buf.as_mut_ptr() as *mut c_char, buf.len())
    };

    match status {
        0 => {}
        1 => return Err(LoudnessError::OpenInput),
        2 => return Err(LoudnessError::StreamInfo),
        3 => return Err(LoudnessError::NoAudioStream),
        4 => return Err(LoudnessError::Decoder),
        5 => return Err(LoudnessError::FilterGraph),
        6 => return Err(LoudnessError::Pipeline),
        7 => return Err(LoudnessError::NoReport),
        other => return Err(LoudnessError::Unknown(other)),
    }

    // SAFETY: on success the C side NUL-terminates the report within `buf`.
    let json = unsafe { CStr::from_ptr(buf.as_ptr() as *const c_char) };
    json.to_str()
        .map(str::to_owned)
        .map_err(LoudnessError::InvalidUtf8)
}

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
    let c_in =
        CString::new(in_path.to_string_lossy().as_bytes()).map_err(ProxyError::InvalidPath)?;
    let c_out =
        CString::new(out_path.to_string_lossy().as_bytes()).map_err(ProxyError::InvalidPath)?;

    // SAFETY: c_in/c_out are valid NUL-terminated C strings for the duration of this call.
    // `bridge.c` frees the decoder/encoder/scaler/filter-graph contexts, output I/O, and
    // output context on every exit path.
    let status =
        unsafe { avbridge_generate_proxy(c_in.as_ptr(), c_out.as_ptr(), target_height as c_int) };

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

    match status {
        0 => {}
        1 => return Err(WaveformError::OpenInput),
        2 => return Err(WaveformError::StreamInfo),
        3 => return Err(WaveformError::NoAudioStream),
        4 => return Err(WaveformError::Decoder),
        5 => return Err(WaveformError::FilterGraph),
        6 => return Err(WaveformError::Pipeline),
        other => return Err(WaveformError::Unknown(other)),
    }

    Ok(mins.into_iter().zip(maxs).collect())
}

/// One pre-rasterized text overlay to composite over an exported video. The core crate writes
/// its semantic text segments to temporary full-canvas RGBA PNGs immediately before this pass,
/// keeping font/background rendering identical to preview and temporary paths out of persisted
/// export jobs.
#[derive(Debug, Clone)]
pub struct TextOverlaySegment {
    /// When the overlay becomes visible, in seconds from the start of the exported video.
    pub start_secs: f64,
    /// How long the overlay is visible, in seconds.
    pub duration_secs: f64,
    /// Full-canvas transparent PNG containing the already-rasterized text/background.
    pub overlay_path: PathBuf,
    /// Complete `geq`-expression-language fragment (built by
    /// `avcore::keyframe::text_opacity_alpha_expr`) multiplied against the PNG's own alpha
    /// channel — empty means "always fully opaque", the same visibility this overlay always had
    /// before opacity keyframes existed.
    pub opacity_keyframe_expr: String,
    /// Complete `overlay`-expression-language fragments (built by
    /// `avcore::keyframe::text_position_offset_expr`) added to the `overlay` filter's `x`/`y`
    /// position — empty means "no offset", the same `x=0:y=0` placement this overlay always had
    /// before position keyframes existed.
    pub position_keyframe_expr_x: String,
    pub position_keyframe_expr_y: String,
    /// Complete `geq`-expression-language fragments (built by
    /// `avcore::keyframe::text_scale_sample_exprs`) used as the (X,Y) coordinates a geq stage
    /// samples the raster's own pixels at, remapping it around its own baked position anchor —
    /// empty (either one) means "no remap", the same pixel-for-pixel raster this overlay always
    /// had before scale keyframes existed. An inverse-sample zoom, not a literal `scale` filter
    /// — see [`apply_text_overlays`]'s doc comment for why.
    pub scale_keyframe_expr_x: String,
    pub scale_keyframe_expr_y: String,
    /// Complete `geq`-expression-language fragments (built by
    /// `avcore::keyframe::text_rotation_sample_exprs`) used as the (X,Y) coordinates a geq stage
    /// samples the raster's own pixels at, remapping it around its own baked position anchor —
    /// empty (either one) means "no remap", the same pixel-for-pixel raster this overlay always
    /// had before rotation keyframes existed.
    pub rotation_keyframe_expr_x: String,
    pub rotation_keyframe_expr_y: String,
}

/// What [`apply_text_overlays`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum TextOverlayError {
    /// A path contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to allocate output context")]
    AllocOutput,
    #[error("failed to build the text-overlay filter graph")]
    FilterGraph,
    #[error("decode/filter/encode pipeline failed mid-stream")]
    Pipeline,
    #[error("unknown text overlay status code: {0}")]
    Unknown(c_int),
}

/// Opens `in_path` (the already-rendered export), overlays each segment's full-canvas RGBA PNG
/// during its timeline window, and writes the result to `out_path`. The caller is responsible
/// for renaming `out_path` over `in_path` when this returns `Ok(())`.
///
/// `canvas_width`/`canvas_height`/`fps_num`/`fps_den` must match the rendered file's canvas
/// dimensions and frame rate so the output encoder matches the existing render.
///
/// A no-op (returns `Ok(())` immediately without calling the C function) when `segments` is
/// empty — the caller can skip creating `out_path` in that case.
///
/// `TextOverlaySegment::scale_keyframe_expr_x`/`_y` drives an inverse-sample geq remap rather
/// than a literal `scale` filter with `eval=frame`: letting `scale` renegotiate its output
/// frame size every frame reliably corrupted the heap in a real export elsewhere in this
/// codebase (`timeline_export.c`'s zoom-transition case, see `CLAUDE.md`) — the geq approach
/// keeps the output frame size fixed and only changes what each pixel samples, avoiding that
/// failure mode entirely.
pub fn apply_text_overlays(
    in_path: &Path,
    out_path: &Path,
    segments: &[TextOverlaySegment],
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), TextOverlayError> {
    if segments.is_empty() {
        return Ok(());
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;

    let mut c_paths: Vec<CString> = Vec::with_capacity(segments.len());
    for seg in segments {
        c_paths.push(
            CString::new(seg.overlay_path.to_string_lossy().as_bytes())
                .map_err(TextOverlayError::InvalidPath)?,
        );
    }
    // One CString each for opacity/position-x/position-y/scale-x/scale-y/rotation-x/rotation-y
    // per segment, held in one Vec of tuples (rather than seven parallel Vecs zipped together)
    // so building raw_segments below stays readable as the field count grows.
    let exprs: Vec<(
        CString,
        CString,
        CString,
        CString,
        CString,
        CString,
        CString,
    )> = segments
        .iter()
        .map(|seg| {
            Ok::<_, TextOverlayError>((
                CString::new(seg.opacity_keyframe_expr.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.position_keyframe_expr_x.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.position_keyframe_expr_y.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.scale_keyframe_expr_x.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.scale_keyframe_expr_y.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.rotation_keyframe_expr_x.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
                CString::new(seg.rotation_keyframe_expr_y.as_bytes())
                    .map_err(TextOverlayError::InvalidPath)?,
            ))
        })
        .collect::<Result<_, _>>()?;

    let raw_segments: Vec<RawTextSegment> = segments
        .iter()
        .zip(c_paths.iter())
        .zip(exprs.iter())
        .map(
            |((seg, path), (opacity_expr, pos_x, pos_y, scale_x, scale_y, rot_x, rot_y))| {
                RawTextSegment {
                    start_secs: seg.start_secs,
                    duration_secs: seg.duration_secs,
                    overlay_path: path.as_ptr(),
                    opacity_keyframe_expr: opacity_expr.as_ptr(),
                    position_keyframe_expr_x: pos_x.as_ptr(),
                    position_keyframe_expr_y: pos_y.as_ptr(),
                    scale_keyframe_expr_x: scale_x.as_ptr(),
                    scale_keyframe_expr_y: scale_y.as_ptr(),
                    rotation_keyframe_expr_x: rot_x.as_ptr(),
                    rotation_keyframe_expr_y: rot_y.as_ptr(),
                }
            },
        )
        .collect();

    // SAFETY: all pointers (c_in, c_out, raw_segments' path/expr pointers from c_paths/exprs)
    // are valid NUL-terminated C strings held alive for the full duration of this call.
    // raw_segments is a contiguous Vec<RawTextSegment> with segment_count entries, never
    // mutated during the call.
    let status = unsafe {
        avbridge_apply_text_overlays(
            c_in.as_ptr(),
            c_out.as_ptr(),
            raw_segments.as_ptr(),
            raw_segments.len() as c_int,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    match status {
        0 => Ok(()),
        1 => Err(TextOverlayError::OpenInput),
        2 => Err(TextOverlayError::AllocOutput),
        3 => Err(TextOverlayError::FilterGraph),
        4 => Err(TextOverlayError::Pipeline),
        other => Err(TextOverlayError::Unknown(other)),
    }
}

/// One geometric shape to composite over an exported video via `avbridge_apply_shape_overlays`.
/// `filter_desc` is a complete, ready-to-chain `geq=...` avfilter node description — see
/// `avcore::shape_render::build_shape_filter_desc`, which builds it (the actual shape geometry/
/// color/rotation math lives there, not here or in the C function this crosses into).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShapeSegment {
    pub filter_desc: String,
}

/// Same shape as [`apply_text_overlays`] (same [`TextOverlayError`] variants, same "opens
/// `in_path`, writes the composited result to `out_path`" contract), but chains each segment's
/// pre-built `geq` filter node instead of building a `drawtext` chain. A no-op (`Ok(())`
/// immediately) when `segments` is empty.
pub fn apply_shape_overlays(
    in_path: &Path,
    out_path: &Path,
    segments: &[ShapeSegment],
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), TextOverlayError> {
    if segments.is_empty() {
        return Ok(());
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(TextOverlayError::InvalidPath)?;

    let mut c_descs: Vec<CString> = Vec::with_capacity(segments.len());
    for seg in segments {
        c_descs
            .push(CString::new(seg.filter_desc.as_bytes()).map_err(TextOverlayError::InvalidPath)?);
    }

    let raw_segments: Vec<RawShapeSegment> = c_descs
        .iter()
        .map(|desc| RawShapeSegment {
            filter_desc: desc.as_ptr(),
        })
        .collect();

    // SAFETY: same reasoning as apply_text_overlays — c_in/c_out/raw_segments' filter_desc
    // pointers (from c_descs) are valid NUL-terminated C strings held alive for the full call.
    let status = unsafe {
        avbridge_apply_shape_overlays(
            c_in.as_ptr(),
            c_out.as_ptr(),
            raw_segments.as_ptr(),
            raw_segments.len() as c_int,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    match status {
        0 => Ok(()),
        1 => Err(TextOverlayError::OpenInput),
        2 => Err(TextOverlayError::AllocOutput),
        3 => Err(TextOverlayError::FilterGraph),
        4 => Err(TextOverlayError::Pipeline),
        other => Err(TextOverlayError::Unknown(other)),
    }
}

/// What [`apply_privacy_blur`] failed on.
#[derive(Debug, thiserror::Error)]
pub enum PrivacyBlurError {
    /// A path contains a NUL byte and can't be handed to the C API.
    #[error("path is not a valid C string: {0}")]
    InvalidPath(NulError),
    #[error("failed to open input")]
    OpenInput,
    #[error("failed to allocate output context")]
    AllocOutput,
    #[error("failed to build the blur/mask filter graph")]
    FilterGraph,
    #[error("decode/filter/encode pipeline failed mid-stream")]
    Pipeline,
    /// `blur_sigma` was <= 0.0 — there is no meaningful "zero blur" for `gblur`; a caller
    /// wanting the frame untouched should skip calling this function entirely.
    #[error("blur_sigma must be positive")]
    NonPositiveSigma,
    #[error("unknown privacy-blur status code: {0}")]
    Unknown(c_int),
}

/// Opens `in_path` (an already-rendered export, same "post-process pass" contract
/// [`apply_text_overlays`]/[`apply_shape_overlays`] use), blurs (Gaussian, `blur_sigma`)
/// everywhere `matte_path`'s own per-frame luma is non-zero, and writes the result to
/// `out_path`. The caller is responsible for renaming `out_path` over `in_path` when this
/// returns `Ok(())`, same as the other two overlay functions.
///
/// `matte_path` is expected to be exactly [`encode_matte_video`]'s own output shape
/// (grayscale-as-luma, neutral chroma) — CF-09's arbitrary-object privacy-blur use case builds
/// one from `avcore::mask_propagation::rasterize_to_matte_frames`, but any matte video of that
/// shape works. Its own frame rate/duration must already match `canvas_fps_num`/`_den` — this
/// function does no timing reconciliation between the main video and the matte.
///
/// Fails with [`PrivacyBlurError::NonPositiveSigma`] immediately (without touching `out_path`)
/// if `blur_sigma <= 0.0`.
#[allow(clippy::too_many_arguments)]
pub fn apply_privacy_blur(
    in_path: &Path,
    out_path: &Path,
    matte_path: &Path,
    blur_sigma: f64,
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
) -> Result<(), PrivacyBlurError> {
    if blur_sigma <= 0.0 {
        return Err(PrivacyBlurError::NonPositiveSigma);
    }

    let c_in = CString::new(in_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;
    let c_out = CString::new(out_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;
    let c_matte = CString::new(matte_path.to_string_lossy().as_bytes())
        .map_err(PrivacyBlurError::InvalidPath)?;

    // SAFETY: c_in/c_out/c_matte are valid NUL-terminated C strings held alive for the full
    // call. privacy_blur.c frees the decoder/encoder contexts, filter graph, and output I/O on
    // every exit path.
    let status = unsafe {
        avbridge_apply_privacy_blur(
            c_in.as_ptr(),
            c_out.as_ptr(),
            c_matte.as_ptr(),
            blur_sigma,
            canvas_width as c_int,
            canvas_height as c_int,
            fps_num as c_int,
            fps_den as c_int,
        )
    };

    match status {
        0 => Ok(()),
        1 => Err(PrivacyBlurError::OpenInput),
        2 => Err(PrivacyBlurError::AllocOutput),
        3 => Err(PrivacyBlurError::FilterGraph),
        4 => Err(PrivacyBlurError::Pipeline),
        other => Err(PrivacyBlurError::Unknown(other)),
    }
}

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

    match status {
        0 => {
            // SAFETY: on OK, out_samples points to a malloc'd buffer of out_sample_count valid
            // f32s (or is null with out_sample_count == 0 for a zero-length result) — copied
            // into a Vec, then the C-owned buffer is freed via the paired allocator.
            let samples = if out_sample_count > 0 {
                unsafe {
                    std::slice::from_raw_parts(out_samples, out_sample_count as usize).to_vec()
                }
            } else {
                Vec::new()
            };
            unsafe { avbridge_free_pcm_buffer(out_samples) };
            Ok(samples)
        }
        1 => Err(PcmError::OpenInput),
        2 => Err(PcmError::StreamInfo),
        3 => Err(PcmError::NoAudioStream),
        4 => Err(PcmError::Decoder),
        5 => Err(PcmError::FilterGraph),
        6 => Err(PcmError::Pipeline),
        other => Err(PcmError::Unknown(other)),
    }
}
