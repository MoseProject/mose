//! Preferences, recent-project and recovery commands.

use std::path::PathBuf;

use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::autosave;
use crate::prefs::PrefsView;
use crate::state::{publish, AppState};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProject {
    pub path: String,
    pub name: String,
    pub modified_unix_secs: u64,
    pub autosave_path: Option<String>,
}

#[tauri::command]
pub fn get_prefs(state: State<'_, AppState>) -> PrefsView {
    state.prefs().view.clone()
}

#[tauri::command]
pub fn set_prefs(app: AppHandle, prefs: PrefsView) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut document = state.prefs();
        document.view = prefs;
        document.save()?;
    }
    let session = state.session();
    publish(&app, &session);
    Ok(())
}

#[tauri::command]
pub fn get_recent_projects(state: State<'_, AppState>) -> Vec<RecentProject> {
    state
        .prefs()
        .view
        .recent_project_paths
        .iter()
        .filter_map(|path| {
            let path = PathBuf::from(path);
            let metadata = std::fs::metadata(&path).ok()?;
            let modified_unix_secs = metadata
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();
            let name = avcore::load_project_from_file(&path).ok()?.name;
            Some(RecentProject {
                autosave_path: autosave::is_newer(&path).map(|path| path.display().to_string()),
                path: path.display().to_string(),
                name,
                modified_unix_secs,
            })
        })
        .collect()
}

#[tauri::command]
pub fn remove_recent_project(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    {
        let mut prefs = state.prefs();
        prefs
            .view
            .recent_project_paths
            .retain(|candidate| candidate != &path);
        prefs.save()?;
    }
    let session = state.session();
    publish(&app, &session);
    Ok(())
}

pub fn record_recent_project(app: &AppHandle, path: &std::path::Path) -> Result<(), String> {
    let state = app.state::<AppState>();
    let path = path.display().to_string();
    let mut prefs = state.prefs();
    prefs
        .view
        .recent_project_paths
        .retain(|candidate| candidate != &path);
    prefs.view.recent_project_paths.insert(0, path);
    prefs.view.recent_project_paths.truncate(10);
    prefs.save()
}

#[tauri::command]
pub fn restore_autosave(app: AppHandle, path: String) -> Result<(), String> {
    let project_path = PathBuf::from(path);
    let autosave_path = autosave::is_newer(&project_path)
        .ok_or_else(|| "no newer autosave is available".to_string())?;
    let mut restored =
        avcore::load_project_from_file(&autosave_path).map_err(|error| error.to_string())?;
    restored.file_path = Some(project_path);
    let state = app.state::<AppState>();
    let mut session = state.session();
    session.replace_project(restored);
    session.dirty = true;
    publish(&app, &session);
    Ok(())
}

#[tauri::command]
pub fn discard_autosave(path: String) -> Result<(), String> {
    let path = crate::prefs::autosave_path(&PathBuf::from(path));
    if path.exists() {
        std::fs::remove_file(path).map_err(|error| error.to_string())?;
    }
    Ok(())
}
