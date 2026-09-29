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

//! Renders a normalized export from a source file — Fase 4's render step. Applies loudness
//! normalization (`loudnorm` + a true-peak safety limiter, per Fase 2) via `oca-avbridge`'s
//! encode FFI while copying video untouched, which is what makes the export's bitrate equal
//! the source's bitrate exactly, one of this editor's two headline differentiators.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use avbridge::Canvas;

pub use crate::export::ExportAspectRatio;
use crate::keyframe;
use crate::media::MediaAsset;
use crate::project::Sequence;
use crate::timeline::{
    ClipInstance, ShapeClip, TextClip, TextFontFamily, TextFontStyle, TrackKind,
};

/// Overrides `canvas` width/height to match `ratio`, keeping fps and bitrate unchanged.
/// Returns `canvas` unmodified when `ratio` is [`ExportAspectRatio::Original`].
pub fn apply_export_aspect_ratio(canvas: Canvas, ratio: ExportAspectRatio) -> Canvas {
    if ratio == ExportAspectRatio::Original {
        return canvas;
    }
    let (width, height) = ratio.dims_or((canvas.width, canvas.height));
    Canvas {
        width,
        height,
        ..canvas
    }
}

/// One resolved text overlay saved in an export job. Unlike avbridge's raw image-overlay FFI
/// input, this remains semantic text plus styling so a persisted queue does not depend on
/// temporary PNG files. [`apply_text_overlay_pass`] rasterizes it only when the job runs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TextSegment {
    pub start_secs: f64,
    pub duration_secs: f64,
    pub text: String,
    pub font_size: f32,
    #[serde(default)]
    pub font_family: TextFontFamily,
    #[serde(default)]
    pub font_style: TextFontStyle,
    /// See [`crate::timeline::TextClip::font_weight`]'s own doc comment.
    #[serde(default)]
    pub font_weight: Option<u16>,
    pub color_rgba: [u8; 4],
    #[serde(default)]
    pub background_rgba: [u8; 4],
    #[serde(default)]
    pub background_padding: f32,
    #[serde(default)]
    pub background_corner_radius: f32,
    /// Optional UTF-8 byte range within `text` to rasterize. Word-highlight segments keep the
    /// full caption here and select just one word, so they inherit the base caption's exact
    /// fontdue wrapping and line position. `None` renders the whole string.
    #[serde(default)]
    pub glyph_byte_range: Option<[u32; 2]>,
    pub pos_x: f32,
    pub pos_y: f32,
    /// Pre-built `geq` alpha-multiplier expression for this clip's `opacity_keyframes`
    /// (`keyframe::text_opacity_alpha_expr`), built once from the *base clip's* own
    /// `start_secs`/`duration_secs` in [`text_clip_to_segments`] and copied onto every segment
    /// derived from that clip (base plus any per-word highlight segments) — the expression is
    /// self-contained/timeline-absolute, so it stays correct even on a word-highlight segment
    /// whose own `start_secs`/`duration_secs` (used only for its `enable=between(...)`
    /// visibility window) differ from the base clip's. Empty means "no fade".
    #[serde(default)]
    pub opacity_keyframe_expr: String,
    /// Pre-built `overlay` `x`/`y` pixel-offset expressions for this clip's
    /// `pos_x_keyframes`/`pos_y_keyframes` (`keyframe::text_position_offset_expr`), built once
    /// from the *base clip's* own `start_secs`/`duration_secs` in [`text_clip_to_segments`] and
    /// copied onto every segment derived from that clip, same "self-contained/timeline-absolute,
    /// stays correct on a word-highlight segment too" reasoning as `opacity_keyframe_expr`.
    /// Empty means "no offset" (`overlay=x=0:y=0`, same as before this field existed).
    #[serde(default)]
    pub position_keyframe_expr_x: String,
    #[serde(default)]
    pub position_keyframe_expr_y: String,
    /// Pre-built `geq` inverse-sample coordinate expressions for this clip's `scale_keyframes`
    /// (`keyframe::text_scale_sample_exprs`), same "built once from the base clip, copied onto
    /// every derived segment" treatment as `position_keyframe_expr_x`/`_y`. Empty (either one)
    /// means "no remap" (the raster's own unscaled pixels, same as before this field existed).
    #[serde(default)]
    pub scale_keyframe_expr_x: String,
    #[serde(default)]
    pub scale_keyframe_expr_y: String,
    /// Pre-built `geq` inverse-sample coordinate expressions for this clip's `rotation_keyframes`
    /// (`keyframe::text_rotation_sample_exprs`), same "built once from the base clip, copied onto
    /// every derived segment" treatment as `scale_keyframe_expr_x`/`_y`. Empty (either one) means
    /// "no remap" (the raster's own unrotated pixels, same as before this field existed).
    #[serde(default)]
    pub rotation_keyframe_expr_x: String,
    #[serde(default)]
    pub rotation_keyframe_expr_y: String,
    /// Paragraph base-direction override — see [`crate::timeline::TextDirection`].
    /// `#[serde(default)]` so an already-queued export job (persisted `.ocqueue` bytes) loads as
    /// `Auto`, the exact behavior every segment already had before this field existed.
    #[serde(default)]
    pub direction: crate::timeline::TextDirection,
    /// Horizontal text alignment — see [`crate::timeline::TextAlign`]. `#[serde(default)]` so an
    /// already-queued export job loads as `Auto`, the exact behavior every segment already had
    /// before this field existed.
    #[serde(default)]
    pub text_align: crate::timeline::TextAlign,
}

#[derive(Debug)]
pub enum RenderError {
    OpenInput,
    StreamInfo,
    AllocOutput,
    NewStream,
    OpenOutput,
    WriteHeader,
    WriteFrame,
    /// `source` has no audio stream to normalize.
    NoAudioStream,
    Decoder,
    FilterGraph,
    Encoder,
    /// A decode/filter/encode call failed mid-render (not at setup).
    Pipeline,
    /// The FFI bridge returned a status this crate doesn't know about.
    Unknown(i32),
    /// (`render_timeline_export` only) a segment has no video stream.
    NoVideoStream,
    /// (`render_timeline_export` only) a later clip's audio format doesn't match the first
    /// clip's — see `avbridge::EncodeError::AudioFormatMismatch`.
    AudioFormatMismatch,
    /// (`render_timeline_export` only) the sequence's video track is missing or empty.
    EmptyTimeline,
    /// (`render_timeline_export` only) a clip's `asset_id` isn't in the project's media
    /// library.
    MissingAsset,
    /// A caller supplied a canvas with no renderable dimensions or frame rate.
    InvalidCanvas,
    /// A caller supplied a timeline segment with an invalid source range or render parameter.
    InvalidTimelineSegment,
    /// Loudness targets must be finite before crossing the native encoder boundary.
    InvalidTargetLufs,
    /// The native multi-track audio mixing pass failed after the video render completed.
    AudioMix(String),
    /// Stream-copying the rendered video together with the mixed audio failed.
    MediaMux(String),
    /// Replacing the rendered file with a completed post-processing result failed.
    ReplaceOutput(std::io::Error),
    /// A clip's [`crate::timeline::ClipInstance::nested_sequence_id`] doesn't resolve to any
    /// [`crate::project::Sequence`] in the project.
    MissingNestedSequence,
    /// A nested-sequence clip (transitively) contains a clip nesting the same sequence again —
    /// [`crate::nested_sequence::materialize_nested_sequences`]'s cycle guard.
    CyclicNestedSequence,
    /// Probing a rendered nested-sequence file (to build its synthetic [`crate::MediaAsset`])
    /// failed.
    ProbeNestedSequence,
}

