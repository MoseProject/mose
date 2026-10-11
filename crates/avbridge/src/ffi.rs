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

use std::ffi::c_void;
use std::os::raw::{c_char, c_int, c_longlong};

#[repr(C)]
pub(crate) struct RawClipSegment {
    pub(crate) source_path: *const c_char,
    pub(crate) source_in_secs: f64,
    pub(crate) source_out_secs: f64,
    pub(crate) gain_db: f32,
    pub(crate) video_filter: *const c_char,
    pub(crate) frozen: c_int,
    pub(crate) speed_factor: f32,
    /// `0.0` (or negative) means no ramp — see `ClipSegment::smooth_speed_ramp_end_factor`'s
    /// doc comment in `bridge.h`.
    pub(crate) smooth_speed_ramp_end_factor: f32,
    pub(crate) position_x_expr: *const c_char,
    pub(crate) position_y_expr: *const c_char,
    pub(crate) transition_in: c_int,
    pub(crate) transition_duration_secs: f32,
    /// Start position of this clip on the shared timeline in seconds. Used by
    /// `avbridge_encode_timeline_export_multi` to determine which overlay tracks are active at
    /// any given decoded-frame time. Ignored by `avbridge_encode_timeline_export`.
    pub(crate) timeline_start_secs: f64,
    /// Path to a grayscale-as-luma alpha-matte video, or an empty string for none — see
    /// `ClipSegment::mask_video_path`'s doc comment in `bridge.h`.
    pub(crate) mask_video_path: *const c_char,
    /// FFmpeg `blend` filter mode name (`blend=all_mode=<name>`), or an empty string for
    /// "no blend mode, use plain `overlay` compositing" — see `ClipSegment::blend_mode`'s doc
    /// comment in `bridge.h`.
    pub(crate) blend_mode: *const c_char,
    /// Fraction (`0.0..=1.0`) of this clip's own decoded frame that rotation/scale pivot
    /// around — see `ClipSegment::anchor_x`/`anchor_y`'s doc comment in `bridge.h`.
    pub(crate) anchor_x: f64,
    pub(crate) anchor_y: f64,
    /// See `ClipSegment::fade_in_secs` in `bridge.h`.
    pub(crate) fade_in_secs: f32,
    pub(crate) fade_out_secs: f32,
}

#[repr(C)]
pub(crate) struct RawAudioSegment {
    pub(crate) source_path: *const c_char,
    pub(crate) source_in_secs: f64,
    pub(crate) source_out_secs: f64,
    pub(crate) timeline_start_secs: f64,
    pub(crate) gain_db: f32,
    pub(crate) speed_factor: f32,
    /// NULL or empty means "use gain_db unchanged" — see `AudioSegment::gain_keyframe_expr`.
    pub(crate) gain_keyframe_expr: *const c_char,
    pub(crate) duck_role: c_int,
    /// CF-03 "Gameplay Voice" cleanup — see `AudioSegment::voice_cleanup_enabled`'s doc comment.
    pub(crate) voice_cleanup_enabled: c_int,
    pub(crate) voice_cleanup_noise_floor_db: f32,
    pub(crate) voice_cleanup_compressor_threshold_db: f32,
    pub(crate) voice_cleanup_compressor_ratio: f32,
    pub(crate) voice_cleanup_ceiling_linear: f32,
    pub(crate) fade_in_secs: f32,
    pub(crate) fade_out_secs: f32,
}

/// Mirror of `TextSegment` in `bridge.h` — one pre-rasterized text overlay image.
#[repr(C)]
pub(crate) struct RawTextSegment {
    pub(crate) start_secs: f64,
    pub(crate) duration_secs: f64,
    pub(crate) overlay_path: *const c_char,
    /// NULL or empty means "always fully opaque" — see `TextOverlaySegment::opacity_keyframe_expr`.
    pub(crate) opacity_keyframe_expr: *const c_char,
    /// NULL or empty means "no offset" — see `TextOverlaySegment::position_keyframe_expr_x`/`_y`.
    pub(crate) position_keyframe_expr_x: *const c_char,
    pub(crate) position_keyframe_expr_y: *const c_char,
    /// NULL, empty, or either-one-empty means "no remap" — see
    /// `TextOverlaySegment::scale_keyframe_expr_x`/`_y`.
    pub(crate) scale_keyframe_expr_x: *const c_char,
    pub(crate) scale_keyframe_expr_y: *const c_char,
    /// NULL, empty, or either-one-empty means "no remap" — see
    /// `TextOverlaySegment::rotation_keyframe_expr_x`/`_y`.
    pub(crate) rotation_keyframe_expr_x: *const c_char,
    pub(crate) rotation_keyframe_expr_y: *const c_char,
}

