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

//! AI background removal — per `request.md`'s Fase 4 "Remoção de fundo por IA": cut the person
//! out of the background without a green screen, complementing chroma key (which needs a real
//! green/chroma backdrop). Unlike chroma key's `colorkey` avfilter expression (a per-pixel
//! formula FFmpeg's filter graph can evaluate directly), a neural matte can't be expressed as an
//! avfilter string — [`segment_person`] runs the model against one decoded frame in Rust and
//! returns a plain alpha buffer; turning a whole clip's worth of per-frame mattes into an actual
//! alpha channel needed its own pipeline stage, built separately from this module: `ui`'s "Gerar
//! máscara" flow (`App::spawn_generate_matte_for_selected_clip`) samples frames across the
//! clip's trim range, runs [`segment_person`] on each, and encodes the mattes into a small H.264
//! video via [`encode_matte_video`], which export then composites in via an `alphamerge` stage
//! (see `crate::timeline::ClipInstance::background_removal_enabled`'s doc comment for the exact
//! export wiring). **Not yet wired into preview** — the same gap `mask_shape`/`chroma_key`
//! compositing has there.
//!
//! **Model:** MODNet (`ZHKKKe/MODNet`, "photographic" weights ported to ONNX by
//! `yakhyo/modnet`, Apache-2.0), shipped in the release bundle under `resources/models/`; the
//! application never downloads it at runtime. Preprocessing (resize so both
//! dimensions are multiples of 32 targeting ~512px on the constrained side, `(pixel/255 - 0.5) /
//! 0.5` normalization, RGB/CHW) and postprocessing (single-channel matte in `0.0..=1.0`, resized
//! back to the source frame) come from that repo's own `onnx_inference.py`/`modnet_onnx.py`
//! reference implementation, not guessed. **Confirmed working end-to-end** on this dev machine:
//! a real inference pass against the downloaded model succeeded against a synthetic frame with
//! a bright rectangle on a dark background, producing a plausible low-average matte (no real
//! person shape in the fixture, so a mostly-background result is expected).

use std::path::{Path, PathBuf};

use crate::project::Project;

/// MODNet is fully convolutional — any input size works, but the reference implementation
/// targets roughly this size on the constrained dimension (then rounds both dimensions down to
/// a multiple of 32, required by MODNet's downsampling stages) for a speed/quality balance.
const TARGET_SIZE: u32 = 512;

#[derive(Debug)]
pub enum SegmentError {
    Onnx(String),
}

impl std::fmt::Display for SegmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SegmentError::Onnx(e) => write!(f, "ONNX inference failed: {e}"),
        }
    }
}

impl std::error::Error for SegmentError {}

/// Picks the model's input resolution for a `source_w x source_h` frame: scales so the
/// constrained side lands near [`TARGET_SIZE`] (matching `modnet_onnx.py`'s `preprocess`), then
/// rounds each dimension down to the nearest multiple of 32 (MODNet's encoder downsamples by
/// 32x internally, so any other input size fails at a shape-mismatch layer). Never rounds down
/// to zero — a frame smaller than 32px on a side still gets a minimum of one 32px block.
fn model_input_size(source_w: u32, source_h: u32) -> (u32, u32) {
    let (mut new_w, mut new_h) =
        if source_w.max(source_h) < TARGET_SIZE || source_w.min(source_h) > TARGET_SIZE {
            if source_w >= source_h {
                let new_h = TARGET_SIZE;
                let new_w = source_w * TARGET_SIZE / source_h;
                (new_w, new_h)
            } else {
                let new_w = TARGET_SIZE;
                let new_h = source_h * TARGET_SIZE / source_w;
                (new_w, new_h)
            }
        } else {
            (source_w, source_h)
        };
    new_w -= new_w % 32;
    new_h -= new_h % 32;
    (new_w.max(32), new_h.max(32))
}

