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

use super::errors::{RenderError, RenderOutcome};
use super::privacy::{apply_privacy_blur_pass, resolve_privacy_blur_segments, PrivacyBlurSegment};
use super::segments::{
    clip_segment_duration_secs, resolve_audio_segments, resolve_timeline_segments_multi,
    timeline_duration,
};
use super::shape::{apply_shape_overlay_pass, resolve_shape_segments};
use super::text::{apply_text_overlay_pass, resolve_text_segments, TextSegment};
use super::validation::{
    validate_audio_segments, validate_segment_list, validate_target_lufs, validate_text_segments,
    validate_timeline_segments,
};
use crate::media::MediaAsset;
use crate::project::Sequence;
use avbridge::Canvas;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
    validate_text_segments(text_segments, canvas)?;
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
    validate_text_segments(text_segments, canvas)?;
    validate_audio_segments(audio_segments)?;
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

pub(crate) fn apply_audio_mix_pass(
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
