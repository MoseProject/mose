//! D1 differentiator (`spec/architecture/differentiators.md`): silence-gap detection/review.
//! Mirrors `ui::App::begin_silence_review`/`apply_silence_review` -- pure read (detect) then an
//! explicit apply command; the webview holds the accept/reject list locally between the two
//! calls rather than staging server-side review state, the same shape the Templates/Transcript
//! popups already use.

use avcore::{
    clip_silence_gaps, SilenceGap, DEFAULT_MIN_SILENCE_SECS, DEFAULT_SILENCE_THRESHOLD_LINEAR,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use super::undoable;
use crate::state::AppState;

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceGapView {
    pub start_secs: f64,
    pub end_secs: f64,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SilenceReviewView {
    pub track_id: u64,
    pub gaps: Vec<SilenceGapView>,
}

/// Scans the track holding `clip_id` for silence gaps across every clip whose asset has cached
/// waveform peaks -- what the timeline toolbar's silence-review button does. `None` if `clip_id`
/// isn't found; a clip whose asset has no cached waveform yet contributes no gaps rather than
/// erroring, same as the egui shell.
#[tauri::command]
pub fn detect_silence_gaps_for_clip(
    state: State<'_, AppState>,
    clip_id: u64,
) -> Option<SilenceReviewView> {
    let session = state.session();
    let project = &session.project;
    let track = project
        .timeline()
        .tracks
        .iter()
        .find(|t| t.clips.iter().any(|c| c.id == clip_id))?;
    let mut gaps: Vec<SilenceGap> = Vec::new();
    for clip in &track.clips {
        let Some(asset) = project.media_library.iter().find(|a| a.id == clip.asset_id) else {
            continue;
        };
        let Some(peaks) = &asset.waveform_peaks else {
            continue;
        };
        gaps.extend(clip_silence_gaps(
            peaks,
            asset.duration_secs,
            clip,
            DEFAULT_SILENCE_THRESHOLD_LINEAR,
            DEFAULT_MIN_SILENCE_SECS,
        ));
    }
    gaps.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    Some(SilenceReviewView {
        track_id: track.id,
        gaps: gaps
            .into_iter()
            .map(|g| SilenceGapView {
                start_secs: g.start_secs,
                end_secs: g.end_secs,
            })
            .collect(),
    })
}

/// Ripple-deletes every gap in `gaps` from `track_id` -- what the review modal's "Apply" button
/// does, one undo step for the whole batch. Processes latest-start-first so an earlier removal's
/// leftward ripple never invalidates a not-yet-applied gap's coordinates, same order
/// `ui::App::apply_silence_review` uses.
#[tauri::command]
pub fn apply_silence_gaps<R: Runtime>(
    app: AppHandle<R>,
    track_id: u64,
    gaps: Vec<SilenceGapView>,
) -> Result<(), String> {
    undoable(&app, |project| {
        if gaps.is_empty() {
            return false;
        }
        let mut next_id = project.timeline().next_clip_id();
        let Some(track) = project
            .timeline_mut()
            .tracks
            .iter_mut()
            .find(|t| t.id == track_id)
        else {
            return false;
        };
        let mut sorted = gaps;
        sorted.sort_by(|a, b| b.start_secs.total_cmp(&a.start_secs));
        let mut changed = false;
        for gap in sorted {
            changed |= track.ripple_delete_range(gap.start_secs, gap.end_secs, &mut next_id);
        }
        changed
    })
}

#[cfg(test)]
#[path = "silence_test.rs"]
mod tests;
