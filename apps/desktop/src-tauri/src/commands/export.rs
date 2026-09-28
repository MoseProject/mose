use std::path::PathBuf;

use avcore::GpuEncoderPreference;
use tauri::{AppHandle, Manager, State};

use crate::export::prepare_export;
use crate::state::AppState;

/// The output file the user picked for an export, and whether it already exists -- the webview
/// asks overwrite-or-rename with its own translated text before calling [`start_export`].
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTarget {
    pub path: String,
    pub exists: bool,
}

/// Shows the save dialog for the active sequence's export. `Ok(None)` when the user cancels.
#[tauri::command]
pub async fn pick_export_path(app: AppHandle) -> Result<Option<ExportTarget>, String> {
    let title = {
        let state = app.state::<AppState>();
        let session = state.session();
        session.project.active_sequence().name.clone()
    };
    Ok(rfd::FileDialog::new()
        .add_filter("MP4", &["mp4"])
        .set_file_name(format!("{title}_export.mp4"))
        .save_file()
        .map(|path| ExportTarget {
            exists: path.exists(),
            path: path.display().to_string(),
        }))
}

/// Queues the active sequence for export to `path`. With `rename`, an existing file is kept and
/// the output goes to the next free `<stem> (n)` sibling instead of overwriting it.
#[tauri::command]
pub fn start_export(app: AppHandle, path: String, rename: bool) -> Result<u64, String> {
    let (prepared, title) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            prepare_export(&session.project)?,
            session.project.active_sequence().name.clone(),
        )
    };
    let mut output = PathBuf::from(path);
    if rename && output.exists() {
        output = avcore::next_available_path(&output);
    }
    let exports = app.state::<AppState>().exports.clone();
    Ok(exports.enqueue(app.clone(), title, output, prepared))
}

#[tauri::command]
pub fn pause_export(app: AppHandle, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.pause(app, job_id);
}
#[tauri::command]
pub fn resume_export(app: AppHandle, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.resume(app, job_id);
}
#[tauri::command]
pub fn cancel_export(app: AppHandle, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.cancel(app, job_id);
}
#[tauri::command]
pub fn remove_export(app: AppHandle, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.remove(app, job_id);
}
#[tauri::command]
pub fn match_export_loudness(app: AppHandle, target_lufs: f32) {
    app.state::<AppState>()
        .exports
        .match_loudness(&app, target_lufs);
}
#[tauri::command]
pub fn set_gpu_encoder_preference(
    state: State<'_, AppState>,
    preference: GpuEncoderPreference,
) -> Result<(), String> {
    state.exports.set_gpu_encoder_preference(preference)
}
#[tauri::command]
pub fn set_export_worker_count(
    state: State<'_, AppState>,
    worker_count: usize,
) -> Result<(), String> {
    state.exports.set_worker_count(worker_count)
}
#[tauri::command]
pub fn get_gpu_encoder_preference(state: State<'_, AppState>) -> GpuEncoderPreference {
    state.exports.gpu_encoder_preference()
}
#[tauri::command]
pub async fn export_active_sequence_srt(app: AppHandle) -> Result<bool, String> {
    let (srt, title) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            avcore::export_srt(session.project.timeline()),
            session.project.active_sequence().name.clone(),
        )
    };
    let Some(path) = rfd::FileDialog::new()
        .add_filter("SubRip", &["srt"])
        .set_file_name(format!("{title}.srt"))
        .save_file()
    else {
        return Ok(false);
    };
    std::fs::write(path, srt).map_err(|error| error.to_string())?;
    Ok(true)
}
#[tauri::command]
pub fn list_exports(state: State<'_, AppState>) -> Vec<crate::export::ExportJobView> {
    state.exports.snapshot()
}