#[repr(C)]
pub(crate) struct RawShapeSegment {
    pub(crate) filter_desc: *const c_char,
}

#[repr(C)]
pub(crate) struct RawProbeInfo {
    pub(crate) has_video: c_int,
    pub(crate) has_audio: c_int,
    pub(crate) duration_secs: f64,
    pub(crate) codec_name: [c_char; 32],
    pub(crate) bit_rate: c_longlong,
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    pub(crate) fps_num: c_int,
    pub(crate) fps_den: c_int,
    pub(crate) sample_rate_hz: c_int,
}

#[repr(C)]
pub(crate) struct RawDecodeInfo {
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    pub(crate) fps_num: c_int,
    pub(crate) fps_den: c_int,
    pub(crate) duration_secs: f64,
    pub(crate) hardware: c_int,
}

#[repr(C)]
pub(crate) struct RawDecodedRgbaFrame {
    pub(crate) pixels: *mut u8,
    pub(crate) pixel_len: usize,
    pub(crate) width: c_int,
    pub(crate) height: c_int,
    pub(crate) stride_bytes: c_int,
    pub(crate) timestamp_secs: f64,
    pub(crate) borrowed: c_int,
}

#[repr(C)]
pub(crate) struct RawDecodeSession {
    pub(crate) _private: [u8; 0],
}

