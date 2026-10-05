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

use super::errors::RenderError;
use super::text::TextSegment;
use avbridge::Canvas;

pub(crate) fn validate_target_lufs(target_lufs: f32) -> Result<(), RenderError> {
    target_lufs
        .is_finite()
        .then_some(())
        .ok_or(RenderError::InvalidTargetLufs)
}

pub(crate) fn validate_canvas(canvas: Canvas) -> Result<(), RenderError> {
    (canvas.width > 0 && canvas.height > 0 && canvas.fps_num > 0 && canvas.fps_den > 0)
        .then_some(())
        .ok_or(RenderError::InvalidCanvas)
}

pub(crate) fn valid_clip_segment(segment: &avbridge::ClipSegment) -> bool {
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

pub(crate) fn validate_segment_list(
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

pub(crate) fn validate_timeline_segments(
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

pub(crate) fn validate_text_segments(
    text_segments: &[TextSegment],
    canvas: Canvas,
) -> Result<(), RenderError> {
    text_segments.iter().try_for_each(|segment| {
        crate::overlay_render::validate_text_segment(segment, canvas.width, canvas.height)
            .map_err(|_| RenderError::InvalidTextLayout)
    })
}

pub(crate) fn valid_audio_segment(segment: &avbridge::AudioSegment) -> bool {
    !segment.source_path.as_os_str().is_empty()
        && segment.source_in_secs.is_finite()
        && segment.source_out_secs.is_finite()
        && segment.source_in_secs >= 0.0
        && segment.source_out_secs > segment.source_in_secs
        && segment.timeline_start_secs.is_finite()
        && segment.timeline_start_secs >= 0.0
        && segment.gain_db.is_finite()
        && segment.speed_factor.is_finite()
        && segment.speed_factor > 0.0
        && segment.duck_role <= 2
        && segment.voice_cleanup_noise_floor_db.is_finite()
        && segment.voice_cleanup_compressor_threshold_db.is_finite()
        && segment.voice_cleanup_compressor_ratio.is_finite()
        && segment.voice_cleanup_compressor_ratio > 0.0
        && segment.voice_cleanup_ceiling_linear.is_finite()
        && (0.0..=1.0).contains(&segment.voice_cleanup_ceiling_linear)
}

pub(crate) fn validate_audio_segments(
    audio_segments: &[avbridge::AudioSegment],
) -> Result<(), RenderError> {
    audio_segments
        .iter()
        .all(valid_audio_segment)
        .then_some(())
        .ok_or(RenderError::InvalidAudioSegment)
}
