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
pub mod prefs;
pub mod privacy_blur;
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

use std::path::PathBuf;

use avcore::{ExportAspectRatio, PanelLayout, Project};
use tauri::{AppHandle, Manager, State};

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

/// Saves the active project's editor dimensions. Layout changes are not undoable timeline edits,
/// but are included in the next normal project save through `Project::panel_layout`.
#[tauri::command]
pub fn set_panel_layout(
    app: AppHandle,
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
    project.file_path = Some(path.clone());
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
        session.project.file_path = Some(path.clone());
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
pub fn preview_set_hardware_decode(state: State<'_, AppState>, enabled: bool) {
    state.preview.set_hardware_decode(enabled);
}

/// The preview engines this build can run, for the preferences screen. The choice is read at
/// startup, so changing it takes effect after a restart.
#[tauri::command]
pub fn preview_engines() -> Vec<&'static str> {
    if cfg!(feature = "bush-preview") {
        vec!["gstreamer", "bush"]
    } else {
        vec!["gstreamer"]
    }
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