impl std::fmt::Display for RenderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RenderError::OpenInput => write!(f, "failed to open input"),
            RenderError::StreamInfo => write!(f, "failed to read stream info"),
            RenderError::AllocOutput => write!(f, "failed to allocate output context"),
            RenderError::NewStream => write!(f, "failed to create an output stream"),
            RenderError::OpenOutput => write!(f, "failed to open output for writing"),
            RenderError::WriteHeader => write!(f, "failed to write output header"),
            RenderError::WriteFrame => write!(f, "failed to write a frame"),
            RenderError::NoAudioStream => write!(f, "source has no audio stream"),
            RenderError::Decoder => write!(f, "failed to open the audio decoder"),
            RenderError::FilterGraph => write!(f, "failed to build the audio filter graph"),
            RenderError::Encoder => write!(f, "failed to open the AAC encoder"),
            RenderError::Pipeline => write!(f, "decode/filter/encode pipeline failed mid-render"),
            RenderError::Unknown(code) => write!(f, "unknown render status code: {code}"),
            RenderError::NoVideoStream => write!(f, "a clip's source has no video stream"),
            RenderError::AudioFormatMismatch => {
                write!(
                    f,
                    "a clip's audio format doesn't match the sequence's first clip"
                )
            }
            RenderError::EmptyTimeline => write!(f, "sequence has no video clips to export"),
            RenderError::MissingAsset => write!(f, "a clip references a missing media asset"),
            RenderError::InvalidCanvas => {
                write!(f, "render canvas has invalid dimensions or frame rate")
            }
            RenderError::InvalidTimelineSegment => {
                write!(f, "timeline contains an invalid render segment")
            }
            RenderError::InvalidTargetLufs => write!(f, "loudness target must be finite"),
            RenderError::AudioMix(message) => write!(f, "audio mix failed: {message}"),
            RenderError::MediaMux(message) => write!(f, "audio mux failed: {message}"),
            RenderError::ReplaceOutput(error) => write!(f, "failed to replace export: {error}"),
            RenderError::MissingNestedSequence => {
                write!(
                    f,
                    "a compound clip references a sequence that no longer exists"
                )
            }
            RenderError::CyclicNestedSequence => {
                write!(f, "a compound clip (transitively) nests its own sequence")
            }
            RenderError::ProbeNestedSequence => {
                write!(f, "failed to probe a rendered nested-sequence file")
            }
        }
    }
}

impl std::error::Error for RenderError {}

impl From<avbridge::EncodeError> for RenderError {
    fn from(e: avbridge::EncodeError) -> Self {
        use avbridge::EncodeError as E;
        match e {
            E::InvalidPath(_) => RenderError::OpenInput,
            E::OpenInput => RenderError::OpenInput,
            E::StreamInfo => RenderError::StreamInfo,
            E::AllocOutput => RenderError::AllocOutput,
            E::NewStream => RenderError::NewStream,
            E::OpenOutput => RenderError::OpenOutput,
            E::WriteHeader => RenderError::WriteHeader,
            E::WriteFrame => RenderError::WriteFrame,
            E::NoAudioStream => RenderError::NoAudioStream,
            E::Decoder => RenderError::Decoder,
            E::FilterGraph => RenderError::FilterGraph,
            E::Encoder => RenderError::Encoder,
            E::Pipeline => RenderError::Pipeline,
            E::NoVideoStream => RenderError::NoVideoStream,
            E::AudioFormatMismatch => RenderError::AudioFormatMismatch,
            E::EmptyTimeline => RenderError::EmptyTimeline,
            E::Unknown(code) => RenderError::Unknown(code),
        }
    }
}

/// How a render ended: all the way through, or stopped early because `cancel` was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderOutcome {
    Completed,
    Cancelled,
}

/// Renders `source` to `output`, applying single-pass loudnorm normalization to
/// `target_lufs` and copying video untouched. Blocks the calling thread until the render
/// finishes, is cancelled, or fails — callers on a UI thread should run this from a background
/// thread and use `cancel` to interrupt it instead of blocking on it.
///
/// Calls `on_progress(percent)` as the encode reports progress, and checks `cancel` between
/// packets — if another thread sets it, this returns `Ok(RenderOutcome::Cancelled)` rather
/// than treating the cancellation as a failure. `output` is left truncated/invalid in that
/// case (no trailer written), same as previously killing an in-progress `ffmpeg` subprocess.
pub fn render_export(
    source: &Path,
    output: &Path,
    target_lufs: f32,
    duration_secs: f64,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    validate_target_lufs(target_lufs)?;
    let outcome = avbridge::encode_export(source, output, target_lufs, cancel, |secs| {
        if duration_secs > 0.0 {
            let percent = ((secs / duration_secs) * 100.0).clamp(0.0, 100.0) as u8;
            on_progress(percent);
        }
    })?;

    Ok(match outcome {
        avbridge::EncodeOutcome::Completed => {
            on_progress(100);
            RenderOutcome::Completed
        }
        avbridge::EncodeOutcome::Cancelled => RenderOutcome::Cancelled,
    })
}

fn validate_target_lufs(target_lufs: f32) -> Result<(), RenderError> {
    target_lufs
        .is_finite()
        .then_some(())
        .ok_or(RenderError::InvalidTargetLufs)
}

fn validate_canvas(canvas: Canvas) -> Result<(), RenderError> {
    (canvas.width > 0 && canvas.height > 0 && canvas.fps_num > 0 && canvas.fps_den > 0)
        .then_some(())
        .ok_or(RenderError::InvalidCanvas)
}

fn valid_clip_segment(segment: &avbridge::ClipSegment) -> bool {
    !segment.source_path.as_os_str().is_empty()
        && segment.source_in_secs.is_finite()
        && segment.source_out_secs.is_finite()
        && segment.source_in_secs >= 0.0
        && segment.source_out_secs > segment.source_in_secs
        && segment.gain_db.is_finite()
        && segment.speed_factor.is_finite()
        && segment.speed_factor > 0.0
        && segment.smooth_speed_ramp_end_factor.is_finite()
        && segment.timeline_start_secs.is_finite()
        && segment.timeline_start_secs >= 0.0
        && segment.transition_duration_secs.is_finite()
        && segment.transition_duration_secs >= 0.0
        && segment.anchor_x.is_finite()
        && segment.anchor_y.is_finite()
        && (0.0..=1.0).contains(&segment.anchor_x)
        && (0.0..=1.0).contains(&segment.anchor_y)
}

fn validate_segment_list(
    segments: &[avbridge::ClipSegment],
    canvas: Canvas,
    target_lufs: f32,
) -> Result<(), RenderError> {
    validate_canvas(canvas)?;
    validate_target_lufs(target_lufs)?;
    if segments.is_empty() {
        return Err(RenderError::EmptyTimeline);
    }
    if segments.iter().any(|segment| !valid_clip_segment(segment)) {
        return Err(RenderError::InvalidTimelineSegment);
    }
    Ok(())
}

fn validate_timeline_segments(
    track_segments: &[Vec<avbridge::ClipSegment>],
    canvas: Canvas,
    target_lufs: f32,
) -> Result<(), RenderError> {
    if track_segments.is_empty() || track_segments[0].is_empty() {
        return Err(RenderError::EmptyTimeline);
    }
    for track in track_segments {
        validate_segment_list(track, canvas, target_lufs)?;
    }
    Ok(())
}

