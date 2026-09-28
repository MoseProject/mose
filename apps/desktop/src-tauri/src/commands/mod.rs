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

pub mod media;
pub mod prefs;
pub mod timeline;

use std::path::PathBuf;

use avcore::{ExportAspectRatio, Project};
use tauri::{AppHandle, Manager, State};

use crate::export::{prepare_export, ExportJobView};
use crate::state::{blank_project, publish, AppState, Session};

/// Runs a non-undoable mutation (navigation, persistence metadata) and publishes the result.
pub(crate) fn with_session<T>(
    app: &AppHandle,
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
pub(crate) fn undoable(
    app: &AppHandle,
    edit: impl FnOnce(&mut Project) -> bool,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut session = state.session();
    session.project.timeline_mut().playhead_secs = state.preview.playhead();
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
    serde_json::to_value(state.session().view()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn new_project(app: AppHandle, name: String, sequence_name: String) -> Result<(), String> {
    let target_lufs = {
        let profile = app.state::<AppState>().prefs().view.lufs_profile;
        [-14.0, -16.0, -23.0].get(profile).copied().unwrap_or(-14.0)
    };
    with_session(&app, |session| {
        let mut project = blank_project(name, sequence_name);
        project.active_sequence_mut().export_settings.target_lufs = target_lufs;
        session.replace_project(project);
        Ok(())
    })
}

/// Shows an open dialog; `Ok(false)` when the user cancels.
#[tauri::command]
pub async fn open_project(app: AppHandle) -> Result<bool, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("oca", &["ocproj"])
        .pick_file()
    else {
        return Ok(false);
    };
    open_project_path(app, path.display().to_string()).await?;
    Ok(true)
}

#[tauri::command]
pub async fn open_project_path(app: AppHandle, path: String) -> Result<(), String> {
    let path = PathBuf::from(path);
    let mut project = avcore::load_project_from_file(&path).map_err(|e| e.to_string())?;
    project.file_path = Some(path);
    let epoch = with_session(&app, |session| {
        session.replace_project(project);
        Ok(session.project_epoch)
    })?;
    prefs::record_recent_project(&app, &path)?;
    // Proxies are a regenerable cache, never serialized: re-attach (or rebuild) them so
    // preview decodes the light file again after a reopen.
    media::rehydrate_proxies(&app, epoch);
    Ok(())
}

/// Saves to the project's own path, or asks for one (always, when `save_as`). `Ok(false)` when
/// the user cancels the dialog.
#[tauri::command]
pub async fn save_project(app: AppHandle, save_as: bool) -> Result<bool, String> {
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
        None => match rfd::FileDialog::new()
            .add_filter("oca", &["ocproj"])
            .set_file_name(format!("{name}.ocproj"))
            .save_file()
        {
            Some(path) => path,
            None => return Ok(false),
        },
    };
    let playhead = app.state::<AppState>().preview.playhead();
    with_session(&app, |session| {
        session.project.timeline_mut().playhead_secs = playhead;
        avcore::save_project_to_file(&session.project, &path).map_err(|e| e.to_string())?;
        session.project.file_path = Some(path);
        session.dirty = false;
        Ok(true)
    })?;
    prefs::record_recent_project(&app, &path)?;
    Ok(true)
}

#[tauri::command]
pub fn undo(app: AppHandle) -> Result<(), String> {
    with_session(&app, |session| {
        session.undo();
        Ok(())
    })
}

#[tauri::command]
pub fn redo(app: AppHandle) -> Result<(), String> {
    with_session(&app, |session| {
        session.redo();
        Ok(())
    })
}

#[tauri::command]
pub fn preview_seek(state: State<'_, AppState>, position_secs: f64) {
    state.preview.seek(position_secs);
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
pub fn set_export_settings(
    app: AppHandle,
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

/// Resolves the active sequence, asks where to write it, and starts rendering. `Ok(None)` when
/// the user cancels the dialog.
#[tauri::command]
pub async fn export_active_sequence(app: AppHandle) -> Result<Option<u64>, String> {
    let (prepared, title) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            prepare_export(&session.project)?,
            session.project.active_sequence().name.clone(),
        )
    };
    let Some(output) = rfd::FileDialog::new()
        .add_filter("MP4", &["mp4"])
        .set_file_name(format!("{title}_export.mp4"))
        .save_file()
    else {
        return Ok(None);
    };
    let exports = app.state::<AppState>().exports.clone();
    Ok(Some(exports.start(app.clone(), title, output, prepared)))
}

#[tauri::command]
pub fn cancel_export(state: State<'_, AppState>, job_id: u64) {
    state.exports.cancel(job_id);
}

#[tauri::command]
pub fn list_exports(state: State<'_, AppState>) -> Vec<ExportJobView> {
    state.exports.snapshot()
}
