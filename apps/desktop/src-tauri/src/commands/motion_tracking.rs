//! Motion tracking -- background-thread template tracking for the selected clip, mirroring
//! `ui::App::spawn_motion_track_selected_clip` exactly (same sampling rate/cap, same
//! `avcore::track_region`/`tracked_positions_to_keyframes` pipeline), no core changes needed.

use std::path::Path;
use std::time::Duration;

use avcore::{Keyframe, Position};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::with_session;
use crate::state::AppState;

pub const MOTION_TRACK_EVENT: &str = "motion-track-event";
const SAMPLES_PER_SEC: f64 = 4.0;
const MAX_SAMPLES: usize = 60;
const SIZE_RANGE: std::ops::RangeInclusive<f32> = 0.05..=0.6;
const SEARCH_RADIUS_RANGE: std::ops::RangeInclusive<f32> = 0.02..=0.3;

/// Tracks the region centered at `(center_x, center_y)` (source-frame fractions,
/// `template_width`/`template_height`/`search_radius` clamped to the same ranges the egui
/// properties panel uses) across `clip_id`'s own trimmed source duration, on a background
/// thread, then rewrites `position_keyframes` -- what the properties panel's "Rastrear
/// movimento" button does. Errors (already running, clip/asset not found) surface synchronously;
/// the result (applied automatically) arrives via [`MOTION_TRACK_EVENT`] carrying the keyframe
/// count (`0` means nothing was decoded and the clip's existing keyframes were left untouched).
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn motion_track_clip<R: Runtime>(
    app: AppHandle<R>,
    clip_id: u64,
    center_x: f32,
    center_y: f32,
    template_width: f32,
    template_height: f32,
    search_radius: f32,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut running = state
            .motion_tracking_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("a motion track is already running".to_string());
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
            .motion_tracking_clip_id
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
            .motion_tracking_clip_id
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = None;
        return Err("asset not found".to_string());
    };
    let base_position = clip
        .position_keyframes
        .first()
        .map(|k| k.value)
        .unwrap_or(Position { x: 0.0, y: 0.0 });
    let center_x = center_x.clamp(0.0, 1.0);
    let center_y = center_y.clamp(0.0, 1.0);
    let template_width = template_width.clamp(*SIZE_RANGE.start(), *SIZE_RANGE.end());
    let template_height = template_height.clamp(*SIZE_RANGE.start(), *SIZE_RANGE.end());
    let search_radius =
        search_radius.clamp(*SEARCH_RADIUS_RANGE.start(), *SEARCH_RADIUS_RANGE.end());

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let keyframes = motion_track_one(
            &source_path,
            clip.source_in_secs,
            clip.source_out_secs,
            base_position,
            center_x,
            center_y,
            template_width,
            template_height,
            search_radius,
        );
        {
            let state = worker_app.state::<AppState>();
            *state
                .motion_tracking_clip_id
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
        }
        if !keyframes.is_empty() {
            let _ = with_session(&worker_app, |session| {
                if let Some(clip) = session.project.timeline_mut().clip_mut(clip_id) {
                    clip.position_keyframes = keyframes.clone();
                }
                session.dirty = true;
                Ok(())
            });
        }
        let _ = worker_app.emit(MOTION_TRACK_EVENT, keyframes.len());
    });
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn motion_track_one(
    source_path: &Path,
    source_in_secs: f64,
    source_out_secs: f64,
    base_position: Position,
    center_x: f32,
    center_y: f32,
    template_width: f32,
    template_height: f32,
    search_radius: f32,
) -> Vec<Keyframe<Position>> {
    let duration = (source_out_secs - source_in_secs).max(0.0);
    if duration <= 0.0 {
        return Vec::new();
    }
    let sample_times = avcore::FrameSampler::even_sample_times(
        source_in_secs,
        source_out_secs,
        SAMPLES_PER_SEC,
        2,
        MAX_SAMPLES,
    );
    let Ok(sampler) = avcore::FrameSampler::open(source_path) else {
        return Vec::new();
    };
    let mut decoded = Vec::with_capacity(sample_times.len());
    for &t in &sample_times {
        let Some(frame) = sampler.sample(t, Duration::from_millis(800)) else {
            continue;
        };
        let time_fraction = ((t - source_in_secs) / duration) as f32;
        decoded.push((
            time_fraction,
            avcore::rgba_to_gray(&frame.rgba, frame.width, frame.height),
        ));
    }
    if decoded.len() < 2 {
        return Vec::new();
    }
    let (time_fractions, frames): (Vec<f32>, Vec<avcore::GrayFrame>) = decoded.into_iter().unzip();
    let tracked = avcore::track_region(
        &frames,
        center_x,
        center_y,
        template_width,
        template_height,
        search_radius,
    );
    avcore::tracked_positions_to_keyframes(&tracked, &time_fractions, base_position)
}

#[cfg(test)]
#[path = "motion_tracking_test.rs"]
mod tests;
