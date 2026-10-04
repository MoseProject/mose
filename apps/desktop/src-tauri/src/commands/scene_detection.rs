//! D4 (`spec/architecture/differentiators.md`): background-thread scene-cut detection for the
//! selected clip -- mirrors `ui::App`'s `scene_detection.rs` exactly (same `FrameSampler`
//! sampling rate/cap, same `MarkerKind::Chapter` marker application), no core changes needed.

use std::path::Path;
use std::time::Duration;

use avcore::timeline::MarkerKind;
use avcore::{FrameSampler, SceneCut};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::with_session;
use crate::state::AppState;

pub const SCENE_CUT_EVENT: &str = "scene-cut-event";
/// Frames sampled per second of the clip's own trimmed source duration.
const SAMPLES_PER_SEC: f64 = 4.0;
/// Upper bound on sampled frames regardless of `SAMPLES_PER_SEC * duration`.
const MAX_SAMPLES: usize = 600;

/// Runs scene-cut detection against `clip_id` on a background thread -- what the timeline
/// toolbar's "🎬 Detect Chapters" button does. Errors (already running, clip not found)
/// surface synchronously; the result arrives via [`SCENE_CUT_EVENT`].
#[tauri::command]
pub fn detect_scene_cuts_for_clip<R: Runtime>(
    app: AppHandle<R>,
    clip_id: u64,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut running = state
            .scene_cut_detection_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("a scene-cut detection is already running".to_string());
        }
        *running = Some(clip_id);
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
        *state
            .scene_cut_detection_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        return Err("clip not found".to_string());
    };
    let source_path = {
        let session = state.session();
        session
            .project
            .media_library
            .iter()
            .find(|a| a.id == clip.asset_id)
            .map(|a| a.source_path.clone())
    };
    let Some(source_path) = source_path else {
        *state
            .scene_cut_detection_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        return Err("asset not found".to_string());
    };

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let cuts = detect_scene_cuts_one(&source_path, clip.source_in_secs, clip.source_out_secs);
        {
            let state = worker_app.state::<AppState>();
            *state
                .scene_cut_detection_clip_id
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
        }
        publish_cuts(&worker_app, clip_id, &cuts);
        let _ = worker_app.emit(SCENE_CUT_EVENT, cuts.len());
    });
    Ok(())
}

/// Adds the detected cuts to the project as chapter markers; nothing is published for none.
fn publish_cuts<R: Runtime>(app: &AppHandle<R>, clip_id: u64, cuts: &[SceneCut]) {
    if cuts.is_empty() {
        return;
    }
    let _ = with_session(app, |session| {
        apply_detected_scene_cuts(session.project.timeline_mut(), clip_id, cuts);
        session.dirty = true;
        Ok(())
    });
}

fn detect_scene_cuts_one(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
) -> Vec<SceneCut> {
    let duration = (source_out_secs - source_in_secs).max(0.0);
    if duration <= 0.0 {
        return Vec::new();
    }
    let sample_times = FrameSampler::even_sample_times(
        source_in_secs,
        source_out_secs,
        SAMPLES_PER_SEC,
        2,
        MAX_SAMPLES,
    );
    let Ok(sampler) = FrameSampler::open(source_path) else {
        return Vec::new();
    };
    let samples: Vec<(f64, avcore::GrayFrame)> = sample_times
        .iter()
        .filter_map(|&t| {
            let frame = sampler.sample(t, Duration::from_millis(500))?;
            Some((
                t,
                avcore::rgba_to_gray(&frame.rgba, frame.width, frame.height),
            ))
        })
        .collect();
    avcore::detect_scene_cuts(&samples, avcore::DEFAULT_SCENE_CUT_THRESHOLD)
}

/// Maps each cut's source-relative time onto the timeline and adds a numbered Chapter marker at
/// each -- mirrors `ui::App::apply_detected_scene_cuts`. A no-op if `clip_id` no longer exists
/// or its `speed_factor` isn't positive.
fn apply_detected_scene_cuts(
    timeline: &mut avcore::timeline::Timeline,
    clip_id: u64,
    cuts: &[SceneCut],
) {
    let Some(clip) = timeline
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .find(|c| c.id == clip_id)
        .cloned()
    else {
        return;
    };
    if clip.speed_factor <= 0.0 {
        return;
    }
    let existing_chapters = timeline
        .markers
        .iter()
        .filter(|m| m.kind == MarkerKind::Chapter)
        .count();
    for (i, cut) in cuts.iter().enumerate() {
        let timeline_secs =
            clip.start_secs + (cut.at_secs - clip.source_in_secs) / clip.speed_factor as f64;
        let marker_id = timeline.add_marker(timeline_secs, MarkerKind::Chapter);
        if let Some(marker) = timeline.marker_mut(marker_id) {
            marker.label = format!("Chapter {}", existing_chapters + i + 1);
        }
    }
}

#[cfg(test)]
#[path = "scene_detection_test.rs"]
mod tests;