/// Resolves `sequence`'s first non-empty video track into export segments (in `start_secs`
/// order) plus the canvas every segment gets conformed onto — the snapshot
/// [`crate::export::ExportJob`] takes when a job enters the queue, and what "Add Export"
/// button actually captures, replacing a single raw asset. Covers the effect fields
/// [`crate::timeline::ClipInstance::video_filter_chain`] documents (crop, brightness/contrast/
/// saturation, color filter, vignette, sharpen, chroma key, blur) plus per-clip audio gain and
/// freeze frame ([`crate::timeline::ClipInstance::frozen`], resolved to
/// [`avbridge::ClipSegment::frozen`] rather than a filter string). The complete visible-track
/// audio snapshot is resolved separately by [`resolve_audio_segments`].
///
/// The canvas's resolution/frame rate is the first clip's own probed resolution/fps; its
/// bitrate is a duration-weighted average of every clip's own source bitrate — video is
/// re-encoded here (unlike [`render_export`]'s exact passthrough copy), so an exact
/// source-bitrate match isn't possible once effects require decode+filter+encode; this is the
/// closest equivalent for a multi-clip timeline.
///
/// Builds `clip`'s combined avfilter `video_filter` string (scale/rotation/opacity keyframe
/// stages, from [`ClipInstance::keyframe_video_filter_chain`], spliced onto the front of
/// [`ClipInstance::video_filter_chain`]'s own stages — the same position the old `zoom` stage
/// used to occupy, and [`ClipInstance::layer_scale_x`]/`_y`'s resize stage appended after
/// everything else — it must be the last geometric transform before compositing, so every
/// earlier stage still operates at native resolution) plus its `overlay` position expressions
/// (from [`keyframe::position_overlay_xy_expr`], empty strings when there's nothing to
/// animate). `is_overlay` gates the resize stage — a single/background track's final
/// canvas-size conform has no pad/fit step, so appending it there would hand the encoder a
/// mismatched-resolution frame instead of a smaller picture within the canvas; position/opacity
/// keyframes have the same overlay-only caveat for a compositing reason instead. Shared by
/// [`resolve_timeline_segments`] and [`resolve_timeline_segments_multi`] since both need
/// identical per-clip resolution.
/// A [`avbridge::ClipSegment`]'s own on-timeline duration — plain division for a constant
/// `speed_factor`, [`keyframe::smooth_speed_ramp_duration_secs`]'s log-based integral when
/// `smooth_speed_ramp_end_factor` is set (`> 0.0`, this FFI struct's own "no ramp" sentinel —
/// see its doc comment in `avbridge::lib`). Used for progress-reporting's total-duration
/// estimate, which otherwise silently under/over-counts a ramped segment.
fn clip_segment_duration_secs(segment: &avbridge::ClipSegment) -> f64 {
    let source_duration = segment.source_out_secs - segment.source_in_secs;
    if segment.smooth_speed_ramp_end_factor > 0.0 {
        keyframe::smooth_speed_ramp_duration_secs(
            source_duration,
            segment.speed_factor,
            segment.smooth_speed_ramp_end_factor,
        )
    } else {
        let speed = if segment.speed_factor > 0.0 {
            segment.speed_factor as f64
        } else {
            1.0
        };
        source_duration / speed
    }
}

fn resolve_clip_filters(
    clip: &ClipInstance,
    fps_num: u32,
    fps_den: u32,
    duration_secs: f64,
    is_overlay: bool,
) -> (String, String, String) {
    let base_filter = clip.video_filter_chain();
    let mut video_filter = match clip.keyframe_video_filter_chain(fps_num, fps_den, duration_secs) {
        Some(kf) if base_filter.is_empty() => kf,
        Some(kf) => format!("{kf},{base_filter}"),
        None => base_filter,
    };
    if is_overlay && clip.has_layer_scale() {
        let resize = format!(
            "scale=iw*{:.4}:ih*{:.4}",
            clip.layer_scale_x, clip.layer_scale_y
        );
        video_filter = if video_filter.is_empty() {
            resize
        } else {
            format!("{video_filter},{resize}")
        };
    }
    let (position_x_expr, position_y_expr) =
        keyframe::position_overlay_xy_expr(&clip.position_keyframes, duration_secs)
            .unwrap_or_default();
    (video_filter, position_x_expr, position_y_expr)
}

/// Fails with [`RenderError::EmptyTimeline`] if the sequence has no video track with at least
/// one clip, [`RenderError::MissingAsset`] if a clip's `asset_id` isn't in `media_library`.
pub fn resolve_timeline_segments(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<(Vec<avbridge::ClipSegment>, Canvas), RenderError> {
    let track = sequence
        .timeline
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Video && !t.clips.is_empty())
        .ok_or(RenderError::EmptyTimeline)?;

    let mut clips: Vec<_> = track.clips.iter().collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

    let mut segments = Vec::with_capacity(clips.len());
    let mut total_duration_secs = 0.0;
    let mut weighted_bitrate_bps_secs = 0.0;
    let mut dimensions_fps = None;
    for clip in &clips {
        let asset = media_library
            .iter()
            .find(|a| a.id == clip.asset_id)
            .ok_or(RenderError::MissingAsset)?;
        if dimensions_fps.is_none() {
            let (width, height) = asset.resolution.ok_or(RenderError::NoVideoStream)?;
            dimensions_fps = Some((width, height, asset.fps.unwrap_or(30.0)));
        }
        let duration_secs = clip.duration_secs();
        total_duration_secs += duration_secs;
        weighted_bitrate_bps_secs += asset.source_bitrate_mbps as f64 * 1_000_000.0 * duration_secs;
        let (_, _, fps) = dimensions_fps.expect("just set above if it was None");
        let (fps_num, fps_den) = fps_to_rational(fps);
        let (video_filter, position_x_expr, position_y_expr) =
            resolve_clip_filters(clip, fps_num, fps_den, duration_secs, false);
        segments.push(avbridge::ClipSegment {
            source_path: asset.source_path.clone(),
            source_in_secs: clip.source_in_secs,
            source_out_secs: clip.source_out_secs,
            gain_db: clip.gain_db,
            video_filter,
            frozen: clip.frozen,
            speed_factor: clip.speed_factor,
            smooth_speed_ramp_end_factor: clip.speed_ramp_end_factor.unwrap_or(0.0),
            position_x_expr,
            position_y_expr,
            transition_in: clip.transition_in.to_export_code(),
            transition_duration_secs: clip.transition_duration_secs,
            timeline_start_secs: clip.start_secs,
            // Ignored by avbridge_encode_timeline_export (this single-track path) regardless —
            // there's no compositing stage for the alpha to survive into, same reasoning as
            // resolve_clip_filters' is_overlay gate above.
            mask_video_path: String::new(),
            // Same reasoning as mask_video_path above — blend_mode is only meaningful on an
            // overlay track, which this single-track path never has.
            blend_mode: String::new(),
            anchor_x: clip.anchor_x,
            anchor_y: clip.anchor_y,
        });
    }
    let (width, height, fps) = dimensions_fps.ok_or(RenderError::EmptyTimeline)?;
    let (fps_num, fps_den) = fps_to_rational(fps);
    let bit_rate_bps = if total_duration_secs > 0.0 {
        (weighted_bitrate_bps_secs / total_duration_secs) as i64
    } else {
        0
    };
    let canvas = Canvas {
        width,
        height,
        fps_num,
        fps_den,
        bit_rate_bps,
    };

    Ok((segments, canvas))
}

