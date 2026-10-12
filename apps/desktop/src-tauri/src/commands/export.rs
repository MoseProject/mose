use std::path::{Path, PathBuf};

use avcore::GpuEncoderPreference;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::export::{prepare_export_with, ExportOptions, PreparedExport};
use crate::state::AppState;

/// The output file the user picked for an export, and whether it already exists -- the webview
/// asks overwrite-or-rename with its own translated text before calling [`start_export`].
#[derive(Debug, serde::Serialize)]
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

/// The extensions an export may be written with: video, then the audio-only formats.
const EXPORT_EXTENSIONS: [&str; 5] = ["mp4", "wav", "flac", "mp3", "opus"];

/// The file an export dialog's folder and name field point at: the folder must exist, the name
/// loses anything a file name cannot hold and gains `extension` (`mp4` by default). Errors are
/// codes the webview translates (`noFolder`, `folderMissing`, `noName`, `badExtension`).
#[tauri::command]
pub fn export_target(
    folder: String,
    file_name: String,
    extension: Option<String>,
) -> Result<ExportTarget, String> {
    let extension = match extension.as_deref() {
        None => "mp4",
        Some(ext) if EXPORT_EXTENSIONS.contains(&ext) => ext,
        Some(_) => return Err("badExtension".to_string()),
    };
    if folder.trim().is_empty() {
        return Err("noFolder".to_string());
    }
    let folder = PathBuf::from(folder.trim());
    if !folder.is_dir() {
        return Err("folderMissing".to_string());
    }
    let name = sanitize_file_name(&file_name);
    if name.is_empty() {
        return Err("noName".to_string());
    }
    let path = folder.join(format!("{name}.{extension}"));
    Ok(ExportTarget {
        exists: path.exists(),
        path: path.display().to_string(),
    })
}

/// A file name stem with path separators and the characters Windows forbids replaced, and no
/// trailing dots, spaces or export extension.
pub(crate) fn sanitize_file_name(raw: &str) -> String {
    let cleaned: String = raw
        .trim()
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let stem = match cleaned.rsplit_once('.') {
        Some((stem, ext))
            if EXPORT_EXTENSIONS
                .iter()
                .any(|known| ext.eq_ignore_ascii_case(known)) =>
        {
            stem
        }
        _ => cleaned.as_str(),
    };
    stem.trim_end_matches(['.', ' ']).to_string()
}

/// What the export would produce for the dialog's aspect ratio before any option: size, the
/// source-matched bitrate and the length, so the dialog can show defaults and a size estimate.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCanvas {
    pub width: u32,
    pub height: u32,
    pub bitrate_mbps: f32,
    pub duration_secs: f64,
}

/// With `clip_ids`, the size and bitrate come from the first of those clips and the length is
/// the sum of them all: what exporting each as its own video produces in total.
#[tauri::command]
pub fn export_canvas(
    state: State<'_, AppState>,
    aspect_ratio: avcore::ExportAspectRatio,
    clip_ids: Option<Vec<u64>>,
) -> Result<ExportCanvas, String> {
    let session = state.session();
    let sequence = session.project.active_sequence();
    let sequences: Vec<avcore::Sequence> = match clip_ids {
        None => vec![sequence.clone()],
        Some(ids) => ids
            .iter()
            .filter_map(|id| sequence.isolating_clip(*id))
            .collect(),
    };
    let mut canvas = None;
    let mut duration_secs = 0.0;
    for sequence in &sequences {
        let (tracks, resolved) =
            avcore::resolve_timeline_segments_multi(sequence, &session.project.media_library)
                .map_err(|e| e.to_string())?;
        canvas.get_or_insert(resolved);
        duration_secs += tracks
            .iter()
            .map(|track| {
                track
                    .iter()
                    .map(|s| {
                        (s.source_out_secs - s.source_in_secs) / f64::from(s.speed_factor.max(0.01))
                    })
                    .sum::<f64>()
            })
            .fold(0.0, f64::max);
    }
    let canvas = canvas.ok_or_else(|| "noClips".to_string())?;
    let canvas = avcore::apply_export_aspect_ratio(canvas, aspect_ratio);
    Ok(ExportCanvas {
        width: canvas.width,
        height: canvas.height,
        bitrate_mbps: (canvas.bit_rate_bps as f64 / 1_000_000.0) as f32,
        duration_secs,
    })
}

