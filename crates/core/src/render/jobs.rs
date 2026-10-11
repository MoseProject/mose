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
use super::text::{
    apply_text_overlay_pass, rasterize_text_overlays, resolve_text_segments, TextSegment,
};
use super::validation::{
    validate_audio_segments, validate_segment_list, validate_target_lufs, validate_text_segments,
    validate_timeline_segments,
};
use crate::media::MediaAsset;
use crate::project::Sequence;
use avbridge::Canvas;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// Pass as `target_lufs` to export without loudness normalization: the audio keeps its mix and
/// gains, with only the true-peak safety limiter applied.
pub const NO_NORMALIZATION: f32 = f32::NAN;

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

    // Text and shape overlays run inside the export, over each frame before it is encoded, rather
    // than as a second decode, filter and encode of the whole finished file (which doubled the
    // time and re-encoded the picture). If those graphs do not build, the passes below still do
    // the job the old way.
    let text_rasters = if text_segments.is_empty() {
        None
    } else {
        rasterize_text_overlays(output, canvas, text_segments)
    };
    let mut post_filters = Vec::new();
    if let Some(rasters) = &text_rasters {
        if let Ok(Some(filter)) = avbridge::text_overlay_filter(&rasters.overlays) {
            post_filters.push(filter);
        }
    }
    if let Ok(Some(filter)) = avbridge::shape_overlay_filter(shape_segments) {
        post_filters.push(filter);
    }
    // Both must have built, or the passes below take over for whichever did not.
    let expected_filters =
        usize::from(!text_segments.is_empty()) + usize::from(!shape_segments.is_empty());
    let mut overlays_inline = !post_filters.is_empty() && post_filters.len() == expected_filters;
    if !overlays_inline {
        post_filters.clear();
    }

    let mut progress = |secs: f64| {
        if total_duration_secs > 0.0 {
            let percent = ((secs / total_duration_secs) * 100.0).clamp(0.0, 100.0) as u8;
            on_progress(percent);
        }
    };
    let mut encode = |post: &[String]| {
        avbridge::encode_timeline_export_with_post(
            segments,
            canvas,
            output,
            target_lufs,
            gpu_encoder,
            cancel,
            post,
            &mut progress,
        )
    };
    let outcome = match encode(&post_filters) {
        Err(avbridge::EncodeError::FilterGraph) if overlays_inline => {
            tracing::warn!(output = %output.display(), "overlay graphs did not build inside the export; using separate passes");
            overlays_inline = false;
            encode(&[])?
        }
        other => other?,
    };
    drop(text_rasters);

    Ok(match outcome {
        avbridge::EncodeOutcome::Completed => {
            on_progress(100);
            // Overlays that did not run inside the export go over the finished file as
            // post-processing passes. Shapes after text so a highlight box can sit visually above
            // a caption if the user stacks them at the same position — an arbitrary but
            // consistent choice, same as any other z-order tie-break. Privacy blur last: it
            // should blur whatever text/shapes already ended up inside the tracked region too,
            // not sit underneath them.
            if !overlays_inline {
                if !text_segments.is_empty() {
                    apply_text_overlay_pass(output, canvas, text_segments);
                }
                if !shape_segments.is_empty() {
                    apply_shape_overlay_pass(output, canvas, shape_segments);
                }
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
    if let Some(done) = try_lossless_cut(
        track_segments,
        audio_segments,
        canvas,
        output,
        target_lufs,
        text_segments,
        shape_segments,
        privacy_blur_segments,
        cancel,
        &mut on_progress,
    ) {
        return done;
    }
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

/// One clip of an H.264 file at the canvas's size and frame rate, with no effect on it, cut
/// somewhere in the middle: the picture between the keyframes next to the cut is copied as it is
/// and only the two ends are encoded again ([`avbridge::smart_cut_video`]), then the audio is
/// mixed in. That is a frame-accurate cut in the time it takes to read the file, with the source's
/// own bits for most of the picture. Returns `None`, having changed nothing, whenever the export
/// does not suit that (or anything in it fails), and the usual re-encode takes over.
#[allow(clippy::too_many_arguments)]
fn try_lossless_cut(
    track_segments: &[Vec<avbridge::ClipSegment>],
    audio_segments: &[avbridge::AudioSegment],
    canvas: Canvas,
    output: &Path,
    target_lufs: f32,
    text_segments: &[TextSegment],
    shape_segments: &[avbridge::ShapeSegment],
    privacy_blur_segments: &[PrivacyBlurSegment],
    cancel: &AtomicBool,
    on_progress: &mut impl FnMut(u8),
) -> Option<Result<RenderOutcome, RenderError>> {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);

    if track_segments.len() != 1
        || track_segments[0].len() != 1
        || !text_segments.is_empty()
        || !shape_segments.is_empty()
        || !privacy_blur_segments.is_empty()
    {
        return None;
    }
    let segment = &track_segments[0][0];
    let untouched = segment.video_filter.is_empty()
        && !segment.frozen
        && segment.speed_factor == 1.0
        && segment.smooth_speed_ramp_end_factor <= 0.0
        && segment.transition_in == 0
        && segment.fade_in_secs == 0.0
        && segment.fade_out_secs == 0.0
        && segment.blend_mode.is_empty()
        && segment.mask_video_path.is_empty()
        && segment.position_x_expr.is_empty()
        && segment.position_y_expr.is_empty();
    if !untouched {
        return None;
    }
    // The same size and frame rate as the export's canvas, so nothing has to be scaled or
    // resampled, and a sound to carry along.
    let info = avbridge::probe(&segment.source_path).ok()?;
    let same_picture = info.codec_name == "h264"
        && info.has_audio
        && info.resolution == Some((canvas.width, canvas.height))
        && info.fps.is_some_and(|fps| {
            (f64::from(fps) - f64::from(canvas.fps_num) / f64::from(canvas.fps_den.max(1))).abs()
                < 0.01
        });
    if !same_picture {
        return None;
    }

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let stem = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("oca_export");
    let suffix = format!(
        "{}-{}",
        std::process::id(),
        TEMP_ID.fetch_add(1, Ordering::Relaxed)
    );
    let video = parent.join(format!("{stem}.oca-cut-{suffix}.mp4"));
    let length = segment.source_out_secs - segment.source_in_secs;

    match avbridge::smart_cut_video(
        &segment.source_path,
        segment.source_in_secs,
        segment.source_out_secs,
        canvas,
        &video,
        cancel,
        |secs| {
            if length > 0.0 {
                // The copy is most of the work; the audio is the rest.
                on_progress(((secs / length) * 90.0).clamp(0.0, 90.0) as u8);
            }
        },
    ) {
        Ok(avbridge::SmartCutOutcome::Done) => {}
        Ok(avbridge::SmartCutOutcome::NotApplicable) => return None,
        Ok(avbridge::SmartCutOutcome::Cancelled) => {
            let _ = std::fs::remove_file(&video);
            return Some(Ok(RenderOutcome::Cancelled));
        }
        Err(error) => {
            tracing::warn!(%error, output = %output.display(), "lossless cut failed; exporting the usual way");
            let _ = std::fs::remove_file(&video);
            return None;
        }
    }

    let plain_audio;
    let audio = if audio_segments.is_empty() {
        plain_audio = [avbridge::AudioSegment::plain(
            segment.source_path.clone(),
            segment.source_in_secs,
            segment.source_out_secs,
            segment.gain_db,
        )];
        &plain_audio[..]
    } else {
        audio_segments
    };
    match apply_audio_mix_pass(&video, audio, length, target_lufs, cancel) {
        Ok(RenderOutcome::Completed) => {}
        Ok(RenderOutcome::Cancelled) => {
            let _ = std::fs::remove_file(&video);
            return Some(Ok(RenderOutcome::Cancelled));
        }
        Err(error) => {
            tracing::warn!(%error, output = %output.display(), "audio for the lossless cut failed; exporting the usual way");
            let _ = std::fs::remove_file(&video);
            return None;
        }
    }
    if let Err(error) = std::fs::rename(&video, output) {
        tracing::warn!(%error, output = %output.display(), "could not place the lossless cut; exporting the usual way");
        let _ = std::fs::remove_file(&video);
        return None;
    }
    on_progress(100);
    Some(Ok(RenderOutcome::Completed))
}
