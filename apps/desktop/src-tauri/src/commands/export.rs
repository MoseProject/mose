use std::path::PathBuf;

use avcore::GpuEncoderPreference;
use tauri::{AppHandle, Manager, State};

use crate::export::prepare_export;
use crate::state::AppState;

#[tauri::command]
pub async fn export_active_sequence(app: AppHandle) -> Result<Option<u64>, String> {
    let (prepared, title) = {
        let session = app.state::<AppState>().session();
        (
            prepare_export(&session.project)?,
            session.project.active_sequence().name.clone(),
        )
    };
    let Some(mut output) = rfd::FileDialog::new()
        .add_filter("MP4", &["mp4"])
        .set_file_name(format!("{title}_export.mp4"))
        .save_file()
    else {
        return Ok(None);
    };
    if output.exists() {
        let overwrite = rfd::MessageDialog::new()
            .set_title("Export output already exists")
            .set_description(
                "Overwrite the existing file? Choose No to create an auto-renamed file.",
            )
            .set_buttons(rfd::MessageButtons::YesNo)
            .show();
        if overwrite != rfd::MessageDialogResult::Yes {
            output = avcore::next_available_path(&output);
        }
    }
    Ok(Some(app.state::<AppState>().exports.enqueue(
        app.clone(),
        title,
        output,
        prepared,
    )))
}

#[tauri::command]
pub fn pause_export(app: AppHandle, job_id: u64) {
    app.state::<AppState>().exports.pause(app, job_id);
}
#[tauri::command]
pub fn resume_export(app: AppHandle, job_id: u64) {
    app.state::<AppState>().exports.resume(app, job_id);
}
#[tauri::command]
pub fn cancel_export(app: AppHandle, job_id: u64) {
    app.state::<AppState>().exports.cancel(app, job_id);
}
#[tauri::command]
pub fn remove_export(app: AppHandle, job_id: u64) {
    app.state::<AppState>().exports.remove(app, job_id);
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
        let session = app.state::<AppState>().session();
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
    std::fs::write(PathBuf::from(path), srt).map_err(|error| error.to_string())?;
    Ok(true)
}
#[tauri::command]
pub fn list_exports(state: State<'_, AppState>) -> Vec<crate::export::ExportJobView> {
    state.exports.snapshot()
}