/// Renders `segments` (already resolved by [`resolve_timeline_segments`], e.g. from a queued
/// [`crate::export::ExportJob`]) as one continuous file via `avbridge::encode_timeline_export`,
/// then composites `text_segments` over the result using `avbridge::apply_text_overlays` if any
/// are present. `on_progress` receives a 0-100 percent, computed off `segments`' own total
/// trimmed duration, same contract as [`render_export`].
///
/// Text overlay errors are logged but do not fail the export — the video is already complete
/// without the overlays and deleting the caller's file on a post-processing issue would be worse.
#[allow(clippy::too_many_arguments)]
pub fn render_export_job(
    segments: &[avbridge::ClipSegment],
    canvas: Canvas,
    output: &Path,
    target_lufs: f32,
    gpu_encoder: avbridge::GpuEncoderPreference,
    text_segments: &[TextSegment],
    shape_segments: &[avbridge::ShapeSegment],
    privacy_blur_segments: &[PrivacyBlurSegment],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    validate_segment_list(segments, canvas, target_lufs)?;
    let total_duration_secs: f64 = segments.iter().map(clip_segment_duration_secs).sum();

    let outcome = avbridge::encode_timeline_export(
        segments,
        canvas,
        output,
        target_lufs,
        gpu_encoder,
        cancel,
        |secs| {
            if total_duration_secs > 0.0 {
                let percent = ((secs / total_duration_secs) * 100.0).clamp(0.0, 100.0) as u8;
                on_progress(percent);
            }
        },
    )?;

    Ok(match outcome {
        avbridge::EncodeOutcome::Completed => {
            on_progress(100);
            // Apply text/shape overlays as post-processing passes if any were placed. Shapes
            // after text so a highlight box can sit visually above a caption if the user
            // stacks them at the same position — an arbitrary but consistent choice, same as
            // any other z-order tie-break. Privacy blur last: it should blur whatever text/
            // shapes already ended up inside the tracked region too, not sit underneath them.
            if !text_segments.is_empty() {
                apply_text_overlay_pass(output, canvas, text_segments);
            }
            if !shape_segments.is_empty() {
                apply_shape_overlay_pass(output, canvas, shape_segments);
            }
            if !privacy_blur_segments.is_empty() {
                apply_privacy_blur_pass(output, canvas, total_duration_secs, privacy_blur_segments);
            }
            RenderOutcome::Completed
        }
        avbridge::EncodeOutcome::Cancelled => RenderOutcome::Cancelled,
    })
}

/// Resolves `sequence` into segments ([`resolve_timeline_segments`]) and renders them
/// immediately ([`render_export_job`]) — the one-call convenience most callers want; the
/// export queue instead calls the two steps separately so it can snapshot the resolved
/// segments into an [`crate::export::ExportJob`] before rendering starts.
pub fn render_timeline_export(
    sequence: &Sequence,
    media_library: &[MediaAsset],
    output: &Path,
    target_lufs: f32,
    gpu_encoder: avbridge::GpuEncoderPreference,
    cancel: &AtomicBool,
    on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    let (track_segments, canvas) = resolve_timeline_segments_multi(sequence, media_library)?;
    let audio_segments = resolve_audio_segments(sequence, media_library)?;
    let text_segments = resolve_text_segments(sequence, canvas.width, canvas.height);
    let shape_segments = resolve_shape_segments(sequence, canvas.width, canvas.height);
    let privacy_blur_segments = resolve_privacy_blur_segments(sequence);
    render_export_job_multi_with_audio(
        &track_segments,
        &audio_segments,
        canvas,
        output,
        target_lufs,
        gpu_encoder,
        &text_segments,
        &shape_segments,
        &privacy_blur_segments,
        cancel,
        on_progress,
    )
}

/// Resolves every audible clip from visible video and audio tracks. Returns an empty list when
/// the first video track is the only audio contributor, allowing the established one-pass
/// export path to avoid an unnecessary second AAC encode. Once any additional track contributes
/// audio, the returned list includes the background clips too because the post-pass replaces,
/// rather than layers on top of, the original output audio.
pub fn resolve_audio_segments(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<Vec<avbridge::AudioSegment>, RenderError> {
    let first_video_track = sequence.timeline.tracks.iter().position(|track| {
        track.kind == TrackKind::Video && track.visible && !track.clips.is_empty()
    });

    let mut resolved = Vec::new();
    let mut has_additional_contributor = false;
    for (track_index, track) in sequence.timeline.tracks.iter().enumerate() {
        if !track.visible || !matches!(track.kind, TrackKind::Video | TrackKind::Audio) {
            continue;
        }
        for clip in &track.clips {
            let asset = media_library
                .iter()
                .find(|asset| asset.id == clip.asset_id)
                .ok_or(RenderError::MissingAsset)?;
            if !asset.has_audio {
                continue;
            }
            if Some(track_index) != first_video_track {
                has_additional_contributor = true;
            }
            resolved.push(avbridge::AudioSegment {
                source_path: asset.source_path.clone(),
                source_in_secs: clip.source_in_secs,
                source_out_secs: clip.source_out_secs,
                timeline_start_secs: clip.start_secs,
                gain_db: clip.gain_db,
                // AudioSegment has no ramp field of its own — atempo takes one fixed parameter,
                // not a `t`-keyed expression, so a smooth per-sample tempo ramp isn't achievable
                // on the audio side in this FFmpeg build (same limitation
                // ClipSegment::smooth_speed_ramp_end_factor's own doc comment describes). The
                // ramp's *average* speed stands in when one is active, matching the approximation
                // ClipSegment's own inline audio path already uses for a ramped background/
                // single-track clip.
                speed_factor: clip
                    .speed_ramp_end_factor
                    .map(|end| (clip.speed_factor + end) / 2.0)
                    .unwrap_or(clip.speed_factor),
                gain_keyframe_expr: keyframe::gain_filter_db_expr(
                    &clip.gain_keyframes,
                    clip.duration_secs(),
                )
                .unwrap_or_default(),
                duck_role: track.audio_role.to_duck_role_code(),
                voice_cleanup_enabled: clip.voice_cleanup_enabled,
                voice_cleanup_noise_floor_db: clip.voice_cleanup_noise_floor_db,
                voice_cleanup_compressor_threshold_db: clip.voice_cleanup_compressor_threshold_db,
                voice_cleanup_compressor_ratio: clip.voice_cleanup_compressor_ratio,
                voice_cleanup_ceiling_linear: clip.voice_cleanup_ceiling_linear,
            });
        }
    }
    if !has_additional_contributor {
        return Ok(Vec::new());
    }
    resolved.sort_by(|a, b| a.timeline_start_secs.total_cmp(&b.timeline_start_secs));
    Ok(resolved)
}

/// Applies text overlays to an already-written export file in place. Writes to a temp path
/// beside `output`, then renames over `output`. Logs and silently skips on any error so an
/// overlay-raster or filter failure doesn't destroy the already-completed video file.
fn apply_text_overlay_pass(output: &Path, canvas: Canvas, text_segments: &[TextSegment]) {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let temp_id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}-{temp_id}", std::process::id());
    let tmp = parent.join(format!("{stem}.oca-text-{suffix}.mp4"));
    let overlay_dir = parent.join(format!(".{stem}.oca-text-{suffix}"));
    if let Err(error) = std::fs::create_dir(&overlay_dir) {
        tracing::error!(error = %error, output = %output.display(), "text overlay temp directory failed");
        return;
    }

    let mut overlays = Vec::with_capacity(text_segments.len());
    for (index, segment) in text_segments.iter().enumerate() {
        let rgba =
            crate::overlay_render::render_text_segment_rgba(segment, canvas.width, canvas.height);
        let overlay_path = overlay_dir.join(format!("overlay-{index}.png"));
        if let Err(error) = image::save_buffer_with_format(
            &overlay_path,
            &rgba,
            canvas.width,
            canvas.height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        ) {
            tracing::error!(error = %error, output = %output.display(), "text overlay raster write failed");
            let _ = std::fs::remove_dir_all(&overlay_dir);
            return;
        }
        overlays.push(avbridge::TextOverlaySegment {
            start_secs: segment.start_secs,
            duration_secs: segment.duration_secs,
            overlay_path,
            opacity_keyframe_expr: segment.opacity_keyframe_expr.clone(),
            position_keyframe_expr_x: segment.position_keyframe_expr_x.clone(),
            position_keyframe_expr_y: segment.position_keyframe_expr_y.clone(),
            scale_keyframe_expr_x: segment.scale_keyframe_expr_x.clone(),
            scale_keyframe_expr_y: segment.scale_keyframe_expr_y.clone(),
            rotation_keyframe_expr_x: segment.rotation_keyframe_expr_x.clone(),
            rotation_keyframe_expr_y: segment.rotation_keyframe_expr_y.clone(),
        });
    }

    match avbridge::apply_text_overlays(
        output,
        &tmp,
        &overlays,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
    ) {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, output) {
                tracing::error!(error = %e, output = %output.display(), "text overlay rename failed");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, output = %output.display(), "text overlay skipped");
            let _ = std::fs::remove_file(&tmp);
        }
    }
    let _ = std::fs::remove_dir_all(&overlay_dir);
}