unsafe extern "C" {
    pub(crate) fn avbridge_version() -> u32;
    pub(crate) fn avbridge_probe(path: *const c_char, out: *mut RawProbeInfo) -> c_int;
    pub(crate) fn avbridge_remux_copy(in_path: *const c_char, out_path: *const c_char) -> c_int;
    pub(crate) fn avbridge_encode_export(
        in_path: *const c_char,
        out_path: *const c_char,
        target_lufs: f32,
        progress_cb: Option<unsafe extern "C" fn(user_data: *mut c_void, seconds: f64)>,
        progress_user_data: *mut c_void,
        cancel: *const u8,
    ) -> c_int;
    pub(crate) fn avbridge_encode_timeline_export_multi(
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
    pub(crate) fn avbridge_mix_audio_timeline(
        segments: *const RawAudioSegment,
        segment_count: c_int,
        timeline_duration_secs: f64,
        out_path: *const c_char,
        target_lufs: f32,
        cancel: *const u8,
    ) -> c_int;
    pub(crate) fn avbridge_mux_video_audio(
        video_path: *const c_char,
        audio_path: *const c_char,
        out_path: *const c_char,
    ) -> c_int;
    pub(crate) fn avbridge_measure_loudness(
        in_path: *const c_char,
        out_json: *mut c_char,
        out_json_len: usize,
    ) -> c_int;
    pub(crate) fn avbridge_generate_proxy(
        in_path: *const c_char,
        out_path: *const c_char,
        target_height: c_int,
    ) -> c_int;
    pub(crate) fn avbridge_generate_waveform(
        in_path: *const c_char,
        bucket_count: c_int,
        out_min: *mut f32,
        out_max: *mut f32,
    ) -> c_int;
    pub(crate) fn avbridge_text_overlay_filter(
        segments: *const RawTextSegment,
        segment_count: c_int,
    ) -> *mut c_char;
    pub(crate) fn avbridge_shape_overlay_filter(
        segments: *const RawShapeSegment,
        segment_count: c_int,
    ) -> *mut c_char;
    pub(crate) fn avbridge_privacy_blur_filter(
        matte_path: *const c_char,
        blur_sigma: f64,
        canvas_width: c_int,
        canvas_height: c_int,
    ) -> *mut c_char;
    pub(crate) fn avbridge_free_string(s: *mut c_char);
    pub(crate) fn avbridge_smart_cut_video(
        in_path: *const c_char,
        in_secs: *const f64,
        out_secs: *const f64,
        range_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
        canvas_bit_rate_bps: c_longlong,
        out_path: *const c_char,
        progress_cb: Option<unsafe extern "C" fn(user_data: *mut c_void, seconds: f64)>,
        progress_user_data: *mut c_void,
        cancel: *const u8,
    ) -> c_int;
    pub(crate) fn avbridge_last_video_encoder() -> *const c_char;
    pub(crate) fn avbridge_encode_timeline_export_with_post(
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
        post_filters: *const *const c_char,
        post_filter_count: c_int,
    ) -> c_int;
    pub(crate) fn avbridge_apply_text_overlays(
        in_path: *const c_char,
        out_path: *const c_char,
        segments: *const RawTextSegment,
        segment_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    pub(crate) fn avbridge_apply_shape_overlays(
        in_path: *const c_char,
        out_path: *const c_char,
        segments: *const RawShapeSegment,
        segment_count: c_int,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    pub(crate) fn avbridge_extract_pcm_16k_mono(
        in_path: *const c_char,
        out_samples: *mut *mut f32,
        out_sample_count: *mut i64,
    ) -> c_int;
    pub(crate) fn avbridge_free_pcm_buffer(samples: *mut f32);
    pub(crate) fn avbridge_encode_matte_video(
        luma_frames: *const u8,
        frame_count: c_int,
        width: c_int,
        height: c_int,
        fps_num: c_int,
        fps_den: c_int,
        out_path: *const c_char,
    ) -> c_int;
    pub(crate) fn avbridge_apply_privacy_blur(
        in_path: *const c_char,
        out_path: *const c_char,
        matte_path: *const c_char,
        blur_sigma: f64,
        canvas_width: c_int,
        canvas_height: c_int,
        canvas_fps_num: c_int,
        canvas_fps_den: c_int,
    ) -> c_int;
    pub(crate) fn avbridge_decode_open_ex(
        path: *const c_char,
        hardware: c_int,
        out_session: *mut *mut RawDecodeSession,
        out_info: *mut RawDecodeInfo,
    ) -> c_int;
    pub(crate) fn avbridge_decode_seek(
        session: *mut RawDecodeSession,
        timestamp_secs: f64,
    ) -> c_int;
    pub(crate) fn avbridge_decode_next_rgba(
        session: *mut RawDecodeSession,
        cancel: *const u8,
        out_frame: *mut RawDecodedRgbaFrame,
    ) -> c_int;
    pub(crate) fn avbridge_decode_advance(
        session: *mut RawDecodeSession,
        cancel: *const u8,
        out_timestamp_secs: *mut f64,
    ) -> c_int;
    pub(crate) fn avbridge_decode_has_held(session: *mut RawDecodeSession) -> c_int;
    pub(crate) fn avbridge_decode_take_rgba(
        session: *mut RawDecodeSession,
        newest: c_int,
        out_frame: *mut RawDecodedRgbaFrame,
    ) -> c_int;
    pub(crate) fn avbridge_decode_set_output_size(
        session: *mut RawDecodeSession,
        width: c_int,
        height: c_int,
    );
    pub(crate) fn avbridge_decode_free_frame(frame: *mut RawDecodedRgbaFrame);
    pub(crate) fn avbridge_decode_close(session: *mut RawDecodeSession);
    pub(crate) fn avbridge_hw_device_cache_clear();
}

/// Trampoline handed to the C side as `progress_cb`; `user_data` is a `*mut F` for whatever
/// closure [`encode_export`] was called with.
pub(crate) unsafe extern "C" fn progress_trampoline<F: FnMut(f64)>(
    user_data: *mut c_void,
    seconds: f64,
) {
    // SAFETY: `user_data` was set to `&mut on_progress` for the duration of the single
    // `avbridge_encode_export` call this trampoline is only ever invoked from, and the
    // pointee outlives that call (it's a stack local in `encode_export`, held for the call).
    let closure = unsafe { &mut *(user_data as *mut F) };
    closure(seconds);
}
