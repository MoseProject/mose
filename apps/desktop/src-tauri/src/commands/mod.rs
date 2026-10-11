// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! `#[tauri::command]` surface the webview calls through `src/ipc.ts`. Commands return
//! `Result<_, String>` so a failure surfaces as a rejected promise with a readable message;
//! successful mutations publish the new session through [`crate::state::publish`] rather than
//! returning it, so every listener (not just the caller) sees the same state.

pub mod auto_reframe;
pub mod background_removal;
pub mod error_reporting;
pub mod export;
pub mod highlight_detection;
pub mod markers;
pub mod media;
pub mod motion_tracking;
pub mod overlays;
pub mod paths;
pub mod prefs;
pub mod privacy_blur;
pub mod recovery;
pub mod scene_detection;
pub mod sequences;
pub mod silence;
pub mod sound_library;
pub mod timeline;
pub mod transcribe;
pub mod tts;
pub mod update_check;
pub mod watch_folder;
pub mod youtube_download;

use std::path::{Path, PathBuf};

use avcore::{ExportAspectRatio, PanelLayout, Project};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::state::{blank_project, publish, AppState, Session};

/// Runs a non-undoable mutation (navigation, persistence metadata) and publishes the result.
pub(crate) fn with_session<R: Runtime, T>(
    app: &AppHandle<R>,
    change: impl FnOnce(&mut Session) -> Result<T, String>,
) -> Result<T, String> {
    let state = app.state::<AppState>();
    let mut session = state.session();
    let out = change(&mut session)?;
    publish(app, &session);
    Ok(out)
}

/// Runs an undoable edit against the project -- one call per user-visible commit, same
/// contract as `ui::App::push_undo_snapshot`. `edit` returns whether anything changed; only
/// then is the pre-edit snapshot pushed, so a rejected edit (locked track, out-of-range trim)
/// leaves no empty undo step behind. The live playhead is synced in first, since edits like
/// split-at-playhead read it from the timeline.
pub(crate) fn undoable<R: Runtime>(
    app: &AppHandle<R>,
    edit: impl FnOnce(&mut Project) -> bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut session = state.session();
    let playhead = session.project.snap_secs(state.preview.playhead());
    session.project.timeline_mut().playhead_secs = playhead;
    let before = session.project.active_sequence().clone();
    if edit(&mut session.project) {
        session.undo.push(before);
        session.dirty = true;
        publish(app, &session);
    }
    Ok(())
}

#[tauri::command]
pub fn get_session(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    // The same lock order as `publish` (session, then sync), so the revision matches the project.
    let session = state.session();
    let rev = crate::state::lock_or_recover(&state.sync).rev;
    Ok(crate::session_sync::snapshot(&session.view_at(rev)))
}

/// Saves the active project's editor dimensions. Layout changes are not undoable timeline edits,
/// but are included in the next normal project save through `Project::panel_layout`.
#[tauri::command]
pub fn set_panel_layout<R: Runtime>(
    app: AppHandle<R>,
    library_width: f32,
    inspector_width: f32,
    timeline_height: f32,
) -> Result<(), String> {
    if !library_width.is_finite()
        || !inspector_width.is_finite()
        || !timeline_height.is_finite()
        || !(160.0..=640.0).contains(&library_width)
        || !(180.0..=640.0).contains(&inspector_width)
        || !(140.0..=1_200.0).contains(&timeline_height)
    {
        return Err("invalid panel layout".to_string());
    }
    with_session(&app, |session| {
        session.project.panel_layout = Some(PanelLayout {
            lib_panel_width: library_width,
            props_panel_width: inspector_width,
            timeline_height,
        });
        session.dirty = true;
        Ok(())
    })
}

/// Loudness target for new projects: the profile chosen in the preferences.
fn default_target_lufs<R: Runtime>(app: &AppHandle<R>) -> f32 {
    let profile = app.state::<AppState>().prefs().view.lufs_profile;
    [-14.0, -16.0, -23.0].get(profile).copied().unwrap_or(-14.0)
}

#[tauri::command]
pub fn new_project<R: Runtime>(
    app: AppHandle<R>,
    name: String,
    sequence_name: String,
) -> Result<(), String> {
    let target_lufs = default_target_lufs(&app);
    with_session(&app, |session| {
        let mut project = blank_project(name, sequence_name);
        project.active_sequence_mut().export_settings.target_lufs = target_lufs;
        session.replace_project(project);
        Ok(())
    })
}

/// A file name for `name`: the characters a file system rejects become spaces.
fn file_stem_for(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c.is_control() || ['<', '>', ':', '"', '/', '\\', '|', '?', '*'].contains(&c) {
                ' '
            } else {
                c
            }
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        "project".to_string()
    } else {
        cleaned.to_string()
    }
}

/// Creates a project named `name`: asks where to keep it, writes the file at once so the project
/// exists (and shows up in the projects list) even if the app is closed right away, and opens
/// it. `Ok(false)` when the user cancels the dialog; the open project is untouched then, and
/// also when the file cannot be written.
#[tauri::command]
pub async fn create_project<R: Runtime>(
    app: AppHandle<R>,
    name: String,
    sequence_name: String,
) -> Result<bool, String> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("projectNameEmpty".to_string());
    }
    let Some(path) = crate::dialogs::save_file(
        "oca",
        &["ocproj"],
        &format!("{}.ocproj", file_stem_for(&name)),
    ) else {
        return Ok(false);
    };
    let target_lufs = default_target_lufs(&app);
    let mut project = blank_project(name, sequence_name);
    project.active_sequence_mut().export_settings.target_lufs = target_lufs;
    project.file_path = Some(path.clone());
    avcore::save_project_to_file(&project, &path).map_err(|e| e.to_string())?;
    with_session(&app, |session| {
        session.replace_project(project);
        Ok(())
    })?;
    note_recent_project(&app, &path);
    Ok(true)
}

