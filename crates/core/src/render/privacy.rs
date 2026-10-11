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

use crate::project::Sequence;
use crate::timeline::{ClipInstance, TrackKind};
use avbridge::Canvas;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

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
pub(crate) const MAX_PRIVACY_BLUR_MATTE_FRAMES: usize = 10_000;

/// A matte video built for one privacy-blur segment, deleted when dropped.
pub(crate) struct PaddedMatte(PathBuf);

impl PaddedMatte {
    pub(crate) fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for PaddedMatte {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Builds each segment's matte for the export to blur inside its own encode. Returns the mattes
/// of the segments up to the first one that does not build, so a caller comparing the count with
/// the segments' falls back to the pass for the lot.
pub(crate) fn privacy_blur_mattes(
    output: &Path,
    canvas: Canvas,
    canvas_duration_secs: f64,
    segments: &[PrivacyBlurSegment],
) -> Vec<PaddedMatte> {
    static TEMP_ID: AtomicU64 = AtomicU64::new(0);
    let (Some(parent), Some(stem)) = (output.parent(), output.file_stem().and_then(|s| s.to_str()))
    else {
        return Vec::new();
    };
    let mut mattes = Vec::new();
    for segment in segments {
        let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "{stem}.oca-privacy-blur-matte-{}-{id}.mp4",
            std::process::id()
        ));
        let built = build_padded_matte(
            Path::new(&segment.mask_path),
            &path,
            canvas.width,
            canvas.height,
            canvas.fps_num,
            canvas.fps_den,
            segment.start_secs,
            segment.duration_secs,
            canvas_duration_secs,
        );
        let matte = PaddedMatte(path);
        if let Err(error) = built {
            tracing::error!(%error, output = %output.display(), "privacy blur matte build failed");
            break;
        }
        mattes.push(matte);
    }
    mattes
}

/// Applies every privacy-blur segment to an already-written export file in place, one at a time
/// (each pass's own output feeds the next, same chaining [`apply_text_overlay_pass`]/
/// [`apply_shape_overlay_pass`] already use) — logs and skips a segment on any error rather than
/// failing the whole export, same "already-complete video, don't destroy it over a post-
/// processing issue" posture those two passes take.
pub(crate) fn apply_privacy_blur_pass(
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
pub(crate) fn apply_one_privacy_blur_segment(
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
pub(crate) fn build_padded_matte(
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
    let sampler = crate::frame_sampler::FrameSampler::open(matte_path)
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