/// Applies shape overlays to an already-written export file in place — same "temp path, rename
/// over `output`, log-and-skip on error" shape as [`apply_text_overlay_pass`].
fn apply_shape_overlay_pass(
    output: &Path,
    canvas: Canvas,
    shape_segments: &[avbridge::ShapeSegment],
) {
    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let tmp = parent.join(format!("{stem}.shape_tmp.mp4"));

    match avbridge::apply_shape_overlays(
        output,
        &tmp,
        shape_segments,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
    ) {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, output) {
                tracing::error!(error = %e, output = %output.display(), "shape overlay rename failed");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, output = %output.display(), "shape overlay skipped");
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

/// One CF-09 slice 4 privacy-blur clip snapshotted from the sequence at export-queue time — same
/// "resolved once, doesn't retroactively change if the source project is edited later"
/// convention [`TextSegment`]/[`avbridge::ShapeSegment`] already have. `mask_path` is the clip-
/// local matte video path [`crate::mask_propagation`]/[`crate::background_removal::
/// encode_matte_video`] produced; `start_secs`/`duration_secs` are this clip's own on-timeline
/// window, used by [`apply_privacy_blur_pass`] to pad that matte out to the export's own canvas
/// duration (see that function's own doc comment for why padding, not a filter-graph timeline
/// window).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PrivacyBlurSegment {
    pub mask_path: String,
    pub blur_sigma: f32,
    pub start_secs: f64,
    pub duration_secs: f64,
}

/// Collects every video-track [`ClipInstance`] with `privacy_blur_enabled` and a non-empty
/// `privacy_blur_mask_path` into a [`PrivacyBlurSegment`], sorted by `start_secs` ascending —
/// same "snapshot at queue time" convention [`resolve_text_segments`]/[`resolve_shape_segments`]
/// already use.
pub fn resolve_privacy_blur_segments(sequence: &Sequence) -> Vec<PrivacyBlurSegment> {
    let mut clips: Vec<&ClipInstance> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video)
        .flat_map(|t| &t.clips)
        .filter(|c| c.privacy_blur_enabled && !c.privacy_blur_mask_path.is_empty())
        .collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    clips
        .into_iter()
        .map(|c| PrivacyBlurSegment {
            mask_path: c.privacy_blur_mask_path.clone(),
            blur_sigma: c.privacy_blur_sigma,
            start_secs: c.start_secs,
            duration_secs: c.duration_secs(),
        })
        .collect()
}

/// Upper bound on how many frames [`build_padded_matte`] decodes from one clip-local matte
/// video — bounds worst-case memory (every decoded luma frame is held in memory at once, same
/// shape [`crate::background_removal::encode_matte_video`]'s own caller already accepts) for a
/// very long privacy-blur clip. A clip whose own `duration_secs * canvas fps` exceeds this still
/// gets a full-duration matte, just resampled at a coarser-than-canvas rate — `movie=...:loop=0`
/// holds each sampled frame until the next one, same tolerance the background-removal matte
/// already relies on for a clip whose sampling rate doesn't match the canvas exactly.
const MAX_PRIVACY_BLUR_MATTE_FRAMES: usize = 10_000;

/// Applies every privacy-blur segment to an already-written export file in place, one at a time
/// (each pass's own output feeds the next, same chaining [`apply_text_overlay_pass`]/
/// [`apply_shape_overlay_pass`] already use) — logs and skips a segment on any error rather than
/// failing the whole export, same "already-complete video, don't destroy it over a post-
/// processing issue" posture those two passes take.
fn apply_privacy_blur_pass(
    output: &Path,
    canvas: Canvas,
    canvas_duration_secs: f64,
    segments: &[PrivacyBlurSegment],
) {
    for segment in segments {
        apply_one_privacy_blur_segment(output, canvas, canvas_duration_secs, segment);
    }
}

/// One segment's own pass: re-samples its clip-local matte video (`segment.mask_path`) at the
/// canvas's own `fps_num`/`fps_den`, evenly across `[0, duration_secs)` — the matte is a plain
/// grayscale-as-luma H.264 file, no different from any other video [`crate::FrameSampler`]
/// already decodes — converts each decoded frame to a luma buffer
/// ([`crate::motion_tracking::rgba_to_gray`]), pads the result out to `canvas_duration_secs`
/// (black frames before `start_secs` and after `start_secs + duration_secs` —
/// [`crate::privacy_blur::pad_matte_frames_to_canvas_duration`]), re-encodes the padded frames
/// into a temporary matte video, then calls [`crate::privacy_blur::apply_privacy_blur`]. Every
/// decoded frame must match the canvas's own `width`/`height` exactly — a clip-local matte
/// encoded at a different resolution (shouldn't happen: it's always sampled from the same
/// source clip this segment came from) is treated as a build failure rather than silently
/// resizing or corrupting the padded buffer.
fn apply_one_privacy_blur_segment(
    output: &Path,
    canvas: Canvas,
    canvas_duration_secs: f64,
    segment: &PrivacyBlurSegment,
) {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    let Some(parent) = output.parent() else {
        return;
    };
    let Some(stem) = output.file_stem().and_then(|s| s.to_str()) else {
        return;
    };
    let temp_id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}-{temp_id}", std::process::id());
    let tmp = parent.join(format!("{stem}.oca-privacy-blur-{suffix}.mp4"));
    let matte_tmp = parent.join(format!("{stem}.oca-privacy-blur-matte-{suffix}.mp4"));

    if let Err(error) = build_padded_matte(
        Path::new(&segment.mask_path),
        &matte_tmp,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
        segment.start_secs,
        segment.duration_secs,
        canvas_duration_secs,
    ) {
        tracing::error!(error = %error, output = %output.display(), "privacy blur matte build failed");
        let _ = std::fs::remove_file(&matte_tmp);
        return;
    }

    match crate::privacy_blur::apply_privacy_blur(
        output,
        &tmp,
        &matte_tmp,
        segment.blur_sigma as f64,
        canvas.width,
        canvas.height,
        canvas.fps_num,
        canvas.fps_den,
    ) {
        Ok(()) => {
            if let Err(e) = std::fs::rename(&tmp, output) {
                tracing::error!(error = %e, output = %output.display(), "privacy blur rename failed");
                let _ = std::fs::remove_file(&tmp);
            }
        }
        Err(e) => {
            tracing::error!(error = %e, output = %output.display(), "privacy blur skipped");
            let _ = std::fs::remove_file(&tmp);
        }
    }
    let _ = std::fs::remove_file(&matte_tmp);
}