/// Shows an open dialog; `Ok(false)` when the user cancels.
#[tauri::command]
pub async fn open_project<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let Some(path) = crate::dialogs::pick_file("oca", &["ocproj"]) else {
        return Ok(false);
    };
    open_project_path(app, path.display().to_string()).await?;
    Ok(true)
}

#[tauri::command]
pub async fn open_project_path<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    if !path.exists() {
        // The file was moved or deleted: a stale entry in the recent list would fail the same
        // way forever, so it goes away.
        let _ = prefs::remove_recent_project(app, path.display().to_string());
        return Err("projectMissing".to_string());
    }
    let mut project = avcore::load_project_from_file(&path).map_err(|e| e.to_string())?;
    project.file_path = Some(path.clone());
    let epoch = with_session(&app, |session| {
        session.replace_project(project);
        Ok(session.project_epoch)
    })?;
    note_recent_project(&app, &path);
    // Proxies are a regenerable cache, never serialized: re-attach (or rebuild) them so
    // preview decodes the light file again after a reopen.
    media::rehydrate_proxies(&app, epoch);
    Ok(())
}

/// Saves to the project's own path, or asks for one (always, when `save_as`). `Ok(false)` when
/// the user cancels the dialog.
#[tauri::command]
pub async fn save_project<R: Runtime>(app: AppHandle<R>, save_as: bool) -> Result<bool, String> {
    let (existing, name) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            session.project.file_path.clone(),
            session.project.name.clone(),
        )
    };
    let path = match existing.filter(|_| !save_as) {
        Some(path) => path,
        None => match crate::dialogs::save_file("oca", &["ocproj"], &format!("{name}.ocproj")) {
            Some(path) => path,
            None => return Ok(false),
        },
    };
    let playhead = app.state::<AppState>().preview.playhead();
    with_session(&app, |session| {
        session.project.timeline_mut().playhead_secs = playhead;
        avcore::save_project_to_file(&session.project, &path).map_err(|e| e.to_string())?;
        session.project.file_path = Some(path.clone());
        session.dirty = false;
        Ok(true)
    })?;
    note_recent_project(&app, &path);
    Ok(true)
}

/// Adds `path` to the recent list. The project is already open or saved by now, so a failure to
/// write the list is logged instead of reported as if the whole operation had failed.
fn note_recent_project<R: Runtime>(app: &AppHandle<R>, path: &Path) {
    note_recent_with(|| prefs::record_recent_project(app, path));
}

/// Runs `record`, logging instead of propagating its failure; whether it worked.
fn note_recent_with(record: impl FnOnce() -> Result<(), String>) -> bool {
    match record() {
        Ok(()) => true,
        Err(error) => {
            tracing::warn!(%error, "could not record the recent project");
            false
        }
    }
}

#[tauri::command]
pub fn undo<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    with_session(&app, |session| {
        session.undo();
        Ok(())
    })
}

#[tauri::command]
pub fn redo<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    with_session(&app, |session| {
        session.redo();
        Ok(())
    })
}

#[tauri::command]
pub fn preview_seek(state: State<'_, AppState>, position_secs: f64) {
    state.preview.seek(position_secs);
}

/// Tells the preview how many pixels its longest side really covers on screen, so it does not
/// composite, encode and ship more than the monitor shows.
#[tauri::command]
pub fn preview_set_max_dim(state: State<'_, AppState>, dim: u32) {
    state.preview.set_max_dim(dim);
}

#[tauri::command]
pub fn preview_step(state: State<'_, AppState>, delta_frames: i64) {
    state.preview.step(delta_frames);
}

#[tauri::command]
pub fn preview_set_playing(state: State<'_, AppState>, playing: bool) {
    state.preview.set_playing(playing);
}

#[tauri::command]
pub fn set_export_settings<R: Runtime>(
    app: AppHandle<R>,
    target_lufs: f32,
    aspect_ratio: ExportAspectRatio,
) -> Result<(), String> {
    with_session(&app, |session| {
        let settings = &mut session.project.active_sequence_mut().export_settings;
        settings.target_lufs = target_lufs.clamp(-40.0, -5.0);
        settings.aspect_ratio = aspect_ratio;
        session.dirty = true;
        Ok(())
    })
}

/// Sets the timeline's frame rate, or clears it with `None`. While set, edits made from the
/// editor land on whole frame boundaries. A rate with a zero part is refused.
#[tauri::command]
pub fn set_project_frame_rate<R: Runtime>(
    app: AppHandle<R>,
    frame_rate: Option<avcore::FrameRate>,
) -> Result<(), String> {
    if frame_rate.is_some_and(|rate| !rate.is_valid()) {
        return Err("invalid frame rate".to_string());
    }
    with_session(&app, |session| {
        if session.project.frame_rate != frame_rate {
            session.project.frame_rate = frame_rate;
            session.dirty = true;
        }
        Ok(())
    })
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
