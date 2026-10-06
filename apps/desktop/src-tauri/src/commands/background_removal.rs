//! AI background-removal matte generation -- background-thread ONNX segmentation for the
//! selected clip, mirroring `ui::App::spawn_generate_matte_for_selected_clip` exactly (same
//! sampling rate/cap, same `avcore::segment_person`/`encode_matte_video` pipeline), no core
//! changes needed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::analysis_job::{AnalysisPermit, AnalysisToken};
use crate::services::Services;
use crate::state::AppState;

pub const MATTE_GENERATION_EVENT: &str = "matte-generation-event";
const SAMPLES_PER_SEC: f64 = 2.0;
const MAX_SAMPLES: usize = 40;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MatteGenerationResult {
    Done { clip_id: u64 },
    Failed { clip_id: u64, message: String },
}

/// Runs AI background-removal matte generation against `clip_id` on a background thread -- what
/// the properties panel's "Gerar máscara" button does. Errors (already running, no model
/// configured, clip/asset not found) surface synchronously; the mask path is applied
/// automatically on success, and either outcome is reported via [`MATTE_GENERATION_EVENT`].
#[tauri::command]
pub fn generate_matte_for_clip<R: Runtime>(app: AppHandle<R>, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    let permit = AnalysisPermit::acquire(&app)?;
    let token = AnalysisToken::capture(&state, Some(clip_id), None);
    {
        let mut running = state
            .matte_generating_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("a matte generation is already running".to_string());
        }
        *running = Some(clip_id);
    }
    let clear_running = |state: &AppState| {
        *state
            .matte_generating_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    };
    let model_path = state.prefs().view.background_removal_model_path.clone();
    if model_path.trim().is_empty() {
        clear_running(&state);
        return Err("no background-removal model configured".to_string());
    }
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

    let model_path = PathBuf::from(model_path);

    let worker_app = app.clone();
    let services = state.services.clone();
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
                let _ = worker_app.emit(
                    MATTE_GENERATION_EVENT,
                    MatteGenerationResult::Failed { clip_id, message },
                );
                return;
            }
        };
        let mask_path = staged.to_path_buf();
        let result = generate_matte_one(
            services.as_ref(),
            &source_path,
            clip.source_in_secs,
            clip.source_out_secs,
            &model_path,
            &mask_dir,
            &mask_path,
        );
        {
            let state = worker_app.state::<AppState>();
            clear_running(&state);
        }
        let event = match result {
            Ok(mask_path) => {
                if let Err(error) = staged.keep() {
                    let _ = worker_app.emit(
                        MATTE_GENERATION_EVENT,
                        MatteGenerationResult::Failed {
                            clip_id,
                            message: error.to_string(),
                        },
                    );
                    return;
                }
                let applied = token.apply(&worker_app, |project| {
                    if let Some(clip) = project.timeline_mut().clip_mut(clip_id) {
                        clip.background_removal_mask_path = mask_path.display().to_string();
                        clip.background_removal_enabled = true;
                    }
                });
                if let Err(message) = applied {
                    let _ = std::fs::remove_file(&mask_path);
                    let _ = worker_app.emit(
                        MATTE_GENERATION_EVENT,
                        MatteGenerationResult::Failed { clip_id, message },
                    );
                    return;
                }
                MatteGenerationResult::Done { clip_id }
            }
            Err(message) => {
                tracing::error!(clip_id, %message, "matte generation failed");
                MatteGenerationResult::Failed { clip_id, message }
            }
        };
        let _ = worker_app.emit(MATTE_GENERATION_EVENT, event);
    });
    Ok(())
}

fn generate_matte_one(
    services: &dyn Services,
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    model_path: &Path,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<PathBuf, String> {
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

    let mut dimensions = None;
    let mut luma_frames = Vec::new();
    for t in sample_times {
        let Some(frame) = sampler.sample_scaled(t, Duration::from_millis(1500), 320) else {
            continue;
        };
        let size = (frame.width, frame.height);
        if dimensions.is_some_and(|d| d != size) {
            continue;
        }
        dimensions = Some(size);
        let alpha = services
            .segment_person(model_path, &frame.rgba, frame.width, frame.height)
            .map_err(|e| format!("segmentation failed: {e}"))?;
        if alpha.len() != frame.width as usize * frame.height as usize {
            return Err("invalid segmentation dimensions".into());
        }
        luma_frames.push(
            alpha
                .iter()
                .map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        );
    }
    let (w, h) = dimensions.unwrap_or((0, 0));
    encode_masks(luma_frames, w, h, duration, mask_dir, mask_path)
}

/// Segments the people in already decoded frames and encodes the matte video. Frames whose size
/// differs from the first one's are skipped.
#[cfg(test)]
fn matte_from_frames(
    services: &dyn Services,
    frames: &[avcore::SampledFrame],
    duration: f64,
    model_path: &Path,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<PathBuf, String> {
    let (frame_w, frame_h) = frames.first().map_or((0, 0), |f| (f.width, f.height));
    let mut luma_frames: Vec<Vec<u8>> = Vec::with_capacity(frames.len());
    for frame in frames
        .iter()
        .filter(|f| f.width == frame_w && f.height == frame_h)
    {
        let alpha = services
            .segment_person(model_path, &frame.rgba, frame.width, frame.height)
            .map_err(|e| format!("segmentation failed: {e}"))?;
        luma_frames.push(
            alpha
                .iter()
                .map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        );
    }
    encode_masks(luma_frames, frame_w, frame_h, duration, mask_dir, mask_path)
}

fn encode_masks(
    luma_frames: Vec<Vec<u8>>,
    frame_w: u32,
    frame_h: u32,
    duration: f64,
    mask_dir: &Path,
    mask_path: &Path,
) -> Result<PathBuf, String> {
    if luma_frames.len() < 2 {
        return Err("couldn't decode enough frames to generate a matte".to_string());
    }

    let effective_fps = luma_frames.len() as f64 / duration;
    let fps_num = ((effective_fps * 1000.0).round() as u32).max(1);
    let fps_den = 1000u32;

    std::fs::create_dir_all(mask_dir)
        .map_err(|e| format!("failed to create matte cache dir: {e}"))?;
    avcore::encode_matte_video(&luma_frames, frame_w, frame_h, fps_num, fps_den, mask_path)
        .map_err(|e| format!("failed to encode matte video: {e}"))?;

    Ok(mask_path.to_path_buf())
}

#[cfg(test)]
#[path = "background_removal_test.rs"]
mod tests;
