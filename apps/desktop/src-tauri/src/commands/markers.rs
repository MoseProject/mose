//! Marker commands. Each accepted edit is one active-sequence undo snapshot.

use avcore::{MarkerKind, Project};
use tauri::AppHandle;

use super::undoable;

#[tauri::command]
pub fn add_marker(app: AppHandle, kind: MarkerKind) -> Result<(), String> {
    undoable(&app, |project| {
        let playhead = project.timeline().playhead_secs;
        project.timeline_mut().add_marker(playhead, kind);
        true
    })
}

#[tauri::command]
pub fn remove_marker(app: AppHandle, marker_id: u64) -> Result<(), String> {
    undoable(&app, |project| {
        project.timeline_mut().remove_marker(marker_id)
    })
}

#[tauri::command]
pub fn update_marker(
    app: AppHandle,
    marker_id: u64,
    label: Option<String>,
    kind: Option<MarkerKind>,
    completed: Option<bool>,
) -> Result<(), String> {
    undoable(&app, |project| {
        update(project, marker_id, label, kind, completed)
    })
}

pub(crate) fn update(
    project: &mut Project,
    marker_id: u64,
    label: Option<String>,
    kind: Option<MarkerKind>,
    completed: Option<bool>,
) -> bool {
    let Some(marker) = project.timeline_mut().marker_mut(marker_id) else {
        return false;
    };
    let before = marker.clone();
    if let Some(label) = label {
        marker.label = label;
    }
    if let Some(kind) = kind {
        marker.kind = kind;
    }
    if let Some(completed) = completed {
        marker.completed = completed;
    }
    *marker != before
}

#[cfg(test)]
#[path = "markers_test.rs"]
mod tests;
