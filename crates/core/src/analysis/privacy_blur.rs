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

//! CF-09 (`spec/architecture/competitive-feature-plan.md`)'s slice 4 — "Support privacy blur as
//! the first end-to-end effect before general selective effects." A thin `avcore` wrapper over
//! `avbridge::apply_privacy_blur`'s new post-process pass (same "opens an already-rendered
//! export, writes a modified copy" shape `avcore::export`'s existing text/shape overlay
//! composition already uses), which blurs an already-rendered export everywhere a matte video's
//! own luma is non-zero.
//!
//! Ties together this session's two other CF-09 pieces: [`crate::mask_propagation::
//! rasterize_to_matte_frames`] turns a propagated arbitrary-object mask into per-frame matte
//! buffers, [`crate::background_removal::encode_matte_video`] (reused — not a second matte
//! encoder) turns those into a matte video file, and [`apply_privacy_blur`] here consumes that
//! file to actually blur the tracked region. **Real end-to-end runtime behavior is unverified in
//! this sandbox** — this repo's own too-old packaged FFmpeg build can't compile the rest of
//! `avbridge` at all (see `CLAUDE.md`), so this new C function has only been syntax-checked
//! per-file (`gcc -fsyntax-only`) against the real FFmpeg headers, never actually run against a
//! real video. Treat the filter-graph string itself (`gblur`+`maskedmerge`, see `privacy_blur.c`)
//! as code-reviewed, not execution-verified.

use std::path::Path;

/// Pads a clip-local matte (one grayscale-as-luma buffer per sampled frame, same shape
/// [`crate::mask_propagation::rasterize_to_matte_frames`] produces and
/// [`crate::background_removal::encode_matte_video`] consumes) out to cover a whole export's own
/// canvas-duration timeline, so a single [`apply_privacy_blur`] call blurs only during the clip's
/// own on-timeline window rather than from the start of the export.
///
/// `avbridge_apply_privacy_blur` has no per-segment timeline window the way
/// `avbridge_apply_text_overlays`/`_shape_overlays` do (`start_secs`/`duration_secs` gated by
/// `enable='between(t,start,end)'`) — extending it to support that needs confirming FFmpeg's
/// `maskedmerge` actually honors the timeline `enable` option in this pinned build, which can't
/// be verified in this sandbox (see `spec/architecture/competitive-feature-plan.md`'s CF-09 "UI
/// integration design" section for the full reasoning). Padding the matte itself sidesteps that
/// uncertainty entirely: black (all-zero) frames before/after the clip's own window mean
/// `maskedmerge` sees a fully-zero mask there regardless of whether `enable=` would have worked,
/// using only machinery already shipped and verified (`encode_matte_video`'s own byte format).
///
/// `frame_bytes` is one padding frame's size (`width * height`, matching every entry in
/// `clip_matte_frames`) — an all-zero buffer of this size is what "no blur" looks like to
/// `maskedmerge`. `fps_num`/`fps_den` is the *canvas* export's own frame rate (not the clip
/// matte's own possibly-coarser sampling rate) — every frame this function returns, padding
/// included, is meant to be encoded at that rate so the padded matte's total duration lines up
/// with the export's own.
///
/// Frame counts are computed by rounding each boundary's own `seconds * (fps_num / fps_den)`
/// rather than accumulating per-frame durations, so padding length is stable regardless of how
/// many frames `clip_matte_frames` itself has. `clip_start_secs`/`clip_duration_secs` clamped to
/// `[0, canvas_duration_secs]` first (a clip starting at a negative offset or extending past the
/// canvas's own end — shouldn't happen from a real timeline, but this function stays total over
/// it rather than underflowing a `usize` subtraction).
pub fn pad_matte_frames_to_canvas_duration(
    clip_matte_frames: &[Vec<u8>],
    frame_bytes: usize,
    fps_num: u32,
    fps_den: u32,
    clip_start_secs: f64,
    clip_duration_secs: f64,
    canvas_duration_secs: f64,
) -> Vec<Vec<u8>> {
    let canvas_duration_secs = canvas_duration_secs.max(0.0);
    let clip_start_secs = clip_start_secs.clamp(0.0, canvas_duration_secs);
    let clip_end_secs = (clip_start_secs + clip_duration_secs.max(0.0))
        .clamp(clip_start_secs, canvas_duration_secs);

    let fps = if fps_den > 0 {
        fps_num as f64 / fps_den as f64
    } else {
        0.0
    };
    let frames_before = (clip_start_secs * fps).round().max(0.0) as usize;
    let frames_after = ((canvas_duration_secs - clip_end_secs) * fps)
        .round()
        .max(0.0) as usize;

    let black_frame = vec![0u8; frame_bytes];
    let mut out = Vec::with_capacity(frames_before + clip_matte_frames.len() + frames_after);
    out.extend(std::iter::repeat_n(black_frame.clone(), frames_before));
    out.extend_from_slice(clip_matte_frames);
    out.extend(std::iter::repeat_n(black_frame, frames_after));
    out
}

#[cfg(test)]
#[path = "privacy_blur/privacy_blur_test.rs"]
mod tests;

/// What [`apply_privacy_blur`] failed on — thin wrapper over [`avbridge::PrivacyBlurError`],
/// same pattern [`crate::background_removal::MatteEncodeError`] already uses for
/// `avbridge::MatteError`.
#[derive(Debug)]
pub enum PrivacyBlurError {
    Bridge(avbridge::PrivacyBlurError),
}

impl std::fmt::Display for PrivacyBlurError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrivacyBlurError::Bridge(e) => write!(f, "failed to apply privacy blur: {e}"),
        }
    }
}

impl std::error::Error for PrivacyBlurError {}

/// Blurs `in_path` (an already-rendered export) everywhere `matte_path`'s own per-frame luma is
/// non-zero, writing the result to `out_path`. See [`avbridge::apply_privacy_blur`]'s doc
/// comment for the exact filter graph and the `matte_path`/timing contract (it must already be
/// encoded at `canvas_fps_num`/`_den`, e.g. via [`crate::background_removal::
/// encode_matte_video`]).
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
    avbridge::apply_privacy_blur(
        in_path,
        out_path,
        matte_path,
        blur_sigma,
        canvas_width,
        canvas_height,
        fps_num,
        fps_den,
    )
    .map_err(PrivacyBlurError::Bridge)
}
