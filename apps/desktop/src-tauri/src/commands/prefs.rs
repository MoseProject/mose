//! Preferences, recent-project and recovery commands.

use std::path::PathBuf;

use avcore::{ClipFormatting, ClipInstance, LayerTemplate, TrackKind};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::autosave;
use crate::commands::undoable;
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
    let telemetry_enabled = prefs.telemetry_enabled;
    {
        let mut document = state.prefs();
        document.view = prefs;
        document.save()?;
    }
    state
        .telemetry_enabled
        .store(telemetry_enabled, std::sync::atomic::Ordering::Relaxed);
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

/// Snapshots `clip_ids`' per-layer `(TrackKind, ClipFormatting)`, ordered by track position then
/// start time, and saves it as a new named [`LayerTemplate`] in `prefs.saved_layer_templates`.
/// Returns the updated list so the caller doesn't need a second `get_prefs` round trip. A no-op
/// (returning the list unchanged) if `name` is blank or no clip in `clip_ids` is found.
#[tauri::command]
pub fn save_layer_template(
    app: AppHandle,
    name: String,
    clip_ids: Vec<u64>,
) -> Result<Vec<LayerTemplate>, String> {
    let state = app.state::<AppState>();
    let name = name.trim().to_string();
    let mut layers: Vec<(usize, f64, TrackKind, ClipFormatting)> = Vec::new();
    {
        let session = state.session();
        for (track_index, track) in session.project.timeline().tracks.iter().enumerate() {
            for clip in &track.clips {
                if clip_ids.contains(&clip.id) {
                    layers.push((track_index, clip.start_secs, track.kind, clip.formatting()));
                }
            }
        }
    }
    if name.is_empty() || layers.is_empty() {
        return Ok(state.prefs().view.saved_layer_templates.clone());
    }
    layers.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    let layers = layers
        .into_iter()
        .map(|(_, _, kind, f)| (kind, f))
        .collect();
    let mut prefs = state.prefs();
    prefs
        .view
        .saved_layer_templates
        .push(LayerTemplate { name, layers });
    prefs.save()?;
    Ok(prefs.view.saved_layer_templates.clone())
}

/// Removes `prefs.saved_layer_templates[index]`. Returns the updated list; unchanged (`index`
/// out of range) if there was nothing to remove there.
#[tauri::command]
pub fn delete_layer_template(app: AppHandle, index: usize) -> Result<Vec<LayerTemplate>, String> {
    let state = app.state::<AppState>();
    let mut prefs = state.prefs();
    if index < prefs.view.saved_layer_templates.len() {
        prefs.view.saved_layer_templates.remove(index);
        prefs.save()?;
    }
    Ok(prefs.view.saved_layer_templates.clone())
}

/// Applies `prefs.saved_layer_templates[template_index]`: for every layer with an asset chosen
/// in `asset_ids` (by layer position, `None` entries skipped), creates a brand-new track of that
/// layer's kind, adds a clip for the chosen asset at the playhead, then applies the layer's
/// saved [`ClipFormatting`] onto it -- one undo step for the whole template. A no-op if
/// `template_index` is out of range or every slot is empty.
#[tauri::command]
pub fn apply_layer_template(
    app: AppHandle,
    template_index: usize,
    asset_ids: Vec<Option<u64>>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let Some(template) = state
        .prefs()
        .view
        .saved_layer_templates
        .get(template_index)
        .cloned()
    else {
        return Ok(());
    };
    let playhead_secs = state.preview.playhead();
    undoable(&app, |project| {
        let mut changed = false;
        for (layer_formatting, asset_id) in template
            .layers
            .iter()
            .map(|(_, formatting)| formatting)
            .zip(asset_ids.iter())
        {
            let Some(asset_id) = asset_id else { continue };
            let Some(asset) = project.media_library.iter().find(|a| a.id == *asset_id) else {
                continue;
            };
            let kind = match asset.kind {
                avcore::MediaKind::Video => TrackKind::Video,
                avcore::MediaKind::Audio => TrackKind::Audio,
            };
            let duration_secs = asset.duration_secs;
            let asset_id = *asset_id;
            let timeline = project.timeline_mut();
            let track_index = timeline.create_track(kind);
            let clip_id = timeline.next_clip_id();
            let mut clip = ClipInstance::new(clip_id, asset_id, playhead_secs, 0.0, duration_secs);
            clip.apply_formatting(layer_formatting);
            timeline.tracks[track_index].clips.push(clip);
            changed = true;
        }
        changed
    })
}