#[allow(clippy::too_many_arguments)]
fn build_padded_matte(
    matte_path: &Path,
    out_path: &Path,
    canvas_width: u32,
    canvas_height: u32,
    fps_num: u32,
    fps_den: u32,
    clip_start_secs: f64,
    clip_duration_secs: f64,
    canvas_duration_secs: f64,
) -> Result<(), String> {
    let fps = if fps_den > 0 {
        fps_num as f64 / fps_den as f64
    } else {
        0.0
    };
    if fps <= 0.0 || clip_duration_secs <= 0.0 {
        return Err("invalid fps or clip duration".to_string());
    }
    let natural_count = (clip_duration_secs * fps).ceil() as usize + 1;
    let sample_times = crate::frame_sampler::FrameSampler::even_sample_times(
        0.0,
        clip_duration_secs,
        fps,
        1,
        natural_count.min(MAX_PRIVACY_BLUR_MATTE_FRAMES),
    );
    let sampler =
        crate::frame_sampler::FrameSampler::open(matte_path, std::time::Duration::from_millis(20))
            .map_err(|e| format!("failed to open matte for decoding: {e}"))?;

    let mut clip_frames: Vec<Vec<u8>> = Vec::with_capacity(sample_times.len());
    for &t in &sample_times {
        let Some(frame) = sampler.sample(t, std::time::Duration::from_millis(1500)) else {
            continue;
        };
        if frame.width != canvas_width || frame.height != canvas_height {
            return Err(format!(
                "matte resolution {}x{} does not match canvas {}x{}",
                frame.width, frame.height, canvas_width, canvas_height
            ));
        }
        let gray = crate::motion_tracking::rgba_to_gray(&frame.rgba, frame.width, frame.height);
        clip_frames.push(gray.data);
    }
    if clip_frames.is_empty() {
        return Err("couldn't decode any matte frames".to_string());
    }

    let padded = crate::privacy_blur::pad_matte_frames_to_canvas_duration(
        &clip_frames,
        (canvas_width as usize) * (canvas_height as usize),
        fps_num,
        fps_den,
        clip_start_secs,
        clip_duration_secs,
        canvas_duration_secs,
    );
    crate::background_removal::encode_matte_video(
        &padded,
        canvas_width,
        canvas_height,
        fps_num,
        fps_den,
        out_path,
    )
    .map_err(|e| format!("failed to encode padded matte: {e}"))
}

/// Collects all [`TextClip`]s from `sequence`'s text tracks into [`TextSegment`]s,
/// sorted by `start_secs` ascending. Returns an empty vec if the sequence has no text tracks
/// or none have any clips. Used to pass text overlays to the post-processing pass after the
/// main video encode ([`render_export_job`]).
///
/// `canvas_width` is retained as a zero-width guard for malformed export settings; word
/// highlights otherwise use UTF-8 byte ranges into the complete caption, letting the shared
/// rasterizer place them with the exact same wrapping as the base text. `canvas_height` is
/// needed only to scale `pos_y_keyframes`' offset expression into pixels.
pub fn resolve_text_segments(
    sequence: &Sequence,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Text)
        .flat_map(|t| &t.text_clips)
        .flat_map(|clip| text_clip_to_segments(clip, canvas_width, canvas_height))
        .collect();
    segments.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    segments
}

/// Expands one [`TextClip`] into one or more [`TextSegment`]s. A plain clip (no words,
/// or `highlight_enabled` off) is exactly the one segment it's always been. A word-highlight
/// clip becomes a base segment (the full text, in `color_rgba`, for the clip's whole duration —
/// so something is always on screen even between words / before the first word starts) plus one
/// short segment per matched word (the complete caption plus that word's UTF-8 byte range, in
/// `highlight_color_rgba`, only for its own `[start_secs, end_secs)`). Rendering the complete
/// caption layout and filtering its glyphs by byte range makes highlights follow both explicit
/// newlines and automatic word wrapping exactly.
fn text_clip_to_segments(
    clip: &TextClip,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<TextSegment> {
    // Built once from the base clip's own timing (not any individual segment's) and copied onto
    // every segment below -- see TextSegment::opacity_keyframe_expr's doc comment for why.
    let opacity_keyframe_expr = keyframe::text_opacity_alpha_expr(
        &clip.opacity_keyframes,
        clip.start_secs,
        clip.duration_secs,
    )
    .unwrap_or_default();

    // The raster anchor: the first keyframe's value when position keyframes are present (same
    // "keyframes win when present" convention as ShapeClip's own fields), the plain constant
    // otherwise -- see TextClip::pos_x_keyframes' doc comment. position_keyframe_expr_x/y is
    // then the *delta* from this anchor, in canvas pixels, as a function of timeline-absolute
    // `t` -- zero for every unkeyframed axis, so this is a no-op when neither axis is animated.
    let base_pos_x = clip.pos_x_keyframes.first().map_or(clip.pos_x, |k| k.value);
    let base_pos_y = clip.pos_y_keyframes.first().map_or(clip.pos_y, |k| k.value);
    let position_keyframe_expr_x = keyframe::text_position_offset_expr(
        &clip.pos_x_keyframes,
        base_pos_x,
        clip.start_secs,
        clip.duration_secs,
        canvas_width as f32,
    )
    .unwrap_or_default();
    let position_keyframe_expr_y = keyframe::text_position_offset_expr(
        &clip.pos_y_keyframes,
        base_pos_y,
        clip.start_secs,
        clip.duration_secs,
        canvas_height as f32,
    )
    .unwrap_or_default();

    // The scale remap's own anchor is the same raster-baked position anchor above, in canvas
    // pixels -- see TextClip::scale_keyframes' doc comment.
    let (scale_keyframe_expr_x, scale_keyframe_expr_y) = keyframe::text_scale_sample_exprs(
        &clip.scale_keyframes,
        base_pos_x * canvas_width as f32,
        base_pos_y * canvas_height as f32,
        clip.start_secs,
        clip.duration_secs,
    )
    .unwrap_or_default();

    // Same anchor as the scale remap above -- see TextClip::rotation_keyframes' doc comment.
    let (rotation_keyframe_expr_x, rotation_keyframe_expr_y) =
        keyframe::text_rotation_sample_exprs(
            &clip.rotation_keyframes,
            base_pos_x * canvas_width as f32,
            base_pos_y * canvas_height as f32,
            clip.start_secs,
            clip.duration_secs,
        )
        .unwrap_or_default();

    let base = TextSegment {
        start_secs: clip.start_secs,
        duration_secs: clip.duration_secs,
        text: clip.text.clone(),
        font_size: clip.font_size,
        font_family: clip.font_family.clone(),
        font_style: clip.font_style,
        font_weight: clip.font_weight,
        color_rgba: clip.color_rgba,
        background_rgba: clip.background_rgba,
        background_padding: clip.background_padding,
        background_corner_radius: clip.background_corner_radius,
        glyph_byte_range: None,
        pos_x: base_pos_x,
        pos_y: base_pos_y,
        opacity_keyframe_expr: opacity_keyframe_expr.clone(),
        position_keyframe_expr_x: position_keyframe_expr_x.clone(),
        position_keyframe_expr_y: position_keyframe_expr_y.clone(),
        scale_keyframe_expr_x: scale_keyframe_expr_x.clone(),
        scale_keyframe_expr_y: scale_keyframe_expr_y.clone(),
        rotation_keyframe_expr_x: rotation_keyframe_expr_x.clone(),
        rotation_keyframe_expr_y: rotation_keyframe_expr_y.clone(),
        direction: clip.direction,
        text_align: clip.text_align,
    };
    if !clip.highlight_enabled || clip.words.is_empty() || canvas_width == 0 {
        return vec![base];
    }

    let words: Vec<&str> = clip.words.iter().map(|w| w.text.as_str()).collect();
    let byte_ranges = crate::text_metrics::word_byte_ranges(&clip.text, &words);

    let mut segments = Vec::with_capacity(1 + clip.words.len());
    segments.push(base);
    for (word, byte_range) in clip.words.iter().zip(byte_ranges) {
        let Some([start, end]) = byte_range else {
            continue;
        };
        let (Ok(start), Ok(end)) = (u32::try_from(start), u32::try_from(end)) else {
            continue;
        };
        segments.push(TextSegment {
            start_secs: clip.start_secs + word.start_secs,
            duration_secs: (word.end_secs - word.start_secs).max(0.05),
            text: clip.text.clone(),
            font_size: clip.font_size,
            font_family: clip.font_family.clone(),
            font_style: clip.font_style,
            font_weight: clip.font_weight,
            color_rgba: clip.highlight_color_rgba,
            background_rgba: [0, 0, 0, 0],
            background_padding: 0.0,
            background_corner_radius: 0.0,
            glyph_byte_range: Some([start, end]),
            pos_x: base_pos_x,
            pos_y: base_pos_y,
            opacity_keyframe_expr: opacity_keyframe_expr.clone(),
            position_keyframe_expr_x: position_keyframe_expr_x.clone(),
            position_keyframe_expr_y: position_keyframe_expr_y.clone(),
            scale_keyframe_expr_x: scale_keyframe_expr_x.clone(),
            scale_keyframe_expr_y: scale_keyframe_expr_y.clone(),
            rotation_keyframe_expr_x: rotation_keyframe_expr_x.clone(),
            rotation_keyframe_expr_y: rotation_keyframe_expr_y.clone(),
            direction: clip.direction,
            text_align: clip.text_align,
        });
    }
    segments
}

/// Collects all [`ShapeClip`]s from `sequence`'s shape tracks into [`avbridge::ShapeSegment`]s
/// (each already a complete `geq` filter node, via
/// `crate::shape_render::build_shape_filter_desc`), sorted by `start_secs` ascending. Returns
/// an empty vec if the sequence has no shape tracks or none have any clips. Same role as
/// [`resolve_text_segments`], one post-processing pass earlier/later in the chain (see
/// [`render_export_job`]).
pub fn resolve_shape_segments(
    sequence: &Sequence,
    canvas_width: u32,
    canvas_height: u32,
) -> Vec<avbridge::ShapeSegment> {
    let mut clips: Vec<&ShapeClip> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Shape)
        .flat_map(|t| &t.shape_clips)
        .collect();
    clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    clips
        .into_iter()
        .map(|clip| shape_clip_to_segment(clip, canvas_width, canvas_height))
        .collect()
}

