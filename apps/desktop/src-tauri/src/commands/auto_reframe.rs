//! CF-04 static auto-reframe -- background-thread face detection + crop computation for the
//! selected clip, mirroring `ui::App::spawn_auto_reframe_selected_clip` exactly (same bundled/
//! configured UltraFace ONNX model, same seed-point bypass), plus the dynamic variant which
//! samples the clip and writes sparse crop keyframes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};

use super::with_session;
use crate::state::AppState;

pub const AUTO_REFRAME_EVENT: &str = "auto-reframe-event";
/// Face detection resizes to its own fixed input regardless, so this only needs to be large
/// enough that a small/distant face survives that resize as more than a couple of pixels.
const REFRAME_FRAME_MAX_DIM: u32 = 960;
const DYNAMIC_REFRAME_SAMPLES_PER_SEC: f64 = 2.0;
const DYNAMIC_REFRAME_MIN_SAMPLES: usize = 4;
const DYNAMIC_REFRAME_MAX_SAMPLES: usize = 30;
const DYNAMIC_REFRAME_SMOOTH_WINDOW: usize = 5;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoReframeResult {
    pub clip_id: u64,
    pub subject_found: bool,
}

/// Runs auto-reframe against `clip_id` on a background thread -- what the properties panel's
/// "Reenquadramento automático" button does. Targets the active sequence's export aspect ratio
/// (source resolution for `Original`). A model must be configured *unless* the clip already
/// carries `reframe_seed_point` (a manually pinned anchor skips face detection entirely).
/// Errors (already running, clip/asset not found, no model and no seed) surface synchronously;
/// the crop is applied automatically and [`AUTO_REFRAME_EVENT`] reports whether a subject was
/// actually found.
#[tauri::command]
pub fn auto_reframe_clip(app: AppHandle, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut running = state
            .auto_reframing_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("an auto-reframe is already running".to_string());
        }
        *running = Some(clip_id);
    }
    let clear_running = |state: &AppState| {
        *state
            .auto_reframing_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    };

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
    let seed_point = clip.reframe_seed_point;
    let configured_model_path = state.prefs().view.reframe_model_path.clone();
    if seed_point.is_none() && configured_model_path.trim().is_empty() {
        clear_running(&state);
        return Err("no auto-reframe model configured".to_string());
    }
    let (asset_resolution, source_path) = {
        let session = state.session();
        let asset = session
            .project
            .media_library
            .iter()
            .find(|a| a.id == clip.asset_id)
            .cloned();
        (
            asset.as_ref().and_then(|a| a.resolution),
            asset.map(|a| a.source_path),
        )
    };
    let (Some((source_w, source_h)), Some(source_path)) = (asset_resolution, source_path) else {
        clear_running(&state);
        return Err("asset resolution unknown".to_string());
    };
    let (target_w, target_h) = {
        let session = state.session();
        session
            .project
            .active_sequence()
            .export_settings
            .aspect_ratio
            .dims_or((source_w, source_h))
    };
    let midpoint_secs = (clip.source_in_secs + clip.source_out_secs) / 2.0;
    let model_path = PathBuf::from(configured_model_path);

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let subject_center = match seed_point {
            Some(seed) => Some(seed),
            None => extract_frame(&source_path, midpoint_secs).and_then(|(w, h, rgba)| {
                match avcore::detect_faces(&model_path, &rgba, w, h) {
                    Ok(faces) => avcore::main_subject_center(&faces),
                    Err(error) => {
                        tracing::warn!(%error, "auto-reframe face detection failed");
                        None
                    }
                }
            }),
        };
        let subject_found = subject_center.is_some();
        let crop =
            avcore::compute_reframe_crop(source_w, source_h, target_w, target_h, subject_center);
        {
            let state = worker_app.state::<AppState>();
            clear_running(&state);
        }
        let _ = with_session(&worker_app, |session| {
            if let Some(clip) = session.project.timeline_mut().clip_mut(clip_id) {
                clip.crop_x = crop.x;
                clip.crop_y = crop.y;
                clip.crop_w = crop.w;
                clip.crop_h = crop.h;
            }
            session.dirty = true;
            Ok(())
        });
        let _ = worker_app.emit(
            AUTO_REFRAME_EVENT,
            AutoReframeResult {
                clip_id,
                subject_found,
            },
        );
    });
    Ok(())
}

