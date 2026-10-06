use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use avcore::GpuEncoderPreference;
use tauri::{AppHandle, Manager, Runtime, State};

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
pub async fn pick_export_path<R: Runtime>(
    app: AppHandle<R>,
) -> Result<Option<ExportTarget>, String> {
    let title = {
        let state = app.state::<AppState>();
        let session = state.session();
        session.project.active_sequence().name.clone()
    };
    Ok(
        crate::dialogs::save_file("MP4", &["mp4"], &format!("{title}_export.mp4")).map(|path| {
            ExportTarget {
                exists: path.exists(),
                path: path.display().to_string(),
            }
        }),
    )
}

/// Queues the active sequence for export to `path`. With `rename`, an existing file is kept and
/// the output goes to the next free `<stem> (n)` sibling instead of overwriting it.
#[tauri::command]
pub async fn start_export<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    rename: bool,
) -> Result<u64, String> {
    let permit = {
        let state = app.state::<AppState>();
        ExportPreparationPermit::acquire(Arc::clone(&state.export_preparing))?
    };
    let project = {
        let state = app.state::<AppState>();
        let session = state.session();
        session.project.clone()
    };
    let (reply, result) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("export-preparation".to_string())
        .spawn(move || {
            let prepared = prepare_and_enqueue_export(app, project, path, rename);
            drop(permit);
            let _ = reply.send(prepared);
        })
        .map_err(|error| format!("failed to start export preparation: {error}"))?;
    result
        .await
        .map_err(|_| "export preparation stopped before completion".to_string())?
}

/// Releases admission on success, early errors, a failed spawn, or a worker panic.
struct ExportPreparationPermit(Arc<AtomicBool>);

impl ExportPreparationPermit {
    fn acquire(running: Arc<AtomicBool>) -> Result<Self, String> {
        running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "exportPreparationBusy".to_string())?;
        Ok(Self(running))
    }
}

impl Drop for ExportPreparationPermit {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

fn prepare_and_enqueue_export<R: Runtime>(
    app: AppHandle<R>,
    mut project: avcore::Project,
    path: String,
    rename: bool,
) -> Result<u64, String> {
    let mut output = PathBuf::from(path);
    if rename && output.exists() {
        output = avcore::next_available_path(&output);
    }
    let exports = app.state::<AppState>().exports.clone();
    check_output_is_free(&app, &exports, &output, &project)?;
    let title = project.active_sequence().name.clone();
    let nested_assets = {
        let state = app.state::<AppState>();
        let cache_dir = avcore::nested_sequence::cache_dir_for_project(&project);
        let mut cache = crate::state::lock_or_recover(&state.nested_sequence_cache);
        avcore::nested_sequence::materialize_nested_sequences(
            &project,
            project.timeline(),
            &cache_dir,
            &mut cache,
        )
        .map_err(|error| error.to_string())?
    };
    project.media_library.extend(nested_assets.into_values());
    let prepared = prepare_export(&project)?;
    // Project switches during preparation do not change the requested export. Protect both
    // that snapshot's inputs (including materialized children) and the currently open media.
    check_output_is_free(&app, &exports, &output, &project)?;
    Ok(exports.enqueue(app.clone(), title, output, prepared))
}

/// Whether two paths name the same file: by identity when both exist, otherwise by their text
/// (ignoring case on Windows, whose file system does).
fn same_file(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (a.canonicalize(), b.canonicalize()) {
        return a == b;
    }
    if cfg!(windows) {
        a.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

/// Refuses an output that would destroy something in use: a media file of the project (the
/// encoder would overwrite what it is reading) or the output of a job still queued or running.
fn check_output_is_free<R: Runtime>(
    app: &AppHandle<R>,
    exports: &crate::export::ExportQueue,
    output: &Path,
    snapshot: &avcore::Project,
) -> Result<(), String> {
    let (current_sources, current_nested_cache): (Vec<PathBuf>, PathBuf) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            source_paths(&session.project)
                .map(Path::to_path_buf)
                .collect(),
            avcore::nested_sequence::cache_dir_for_project(&session.project),
        )
    };
    let snapshot_nested_cache = avcore::nested_sequence::cache_dir_for_project(snapshot);
    // Reject cache destinations before materialization can write its own output there.
    let is_cache_output = output.parent().is_some_and(|parent| {
        same_file(parent, &snapshot_nested_cache) || same_file(parent, &current_nested_cache)
    });
    let is_source = source_paths(snapshot)
        .chain(current_sources.iter().map(PathBuf::as_path))
        .any(|source| same_file(source, output));
    if is_source || is_cache_output {
        return Err("outputIsSource".to_string());
    }
    let busy = exports.snapshot().iter().any(|job| {
        !matches!(
            job.status,
            avcore::ExportJobStatus::Done | avcore::ExportJobStatus::Failed { .. }
        ) && same_file(Path::new(&job.output_path), output)
    });
    if busy {
        return Err("outputInQueue".to_string());
    }
    Ok(())
}

fn source_paths(project: &avcore::Project) -> impl Iterator<Item = &Path> {
    project.media_library.iter().flat_map(|asset| {
        std::iter::once(asset.source_path.as_path()).chain(asset.proxy_path.as_deref())
    })
}

#[tauri::command]
pub fn pause_export<R: Runtime>(app: AppHandle<R>, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.pause(app, job_id);
}
#[tauri::command]
pub fn resume_export<R: Runtime>(app: AppHandle<R>, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.resume(app, job_id);
}
#[tauri::command]
pub fn cancel_export<R: Runtime>(app: AppHandle<R>, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.cancel(app, job_id);
}
#[tauri::command]
pub fn remove_export<R: Runtime>(app: AppHandle<R>, job_id: u64) {
    let exports = app.state::<AppState>().exports.clone();
    exports.remove(app, job_id);
}
#[tauri::command]
pub fn match_export_loudness<R: Runtime>(app: AppHandle<R>, target_lufs: f32) {
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
pub async fn export_active_sequence_srt<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let (srt, title) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            avcore::export_srt(session.project.timeline()),
            session.project.active_sequence().name.clone(),
        )
    };
    let Some(path) = crate::dialogs::save_file("SubRip", &["srt"], &format!("{title}.srt")) else {
        return Ok(false);
    };
    avcore::write_atomic(std::path::Path::new(&path), srt.as_bytes())
        .map_err(|error| error.to_string())?;
    Ok(true)
}
#[tauri::command]
pub fn list_exports(state: State<'_, AppState>) -> Vec<crate::export::ExportJobView> {
    state.exports.snapshot()
}

#[cfg(test)]
#[path = "export_test.rs"]
mod tests;