/// Converts one [`ShapeClip`] into its [`avbridge::ShapeSegment`] — the actual geometry/color/
/// rotation math lives in [`crate::shape_render::build_shape_filter_desc`], this just adapts
/// field names/types across that boundary.
fn shape_clip_to_segment(
    clip: &ShapeClip,
    canvas_width: u32,
    canvas_height: u32,
) -> avbridge::ShapeSegment {
    let filter_desc = crate::shape_render::build_shape_filter_desc(&crate::ShapeRenderInput {
        shape_kind: &clip.shape_kind,
        center_x: clip.center_x,
        center_y: clip.center_y,
        center_x_keyframes: &clip.center_x_keyframes,
        center_y_keyframes: &clip.center_y_keyframes,
        width: clip.width,
        height: clip.height,
        width_keyframes: &clip.width_keyframes,
        height_keyframes: &clip.height_keyframes,
        rotation_deg: clip.rotation_deg,
        rotation_keyframes: &clip.rotation_keyframes,
        color_rgba: clip.color_rgba,
        stroke_thickness_px: clip.stroke_thickness_px,
        start_secs: clip.start_secs,
        duration_secs: clip.duration_secs,
        canvas_width,
        canvas_height,
    });
    avbridge::ShapeSegment { filter_desc }
}

/// Resolves all visible video tracks in `sequence` into per-track segment lists, suitable for
/// passing to [`render_export_job_multi`].  The returned `Vec` has one inner `Vec<ClipSegment>`
/// per visible video track that has at least one clip; the canvas is derived from the first
/// clip of the first track (same logic as [`resolve_timeline_segments`]).
///
/// When the sequence has exactly one visible video track, the result is a single-element outer
/// `Vec` that round-trips cleanly through [`render_export_job_multi`] (which delegates to
/// [`render_export_job`] in that case).
///
/// Fails with [`RenderError::EmptyTimeline`] if there are no visible video tracks with clips,
/// [`RenderError::MissingAsset`] if any clip's `asset_id` isn't in `media_library`.
pub fn resolve_timeline_segments_multi(
    sequence: &Sequence,
    media_library: &[MediaAsset],
) -> Result<(Vec<Vec<avbridge::ClipSegment>>, Canvas), RenderError> {
    let visible_video_tracks: Vec<_> = sequence
        .timeline
        .tracks
        .iter()
        .filter(|t| t.kind == TrackKind::Video && t.visible && !t.clips.is_empty())
        .collect();

    if visible_video_tracks.is_empty() {
        return Err(RenderError::EmptyTimeline);
    }

    let mut all_track_segments: Vec<Vec<avbridge::ClipSegment>> = Vec::new();
    let mut canvas: Option<Canvas> = None;

    for track in &visible_video_tracks {
        // Track 0 (background) vs 1+ (overlay) — matches avbridge_encode_timeline_export_multi's
        // own convention and the same distinction position_keyframes/opacity_keyframes already
        // rely on: `all_track_segments.len()` is this track's index since it fills in order.
        let is_overlay = !all_track_segments.is_empty();
        let mut clips: Vec<_> = track.clips.iter().collect();
        clips.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));

        let mut segments = Vec::with_capacity(clips.len());
        let mut total_duration_secs = 0.0;
        let mut weighted_bitrate_bps_secs = 0.0;
        let mut dimensions_fps = None;

        for clip in &clips {
            let asset = media_library
                .iter()
                .find(|a| a.id == clip.asset_id)
                .ok_or(RenderError::MissingAsset)?;
            if dimensions_fps.is_none() {
                let (width, height) = asset.resolution.ok_or(RenderError::NoVideoStream)?;
                dimensions_fps = Some((width, height, asset.fps.unwrap_or(30.0)));
            }
            let duration_secs = clip.duration_secs();
            total_duration_secs += duration_secs;
            weighted_bitrate_bps_secs +=
                asset.source_bitrate_mbps as f64 * 1_000_000.0 * duration_secs;
            let (_, _, fps) = dimensions_fps.expect("just set above if it was None");
            let (fps_num, fps_den) = fps_to_rational(fps);
            let (video_filter, position_x_expr, position_y_expr) =
                resolve_clip_filters(clip, fps_num, fps_den, duration_secs, is_overlay);
            // Same overlay-only gate resolve_clip_filters' resize stage above uses (see its
            // doc comment) — a track-0/background clip's alpha has no compositing stage to
            // survive into (build_vfilter_descr's immediate format=yuv420p conform discards
            // it), so the matte is only worth decoding/alphamerging on an overlay track.
            let mask_video_path = if is_overlay && clip.background_removal_enabled {
                clip.background_removal_mask_path.clone()
            } else {
                String::new()
            };
            // Blend mode is only meaningful on an overlay track (track 1+) — a background
            // track's clip has no layer below it to blend against, same is_overlay gate
            // resolve_clip_filters' layer-scale resize stage uses above.
            let blend_mode = if is_overlay && clip.has_blend_mode() {
                clip.blend_mode.ffmpeg_name().to_string()
            } else {
                String::new()
            };
            segments.push(avbridge::ClipSegment {
                source_path: asset.source_path.clone(),
                source_in_secs: clip.source_in_secs,
                source_out_secs: clip.source_out_secs,
                gain_db: clip.gain_db,
                video_filter,
                frozen: clip.frozen,
                speed_factor: clip.speed_factor,
                smooth_speed_ramp_end_factor: clip.speed_ramp_end_factor.unwrap_or(0.0),
                position_x_expr,
                position_y_expr,
                transition_in: clip.transition_in.to_export_code(),
                transition_duration_secs: clip.transition_duration_secs,
                timeline_start_secs: clip.start_secs,
                mask_video_path,
                blend_mode,
                anchor_x: clip.anchor_x,
                anchor_y: clip.anchor_y,
            });
        }

        if canvas.is_none() {
            if let Some((width, height, fps)) = dimensions_fps {
                let (fps_num, fps_den) = fps_to_rational(fps);
                let bit_rate_bps = if total_duration_secs > 0.0 {
                    (weighted_bitrate_bps_secs / total_duration_secs) as i64
                } else {
                    0
                };
                canvas = Some(Canvas {
                    width,
                    height,
                    fps_num,
                    fps_den,
                    bit_rate_bps,
                });
            }
        }

        all_track_segments.push(segments);
    }

    let canvas = canvas.ok_or(RenderError::EmptyTimeline)?;
    Ok((all_track_segments, canvas))
}

