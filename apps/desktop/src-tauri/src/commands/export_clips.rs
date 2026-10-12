use std::path::PathBuf;

use avcore::timeline::TrackKind;
use tauri::{AppHandle, Manager, Runtime, State};

use super::export::{check_output_is_free, prepare_for, sanitize_file_name, ExportTarget};
use crate::export::ExportOptions;
use crate::state::AppState;

/// One selected clip and the file its export is written to.
struct ClipOutput {
    clip_id: u64,
    path: PathBuf,
}

/// The files the selected video clips are exported to, in timeline order: `<prefix> - <media
/// name> <nn>.mp4` inside `folder` (the prefix part is left out when empty), numbered so two clips
/// of one media file never share a name. Clips that are not on a video track are skipped. Errors
/// are codes the webview translates (`noFolder`, `folderMissing`, `noClips`).
fn clip_outputs(
    state: &AppState,
    clip_ids: &[u64],
    folder: &str,
    prefix: &str,
) -> Result<Vec<ClipOutput>, String> {
    if folder.trim().is_empty() {
        return Err("noFolder".to_string());
    }
    let folder = PathBuf::from(folder.trim());
    if !folder.is_dir() {
        return Err("folderMissing".to_string());
    }
    let prefix = sanitize_file_name(prefix);
    let session = state.session();
    let sequence = session.project.active_sequence();
    let mut found: Vec<(f64, usize, u64, u64)> = Vec::new();
    for (row, track) in sequence.timeline.tracks.iter().enumerate() {
        if track.kind != TrackKind::Video {
            continue;
        }
        for clip in &track.clips {
            if clip_ids.contains(&clip.id) {
                found.push((clip.start_secs, row, clip.id, clip.asset_id));
            }
        }
    }
    found.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    if found.is_empty() {
        return Err("noClips".to_string());
    }
    let width = found.len().to_string().len().max(2);
    Ok(found
        .into_iter()
        .enumerate()
        .map(|(index, (_, _, clip_id, asset_id))| {
            let media = session
                .project
                .media_library
                .iter()
                .find(|asset| asset.id == asset_id)
                .map(|asset| {
                    let name = std::path::Path::new(&asset.file_name)
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default();
                    sanitize_file_name(&name)
                })
                .unwrap_or_default();
            let number = format!("{:0width$}", index + 1);
            let name = [prefix.as_str(), media.as_str(), number.as_str()]
                .iter()
                .filter(|part| !part.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" - ");
            ClipOutput {
                clip_id,
                path: folder.join(format!("{name}.mp4")),
            }
        })
        .collect())
}

/// Where each selected clip would be exported, and which of those files already exist, so the
/// dialog can ask once before replacing any.
#[tauri::command]
pub fn export_clips_targets(
    state: State<'_, AppState>,
    clip_ids: Vec<u64>,
    folder: String,
    prefix: String,
) -> Result<Vec<ExportTarget>, String> {
    Ok(clip_outputs(&state, &clip_ids, &folder, &prefix)?
        .into_iter()
        .map(|output| ExportTarget {
            exists: output.path.exists(),
            path: output.path.display().to_string(),
        })
        .collect())
}

/// Queues every selected video clip as its own export, each with its effects but without the
/// titles and shapes of other tracks. With `rename`, a file that already exists is kept and its
/// export goes to the next free `<stem> (n)` sibling. Everything is prepared before anything is
/// queued, so a failure leaves the queue as it was. Returns the job ids in timeline order.
#[tauri::command]
pub fn start_export_clips<R: Runtime>(
    app: AppHandle<R>,
    clip_ids: Vec<u64>,
    folder: String,
    prefix: String,
    rename: bool,
    options: Option<ExportOptions>,
) -> Result<Vec<u64>, String> {
    let options = options.unwrap_or_default();
    let exports = app.state::<AppState>().exports.clone();
    let outputs = clip_outputs(&app.state::<AppState>(), &clip_ids, &folder, &prefix)?;
    let mut ready = Vec::with_capacity(outputs.len());
    for output in outputs {
        let (prepared, title) = prepare_for(&app, options, Some(output.clip_id))?;
        let mut path = output.path;
        if rename && path.exists() {
            path = avcore::next_available_path(&path);
        }
        check_output_is_free(&app, &exports, &path)?;
        ready.push((title, path, prepared));
    }
    Ok(ready
        .into_iter()
        .map(|(title, path, prepared)| exports.enqueue(app.clone(), title, path, prepared))
        .collect())
}

#[cfg(test)]
#[path = "export_clips_test.rs"]
mod tests;
