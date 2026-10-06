//! CF-09 privacy-blur mask propagation -- background-thread block-matching mask tracking for
//! the selected clip, mirroring `ui::App::spawn_apply_privacy_blur_for_selected_clip` exactly
//! (same sampling rate/cap, same `avcore::mask_propagation` pipeline), no core changes needed.
//! The seed rectangle is passed in directly (center/width/height fractions) rather than picked
//! on-canvas -- same simplification the egui shell's own search-radius constant already takes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::analysis_job::{AnalysisPermit, AnalysisToken};
use crate::state::AppState;

pub const PRIVACY_BLUR_EVENT: &str = "privacy-blur-event";
const SAMPLES_PER_SEC: f64 = 4.0;
const MAX_SAMPLES: usize = 400;
const SEARCH_RADIUS_FRAC: f32 = 0.08;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum PrivacyBlurResult {
    Done { clip_id: u64 },
    Failed { clip_id: u64, message: String },
}

/// Runs CF-09 privacy-blur mask propagation against `clip_id` on a background thread -- what
/// the properties panel's "Aplicar blur" button does, seeding a rectangle centered at
/// `(center_x, center_y)` sized `width_frac`/`height_frac` (both fractions of the frame's
/// shorter dimension, `avcore::track_region`'s own convention). Errors (already running, clip/
/// asset not found) surface synchronously; the mask path and seed vertices are applied
/// automatically on success, reported via [`PRIVACY_BLUR_EVENT`].
#[tauri::command]
pub fn apply_privacy_blur_to_clip<R: Runtime>(
    app: AppHandle<R>,
    clip_id: u64,
    center_x: f32,
    center_y: f32,
    width_frac: f32,
    height_frac: f32,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let permit = AnalysisPermit::acquire(&app)?;
    let token = AnalysisToken::capture(&state, Some(clip_id), None);
    {
        let mut running = state
            .privacy_blur_generating_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("a privacy blur is already running".to_string());
        }
        *running = Some(clip_id);
    }
    let clear_running = |state: &AppState| {
        *state
            .privacy_blur_generating_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    };
    let clip = token.clip();
    let Some(clip) = clip else {
        clear_running(&state);
        return Err("clip not found".to_string());
    };
    let (source_path, mask_dir) = (token.source(), token.mask_dir.clone());
    let Some(source_path) = source_path else {
        clear_running(&state);
        return Err("asset not found".to_string());
    };

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let _permit = permit;
        if !token.is_current(&worker_app) {
            return;
        }
        let staged = std::fs::create_dir_all(&mask_dir).and_then(|_| {
            tempfile::Builder::new()
                .prefix("analysis_")
                .suffix(".mp4")
                .tempfile_in(&mask_dir)
        });
        let staged = match staged {
            Ok(file) => file.into_temp_path(),
            Err(error) => {
                let message = error.to_string();
                emit_privacy_failure(&worker_app, clip_id, message);
                return;
            }
        };
        let mask_path = staged.to_path_buf();
        let result = apply_privacy_blur_one(
            &source_path,
            clip.source_in_secs,
            clip.source_out_secs,
            center_x,
            center_y,
            width_frac,
            height_frac,
            &mask_dir,
            &mask_path,
        );
        {
            let state = worker_app.state::<AppState>();
            clear_running(&state);
        }
        let event = match result {
            Ok((mask_path, seed_vertices)) => {
                if let Err(error) = staged.keep() {
                    return emit_privacy_failure(&worker_app, clip_id, error.to_string());
                }
                let applied = token.apply(&worker_app, |project| {
                    if let Some(clip) = project.timeline_mut().clip_mut(clip_id) {
                        clip.privacy_blur_mask_path = mask_path.display().to_string();
                        clip.privacy_blur_enabled = true;
                        clip.privacy_blur_seed_vertices = seed_vertices;
                        clip.privacy_blur_seed_center_x_frac = center_x;
                        clip.privacy_blur_seed_center_y_frac = center_y;
                    }
                });
                if let Err(message) = applied {
                    let _ = std::fs::remove_file(&mask_path);
                    return emit_privacy_failure(&worker_app, clip_id, message);
                }
                PrivacyBlurResult::Done { clip_id }
            }
            Err(message) => {
                tracing::error!(clip_id, %message, "privacy blur matte generation failed");
                PrivacyBlurResult::Failed { clip_id, message }
            }
        };
        let _ = worker_app.emit(PRIVACY_BLUR_EVENT, event);
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_privacy_blur_one(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    center_x: f32,
    center_y: f32,
    width_frac: f32,
    height_frac: f32,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<(PathBuf, Vec<(f32, f32)>), String> {
    let duration = (source_out_secs - source_in_secs).max(0.0);
    if duration <= 0.0 {
        return Err("clip has zero duration".to_string());
    }
    let sample_times = avcore::FrameSampler::even_sample_times(
        source_in_secs,
        source_out_secs,
        SAMPLES_PER_SEC,
        2,
        MAX_SAMPLES,
    );
    let sampler = avcore::FrameSampler::open(source_path)
        .map_err(|e| format!("failed to open source for decoding: {e}"))?;

    let frames: Vec<avcore::GrayFrame> = sample_times
        .iter()
        .filter_map(|&t| {
            let frame = sampler.sample_scaled(t, Duration::from_millis(1500), 320)?;
            Some(avcore::rgba_to_gray(&frame.rgba, frame.width, frame.height))
        })
        .collect();
    matte_from_gray_frames(
        frames,
        duration,
        center_x,
        center_y,
        width_frac,
        height_frac,
        mask_dir,
        mask_path,
    )
}

/// Propagates the blur region through already decoded frames and encodes the matte video.
/// Frames whose size differs from the first one's are skipped.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn matte_from_frames(
    frames: &[avcore::SampledFrame],
    duration: f64,
    center_x: f32,
    center_y: f32,
    width_frac: f32,
    height_frac: f32,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<(PathBuf, Vec<(f32, f32)>), String> {
    let (frame_w, frame_h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let gray_frames: Vec<avcore::GrayFrame> = frames
        .iter()
        .filter(|f| f.width == frame_w && f.height == frame_h)
        .map(|f| avcore::rgba_to_gray(&f.rgba, f.width, f.height))
        .collect();
    matte_from_gray_frames(
        gray_frames,
        duration,
        center_x,
        center_y,
        width_frac,
        height_frac,
        mask_dir,
        mask_path,
    )
}

#[allow(clippy::too_many_arguments)]
fn matte_from_gray_frames(
    gray_frames: Vec<avcore::GrayFrame>,
    duration: f64,
    center_x: f32,
    center_y: f32,
    width_frac: f32,
    height_frac: f32,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<(PathBuf, Vec<(f32, f32)>), String> {
    let (frame_w, frame_h) = gray_frames.first().map_or((0, 0), |f| (f.width, f.height));
    let gray_frames: Vec<_> = gray_frames
        .into_iter()
        .filter(|f| f.width == frame_w && f.height == frame_h)
        .collect();
    if gray_frames.len() < 2 {
        return Err("couldn't decode enough frames to propagate a mask".to_string());
    }

    let short_side = frame_w.min(frame_h) as f32;
    let half_w_frac = (width_frac * short_side) / 2.0 / frame_w as f32;
    let half_h_frac = (height_frac * short_side) / 2.0 / frame_h as f32;
    let seed_vertices = vec![
        (center_x - half_w_frac, center_y - half_h_frac),
        (center_x + half_w_frac, center_y - half_h_frac),
        (center_x + half_w_frac, center_y + half_h_frac),
        (center_x - half_w_frac, center_y + half_h_frac),
    ];

    let propagated = avcore::mask_propagation::propagate_mask_by_translation(
        &seed_vertices,
        center_x,
        center_y,
        &gray_frames,
        width_frac,
        height_frac,
        SEARCH_RADIUS_FRAC,
        avcore::mask_propagation::DEFAULT_LOW_CONFIDENCE_THRESHOLD,
    );
    let luma_frames =
        avcore::mask_propagation::rasterize_to_matte_frames(&propagated, frame_w, frame_h);

    let effective_fps = luma_frames.len() as f64 / duration;
    let fps_num = ((effective_fps * 1000.0).round() as u32).max(1);
    let fps_den = 1000u32;

    std::fs::create_dir_all(mask_dir)
        .map_err(|e| format!("failed to create matte cache dir: {e}"))?;
    avcore::encode_matte_video(&luma_frames, frame_w, frame_h, fps_num, fps_den, mask_path)
        .map_err(|e| format!("failed to encode matte video: {e}"))?;

    Ok((mask_path.to_path_buf(), seed_vertices))
}

#[cfg(test)]
#[path = "privacy_blur_test.rs"]
mod tests;

fn emit_privacy_failure<R: Runtime>(app: &AppHandle<R>, clip_id: u64, message: String) {
    let _ = app.emit(
        PRIVACY_BLUR_EVENT,
        PrivacyBlurResult::Failed { clip_id, message },
    );
}