/// Renders `track_segments` (one `Vec<ClipSegment>` per visible video track, as returned by
/// [`resolve_timeline_segments_multi`]) as one continuous composited file.
///
/// When `track_segments.len() == 1`, delegates to [`render_export_job`] — no new C code is
/// exercised.  When there are two or more tracks, calls
/// `avbridge::encode_timeline_export_multi` which composites every active layer via a dynamic
/// avfilter `overlay` chain. Track order is z-order: later tracks appear above earlier tracks.
///
/// `on_progress` and `cancel` have the same contract as [`render_export_job`].
#[allow(clippy::too_many_arguments)]
pub fn render_export_job_multi(
    track_segments: &[Vec<avbridge::ClipSegment>],
    canvas: Canvas,
    output: &Path,
    target_lufs: f32,
    gpu_encoder: avbridge::GpuEncoderPreference,
    text_segments: &[TextSegment],
    shape_segments: &[avbridge::ShapeSegment],
    privacy_blur_segments: &[PrivacyBlurSegment],
    cancel: &AtomicBool,
    on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    render_export_job_multi_with_audio(
        track_segments,
        &[],
        canvas,
        output,
        target_lufs,
        gpu_encoder,
        text_segments,
        shape_segments,
        privacy_blur_segments,
        cancel,
        on_progress,
    )
}

/// Multi-track render with an optional complete audio snapshot. When `audio_segments` is empty,
/// this is byte-for-byte the established render path. Otherwise the completed video's audio is
/// replaced by a native `amix` result after visual post-processing, stream-copying video so the
/// extra pass never reduces image quality.
#[allow(clippy::too_many_arguments)]
pub fn render_export_job_multi_with_audio(
    track_segments: &[Vec<avbridge::ClipSegment>],
    audio_segments: &[avbridge::AudioSegment],
    canvas: Canvas,
    output: &Path,
    target_lufs: f32,
    gpu_encoder: avbridge::GpuEncoderPreference,
    text_segments: &[TextSegment],
    shape_segments: &[avbridge::ShapeSegment],
    privacy_blur_segments: &[PrivacyBlurSegment],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    validate_timeline_segments(track_segments, canvas, target_lufs)?;
    if track_segments.len() == 1 {
        let outcome = render_export_job(
            &track_segments[0],
            canvas,
            output,
            target_lufs,
            gpu_encoder,
            text_segments,
            shape_segments,
            privacy_blur_segments,
            cancel,
            on_progress,
        )?;
        return if outcome == RenderOutcome::Completed && !audio_segments.is_empty() {
            apply_audio_mix_pass(
                output,
                audio_segments,
                timeline_duration(track_segments),
                target_lufs,
                cancel,
            )
        } else {
            Ok(outcome)
        };
    }

    // Total output duration from track 0 (the primary / audio track).
    let total_duration_secs: f64 = track_segments[0]
        .iter()
        .map(clip_segment_duration_secs)
        .sum();

    let outcome = avbridge::encode_timeline_export_multi(
        track_segments,
        canvas,
        output,
        target_lufs,
        gpu_encoder,
        cancel,
        |secs| {
            if total_duration_secs > 0.0 {
                let percent = ((secs / total_duration_secs) * 100.0).clamp(0.0, 100.0) as u8;
                on_progress(percent);
            }
        },
    )?;

    Ok(match outcome {
        avbridge::EncodeOutcome::Completed => {
            on_progress(100);
            if !text_segments.is_empty() {
                apply_text_overlay_pass(output, canvas, text_segments);
            }
            if !shape_segments.is_empty() {
                apply_shape_overlay_pass(output, canvas, shape_segments);
            }
            if !privacy_blur_segments.is_empty() {
                apply_privacy_blur_pass(output, canvas, total_duration_secs, privacy_blur_segments);
            }
            if !audio_segments.is_empty() {
                return apply_audio_mix_pass(
                    output,
                    audio_segments,
                    total_duration_secs,
                    target_lufs,
                    cancel,
                );
            }
            RenderOutcome::Completed
        }
        avbridge::EncodeOutcome::Cancelled => RenderOutcome::Cancelled,
    })
}

fn timeline_duration(track_segments: &[Vec<avbridge::ClipSegment>]) -> f64 {
    track_segments
        .first()
        .map(|track| {
            track
                .iter()
                .map(|segment| {
                    (segment.source_out_secs - segment.source_in_secs)
                        / (segment.speed_factor as f64).max(0.0001)
                })
                .sum()
        })
        .unwrap_or(0.0)
}

fn apply_audio_mix_pass(
    output: &Path,
    audio_segments: &[avbridge::AudioSegment],
    timeline_duration_secs: f64,
    target_lufs: f32,
    cancel: &AtomicBool,
) -> Result<RenderOutcome, RenderError> {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("oca_export");
    let temp_id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
    let suffix = format!("{}-{temp_id}", std::process::id());
    let audio_tmp = parent.join(format!("{stem}.oca-audio-mix-{suffix}.m4a"));
    let mux_tmp = parent.join(format!("{stem}.oca-audio-mux-{suffix}.mp4"));
    let backup = parent.join(format!("{stem}.oca-audio-original-{suffix}.mp4"));

    let mix_outcome = match avbridge::mix_audio_timeline(
        audio_segments,
        timeline_duration_secs,
        &audio_tmp,
        target_lufs,
        cancel,
    ) {
        Ok(outcome) => outcome,
        Err(error) => {
            let _ = std::fs::remove_file(&audio_tmp);
            return Err(RenderError::AudioMix(error.to_string()));
        }
    };
    if mix_outcome == avbridge::AudioMixOutcome::Cancelled {
        let _ = std::fs::remove_file(&audio_tmp);
        let _ = std::fs::remove_file(output);
        return Ok(RenderOutcome::Cancelled);
    }
    if let Err(error) = avbridge::mux_video_audio(output, &audio_tmp, &mux_tmp) {
        let _ = std::fs::remove_file(&audio_tmp);
        let _ = std::fs::remove_file(&mux_tmp);
        return Err(RenderError::MediaMux(error.to_string()));
    }
    let _ = std::fs::remove_file(&audio_tmp);

    if let Err(error) = std::fs::rename(output, &backup) {
        let _ = std::fs::remove_file(&mux_tmp);
        return Err(RenderError::ReplaceOutput(error));
    }
    if let Err(error) = std::fs::rename(&mux_tmp, output) {
        let _ = std::fs::rename(&backup, output);
        let _ = std::fs::remove_file(&mux_tmp);
        return Err(RenderError::ReplaceOutput(error));
    }
    let _ = std::fs::remove_file(&backup);
    Ok(RenderOutcome::Completed)
}

/// Approximates `fps` as a small integer ratio for `avbridge::Canvas` — exact for whole frame
/// rates (`30.0` -> `30/1`), otherwise a fixed-point fraction (`29.97` -> `29970/1000`) rather
/// than a true lowest-terms rational (e.g. NTSC's canonical `30000/1001`), which is precise
/// enough for a canvas frame rate and doesn't need a full rational-approximation algorithm.
fn fps_to_rational(fps: f32) -> (u32, u32) {
    if !fps.is_finite() || fps <= 0.0 {
        return (30, 1);
    }
    let rounded = fps.round();
    if (fps - rounded).abs() < 0.001 {
        (rounded as u32, 1)
    } else {
        ((fps * 1000.0).round() as u32, 1000)
    }
}

#[cfg(test)]
#[path = "render/render_test.rs"]
mod tests;