/// Samples the selected subject throughout the clip and stores animated crop keyframes. This
/// shares the static operation's single-flight guard so two detector jobs cannot compete for the
/// ONNX runtime or overwrite one another's result.
#[tauri::command]
pub fn dynamic_auto_reframe_clip(app: AppHandle, clip_id: u64) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut running = state
            .auto_reframing_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("an auto-reframe is already running".to_string());
        }
        *running = Some(clip_id);
    }
    let clear_running = |state: &AppState| {
        *state
            .auto_reframing_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
    };
    let clip = {
        let session = state.session();
        session
            .project
            .timeline()
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|clip| clip.id == clip_id)
            .cloned()
    };
    let Some(clip) = clip else {
        clear_running(&state);
        return Err("clip not found".to_string());
    };
    let seed_point = clip.reframe_seed_point;
    let model_path = state.prefs().view.reframe_model_path.clone();
    if seed_point.is_none() && model_path.trim().is_empty() {
        clear_running(&state);
        return Err("no auto-reframe model configured".to_string());
    }
    let (source_w, source_h, source_path) = {
        let session = state.session();
        let asset = session
            .project
            .media_library
            .iter()
            .find(|asset| asset.id == clip.asset_id);
        match asset.and_then(|asset| {
            asset
                .resolution
                .map(|resolution| (resolution, &asset.source_path))
        }) {
            Some(((width, height), path)) => (width, height, path.clone()),
            None => {
                clear_running(&state);
                return Err("asset resolution unknown".to_string());
            }
        }
    };
    let (target_w, target_h) = {
        let session = state.session();
        session
            .project
            .active_sequence()
            .export_settings
            .aspect_ratio
            .dims_or((source_w, source_h))
    };
    let worker_app = app.clone();
    std::thread::spawn(move || {
        let Some(samples) = dynamic_reframe_samples(
            &source_path,
            clip.source_in_secs,
            clip.source_out_secs,
            &PathBuf::from(model_path),
            seed_point,
        ) else {
            let state = worker_app.state::<AppState>();
            clear_running(&state);
            let _ = worker_app.emit(
                AUTO_REFRAME_EVENT,
                AutoReframeResult {
                    clip_id,
                    subject_found: false,
                },
            );
            return;
        };
        let subject_found = samples.iter().any(|sample| sample.subject_center.is_some());
        let filled = avcore::fill_reframe_gaps(&samples);
        let smoothed = avcore::smooth_subject_centers(&filled, DYNAMIC_REFRAME_SMOOTH_WINDOW);
        let crop_samples: Vec<_> = samples
            .iter()
            .zip(&smoothed)
            .map(|(sample, center)| {
                (
                    sample.time_fraction,
                    avcore::compute_reframe_crop(source_w, source_h, target_w, target_h, *center),
                )
            })
            .collect();
        let (crop_x, crop_y, crop_w, crop_h) =
            avcore::sparse_crop_keyframes(&crop_samples, avcore::DEFAULT_SPARSIFY_EPSILON);
        let state = worker_app.state::<AppState>();
        clear_running(&state);
        let _ = with_session(&worker_app, |session| {
            if let Some(clip) = session.project.timeline_mut().clip_mut(clip_id) {
                clip.crop_x_keyframes = crop_x;
                clip.crop_y_keyframes = crop_y;
                clip.crop_w_keyframes = crop_w;
                clip.crop_h_keyframes = crop_h;
            }
            session.dirty = true;
            Ok(())
        });
        let _ = worker_app.emit(
            AUTO_REFRAME_EVENT,
            AutoReframeResult {
                clip_id,
                subject_found,
            },
        );
    });
    Ok(())
}

fn dynamic_reframe_samples(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    model_path: &Path,
    seed_point: Option<(f32, f32)>,
) -> Option<Vec<avcore::ReframeSample>> {
    let times = avcore::FrameSampler::even_sample_times(
        source_in_secs,
        source_out_secs,
        DYNAMIC_REFRAME_SAMPLES_PER_SEC,
        DYNAMIC_REFRAME_MIN_SAMPLES,
        DYNAMIC_REFRAME_MAX_SAMPLES,
    );
    let duration = (source_out_secs - source_in_secs).max(f64::EPSILON);
    if let Some(center) = seed_point {
        return Some(
            times
                .into_iter()
                .map(|time| avcore::ReframeSample {
                    time_fraction: ((time - source_in_secs) / duration).clamp(0.0, 1.0) as f32,
                    subject_center: Some(center),
                })
                .collect(),
        );
    }
    let Ok(sampler) = avcore::FrameSampler::open(source_path) else {
        tracing::warn!(path = %source_path.display(), "dynamic auto-reframe could not open source video");
        return None;
    };
    let mut previous_center = None;
    Some(times
        .into_iter()
        .map(|time| {
            let center = sampler
                .sample(time, Duration::from_millis(800))
                .and_then(|frame| {
                    let (width, height, rgba) = downscale_frame_rgba(
                        frame.width,
                        frame.height,
                        frame.rgba,
                        REFRAME_FRAME_MAX_DIM,
                    );
                    avcore::detect_faces(model_path, &rgba, width, height)
                        .map_err(|error| tracing::warn!(%error, "dynamic auto-reframe face detection failed"))
                        .ok()
                        .and_then(|faces| avcore::select_subject_center(
                            &faces,
                            previous_center,
                            avcore::DEFAULT_CONTINUITY_MAX_DISTANCE,
                        ))
                });
            if center.is_some() {
                previous_center = center;
            }
            avcore::ReframeSample {
                time_fraction: ((time - source_in_secs) / duration).clamp(0.0, 1.0) as f32,
                subject_center: center,
            }
        })
        .collect())
}

fn extract_frame(path: &Path, at_secs: f64) -> Option<(u32, u32, Vec<u8>)> {
    let sampler = avcore::FrameSampler::open(path).ok()?;
    let frame = sampler.sample(at_secs, Duration::from_millis(1500))?;
    Some(downscale_frame_rgba(
        frame.width,
        frame.height,
        frame.rgba,
        REFRAME_FRAME_MAX_DIM,
    ))
}

fn downscale_frame_rgba(
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    max_dim: u32,
) -> (u32, u32, Vec<u8>) {
    let scale = (max_dim as f32 / width.max(height) as f32).min(1.0);
    if scale >= 1.0 {
        return (width, height, rgba);
    }
    let new_width = ((width as f32 * scale) as u32).max(1);
    let new_height = ((height as f32 * scale) as u32).max(1);
    let mut out = vec![0u8; (new_width * new_height * 4) as usize];
    for y in 0..new_height {
        let src_y = (y * height / new_height).min(height - 1);
        for x in 0..new_width {
            let src_x = (x * width / new_width).min(width - 1);
            let src = ((src_y * width + src_x) * 4) as usize;
            let dst = ((y * new_width + x) * 4) as usize;
            out[dst..dst + 4].copy_from_slice(&rgba[src..src + 4]);
        }
    }
    (new_width, new_height, out)
}