/// Queues the active sequence for export to `path`. With `rename`, an existing file is kept and
/// the output goes to the next free `<stem> (n)` sibling instead of overwriting it. `options`
/// carries the dialog's bitrate and resolution ceiling. Off the main thread: a compound clip has
/// its nested sequence rendered first, which can take seconds.
#[tauri::command(async)]
pub fn start_export<R: Runtime>(
    app: AppHandle<R>,
    path: String,
    rename: bool,
    options: Option<ExportOptions>,
) -> Result<u64, String> {
    let (prepared, title) = prepare_for(&app, options.unwrap_or_default(), None)?;
    let mut output = PathBuf::from(path);
    if rename && output.exists() {
        output = avcore::next_available_path(&output);
    }
    let exports = app.state::<AppState>().exports.clone();
    check_output_is_free(&app, &exports, &output)?;
    Ok(exports.enqueue(app, title, output, prepared))
}

/// What the active sequence's export renders, or with `only_clip` just that one clip alone,
/// plus the title its job gets. A compound clip needs its nested sequence rendered and merged in
/// first -- same synchronous, blocks-this-call tradeoff `ui::App`'s own export path takes (see
/// `avcore::nested_sequence`'s doc comment), cached across exports via
/// `AppState::nested_sequence_cache` so re-exporting the same sequence doesn't re-render
/// unchanged nested content.
pub(crate) fn prepare_for<R: Runtime>(
    app: &AppHandle<R>,
    options: ExportOptions,
    only_clip: Option<u64>,
) -> Result<(PreparedExport, String), String> {
    let state = app.state::<AppState>();
    let session = state.session();
    let mut base = session.project.clone();
    let mut title = base.active_sequence().name.clone();
    if let Some(clip_id) = only_clip {
        let alone = base
            .active_sequence()
            .isolating_clip(clip_id)
            .ok_or_else(|| "clipMissing".to_string())?;
        let asset_id = alone.timeline.tracks[0].clips[0].asset_id;
        if let Some(asset) = base.media_library.iter().find(|a| a.id == asset_id) {
            title = format!("{title} - {}", asset.file_name);
        }
        *base.active_sequence_mut() = alone;
    }
    let cache_dir = avcore::nested_sequence::cache_dir_for_project(&base);
    let mut cache = crate::state::lock_or_recover(&state.nested_sequence_cache);
    let nested_assets = avcore::nested_sequence::materialize_nested_sequences(
        &base,
        base.timeline(),
        &cache_dir,
        &mut cache,
    )
    .map_err(|e| e.to_string())?;
    base.media_library.extend(nested_assets.into_values());
    Ok((prepare_export_with(&base, options)?, title))
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
pub(crate) fn check_output_is_free<R: Runtime>(
    app: &AppHandle<R>,
    exports: &crate::export::ExportQueue,
    output: &Path,
) -> Result<(), String> {
    let is_source = {
        let state = app.state::<AppState>();
        let session = state.session();
        session.project.media_library.iter().any(|asset| {
            same_file(&asset.source_path, output)
                || asset
                    .proxy_path
                    .as_deref()
                    .is_some_and(|proxy| same_file(proxy, output))
        })
    };
    if is_source {
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
pub fn set_normalize_audio(state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    state.exports.set_normalize_audio(enabled)
}
#[tauri::command]
pub fn get_normalize_audio(state: State<'_, AppState>) -> bool {
    state.exports.normalize_audio()
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
