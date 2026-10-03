//! AI background-removal matte generation -- background-thread ONNX segmentation for the
//! selected clip, mirroring `ui::App::spawn_generate_matte_for_selected_clip` exactly (same
//! sampling rate/cap, same `avcore::segment_person`/`encode_matte_video` pipeline), no core
//! changes needed.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::with_session;
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
    let clip = {
        let session = state.session();
        session
            .project
            .timeline()
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == clip_id)
            .cloned()
    };
    let Some(clip) = clip else {
        clear_running(&state);
        return Err("clip not found".to_string());
    };
    let (source_path, mask_dir) = {
        let session = state.session();
        let source_path = session
            .project
            .media_library
            .iter()
            .find(|a| a.id == clip.asset_id)
            .map(|a| a.source_path.clone());
        let mask_dir = avcore::background_removal::mask_cache_dir_for_project(&session.project);
        (source_path, mask_dir)
    };
    let Some(source_path) = source_path else {
        clear_running(&state);
        return Err("asset not found".to_string());
    };
    let mask_path = avcore::background_removal::mask_path_for_clip(clip_id, &mask_dir);
    let model_path = PathBuf::from(model_path);

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let result = generate_matte_one(
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
                let _ = with_session(&worker_app, |session| {
                    if let Some(clip) = session.project.timeline_mut().clip_mut(clip_id) {
                        clip.background_removal_mask_path = mask_path.display().to_string();
                        clip.background_removal_enabled = true;
                    }
                    session.dirty = true;
                    Ok(())
                });
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

    let mut luma_frames: Vec<Vec<u8>> = Vec::with_capacity(sample_times.len());
    let mut frame_w = 0u32;
    let mut frame_h = 0u32;
    for &t in &sample_times {
        let Some(frame) = sampler.sample(t, Duration::from_millis(1500)) else {
            continue;
        };
        if frame_w == 0 {
            frame_w = frame.width;
            frame_h = frame.height;
        } else if frame.width != frame_w || frame.height != frame_h {
            continue;
        }
        let alpha = avcore::segment_person(model_path, &frame.rgba, frame.width, frame.height)
            .map_err(|e| format!("segmentation failed: {e}"))?;
        luma_frames.push(
            alpha
                .iter()
                .map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8)
                .collect(),
        );
    }
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
