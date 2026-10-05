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

use std::ffi::{c_void, CString};
use std::os::raw::{c_int, c_longlong};
use std::path::Path;
use std::sync::atomic::AtomicBool;

use crate::export::{EncodeError, EncodeOutcome};
use crate::ffi::{
    avbridge_encode_timeline_export, avbridge_encode_timeline_export_multi, progress_trampoline,
    RawClipSegment,
};

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
    /// Fade from black (video) and from silence (audio) over this many seconds at the start of
    /// the clip; `0.0` = none. Output seconds, after any speed change.
    #[serde(default)]
    pub fade_in_secs: f32,
    /// Fade to black and silence over this many seconds at the end of the clip.
    #[serde(default)]
    pub fade_out_secs: f32,
}

pub(crate) fn default_anchor() -> f32 {
    0.5
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
    pub(crate) fn as_c_int(self) -> c_int {
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
                fade_in_secs: seg.fade_in_secs,
                fade_out_secs: seg.fade_out_secs,
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

    encode_timeline_export_status(status)
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
                    fade_in_secs: seg.fade_in_secs,
                    fade_out_secs: seg.fade_out_secs,
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

    encode_timeline_export_multi_status(status)
}

/// Maps the status code the C side returned from `encode_timeline_export` to its result.
pub(crate) fn encode_timeline_export_status(status: c_int) -> Result<EncodeOutcome, EncodeError> {
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

/// Maps the status code the C side returned from `encode_timeline_export_multi` to its result.
pub(crate) fn encode_timeline_export_multi_status(
    status: c_int,
) -> Result<EncodeOutcome, EncodeError> {
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