/// Nearest-neighbor resize of an RGBA buffer's RGB channels into MODNet's expected NCHW,
/// `(pixel/255 - 0.5) / 0.5`-normalized float layout at `(model_w, model_h)`.
fn preprocess(rgba: &[u8], width: u32, height: u32, model_w: u32, model_h: u32) -> Vec<f32> {
    let mut chw = vec![0f32; 3 * (model_w * model_h) as usize];
    let plane = (model_w * model_h) as usize;
    for y in 0..model_h {
        let src_y = (y * height / model_h).min(height - 1);
        for x in 0..model_w {
            let src_x = (x * width / model_w).min(width - 1);
            let src = ((src_y * width + src_x) * 4) as usize;
            let dst = (y * model_w + x) as usize;
            chw[dst] = (rgba[src] as f32 / 255.0 - 0.5) / 0.5;
            chw[plane + dst] = (rgba[src + 1] as f32 / 255.0 - 0.5) / 0.5;
            chw[2 * plane + dst] = (rgba[src + 2] as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    chw
}

/// Nearest-neighbor resize of a single-channel matte from `(model_w, model_h)` back up to
/// `(width, height)` — the source frame's own resolution.
fn resize_matte(matte: &[f32], model_w: u32, model_h: u32, width: u32, height: u32) -> Vec<f32> {
    let mut out = vec![0f32; (width * height) as usize];
    for y in 0..height {
        let src_y = (y * model_h / height).min(model_h - 1);
        for x in 0..width {
            let src_x = (x * model_w / width).min(model_w - 1);
            out[(y * width + x) as usize] = matte[(src_y * model_w + src_x) as usize];
        }
    }
    out
}

/// Runs MODNet against one decoded RGBA frame and returns a per-pixel alpha matte
/// (`width * height` values, `0.0` = background, `1.0` = foreground/person), same resolution as
/// the input frame. `model_path` is a MODNet ONNX file (see module docs for where to get one).
pub fn segment_person(
    model_path: &Path,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Result<Vec<f32>, SegmentError> {
    segment_person_with(rgba, width, height, |input, model_w, model_h| {
        crate::onnx_models::run_segmentation_model(model_path, input, model_w, model_h)
    })
}

/// [`segment_person`] with the model run passed in: it receives the preprocessed input and the
/// model's input size, and returns the model-resolution matte, so the resizing is testable
/// without a model.
pub(crate) fn segment_person_with(
    rgba: &[u8],
    width: u32,
    height: u32,
    run: impl FnOnce(Vec<f32>, u32, u32) -> Result<Vec<f32>, SegmentError>,
) -> Result<Vec<f32>, SegmentError> {
    let (model_w, model_h) = model_input_size(width, height);
    let matte = run(
        preprocess(rgba, width, height, model_w, model_h),
        model_w,
        model_h,
    )?;
    Ok(resize_matte(&matte, model_w, model_h, width, height))
}

/// Where a project's per-clip AI-background-removal alpha mattes are cached: a hidden sibling
/// folder next to the project file (`myproject.json` -> `.myproject_mattes/`), or a temp folder
/// for a project that hasn't been saved yet (mattes there won't survive a reboot, but neither
/// would anything else about an unsaved project) — same convention as
/// [`crate::proxy::cache_dir_for_project`].
pub fn mask_cache_dir_for_project(project: &Project) -> PathBuf {
    match &project.file_path {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            path.with_file_name(format!(".{stem}_mattes"))
        }
        None => std::env::temp_dir().join("oca_unsaved_mattes"),
    }
}

/// The matte video's path for `clip_id` inside `mask_dir`, without checking whether it exists
/// yet. One matte per *clip instance*, not per source asset (unlike
/// [`crate::proxy::proxy_path_for`]) — the matte is generated for this clip's own trimmed
/// `source_in_secs..source_out_secs` range and stops matching that range the moment it changes
/// (re-trim, split — see `crate::timeline::ClipInstance::background_removal_mask_path`'s doc
/// comment), so it can't be shared across clips the way one asset's proxy can.
pub fn mask_path_for_clip(clip_id: u64, mask_dir: &Path) -> PathBuf {
    mask_dir.join(format!("clip_{clip_id}_matte.mp4"))
}

#[derive(Debug)]
pub enum MatteEncodeError {
    Bridge(avbridge::MatteError),
}

impl std::fmt::Display for MatteEncodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatteEncodeError::Bridge(e) => write!(f, "failed to encode matte video: {e}"),
        }
    }
}

impl std::error::Error for MatteEncodeError {}

/// Encodes `frames` (one grayscale-as-luma alpha matte per sampled source frame — each exactly
/// `width * height` bytes, produced by rounding [`segment_person`]'s `0.0..=1.0` output to
/// `0..=255`) into a plain H.264 video at `out_path`, via `oca-avbridge`'s FFI. See
/// [`avbridge::encode_matte_video`] for exactly what this encodes (grayscale-as-luma, neutral
/// chroma — not a true single-plane format).
pub fn encode_matte_video(
    frames: &[Vec<u8>],
    width: u32,
    height: u32,
    fps_num: u32,
    fps_den: u32,
    out_path: &Path,
) -> Result<(), MatteEncodeError> {
    avbridge::encode_matte_video(frames, width, height, fps_num, fps_den, out_path)
        .map_err(MatteEncodeError::Bridge)
}

#[cfg(test)]
#[path = "background_removal/background_removal_test.rs"]
mod tests;
