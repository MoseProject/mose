// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Media import, in the egui shell's two phases (`ui::app::import::import_one`): a cheap probe
//! that makes the asset usable immediately, then a background enrichment pass (loudness,
//! editing proxy, waveform) that patches the asset in place when it finishes.

use std::path::{Path, PathBuf};

use avcore::{MediaKind, PreviewQuality};
use tauri::{AppHandle, Manager, Runtime};

use super::with_session;
use crate::state::{publish, AppState, Session};

const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "m4a", "aac", "flac", "ogg", "opus"];

/// Shows a multi-select file dialog and imports the chosen files.
#[tauri::command]
pub async fn import_media_dialog<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let mut extensions: Vec<&str> = avcore::VIDEO_EXTENSIONS.to_vec();
    extensions.extend_from_slice(AUDIO_EXTENSIONS);
    let Some(paths) = crate::dialogs::pick_files("Media", &extensions) else {
        return Ok(());
    };
    import_paths(&app, paths);
    Ok(())
}

/// Imports files dropped onto the window (paths come from Tauri's drag-drop event).
#[tauri::command]
pub fn import_media<R: Runtime>(app: AppHandle<R>, paths: Vec<String>) {
    import_paths(&app, paths.into_iter().map(PathBuf::from).collect());
}

#[tauri::command]
pub fn toggle_favorite<R: Runtime>(app: AppHandle<R>, asset_id: u64) -> Result<(), String> {
    with_session(&app, |session| {
        if let Some(asset) = session
            .project
            .media_library
            .iter_mut()
            .find(|asset| asset.id == asset_id)
        {
            asset.favorited = !asset.favorited;
            session.dirty = true;
        }
        Ok(())
    })
}

fn import_paths<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    let (epoch, proxy_dir) = {
        let state = app.state::<AppState>();
        let session = state.session();
        (
            session.project_epoch,
            avcore::proxy::cache_dir_for_project(&session.project),
        )
    };
    for path in paths {
        let app = app.clone();
        let proxy_dir = proxy_dir.clone();
        std::thread::spawn(move || import_one(&app, &path, epoch, &proxy_dir));
    }
}

fn import_one<R: Runtime>(app: &AppHandle<R>, path: &Path, epoch: u64, proxy_dir: &Path) {
    let probed = match avcore::probe_media(path) {
        Ok(probed) => probed,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to probe media");
            return;
        }
    };
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let Some((asset_id, kind)) = with_current_project(app, epoch, |session| {
        let id = session
            .project
            .media_library
            .iter()
            .map(|a| a.id)
            .max()
            .unwrap_or(0)
            + 1;
        let asset = probed.into_media_asset(id, file_name, path.to_path_buf());
        let kind = asset.kind;
        session.project.media_library.push(asset);
        session.dirty = true;
        (id, kind)
    }) else {
        return;
    };

    let loudness = avcore::measure_loudness(path)
        .inspect_err(|error| tracing::warn!(%error, "failed to measure loudness"))
        .ok();
    let proxy_path = if kind == MediaKind::Video {
        avcore::ensure_proxy(path, proxy_dir, PreviewQuality::default())
            .inspect_err(|error| tracing::warn!(%error, "failed to generate proxy"))
            .ok()
    } else {
        None
    };
    let waveform_peaks = avcore::generate_waveform(path)
        .inspect_err(|error| tracing::warn!(%error, "failed to compute waveform"))
        .ok();

    with_current_project(app, epoch, |session| {
        if let Some(asset) = session
            .project
            .media_library
            .iter_mut()
            .find(|a| a.id == asset_id)
        {
            asset.loudness = loudness;
            asset.proxy_path = proxy_path;
            asset.waveform_peaks = waveform_peaks;
        }
    });
}

/// Re-attaches editing proxies after a project load. `ensure_proxy` returns an existing cache
/// entry without re-encoding, so this is cheap for media proxied before.
pub fn rehydrate_proxies<R: Runtime>(app: &AppHandle<R>, epoch: u64) {
    let (sources, proxy_dir) = {
        let state = app.state::<AppState>();
        let session = state.session();
        let sources: Vec<(u64, PathBuf)> = session
            .project
            .media_library
            .iter()
            .filter(|a| a.kind == MediaKind::Video && a.proxy_path.is_none())
            .map(|a| (a.id, a.source_path.clone()))
            .collect();
        (
            sources,
            avcore::proxy::cache_dir_for_project(&session.project),
        )
    };
    if sources.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for (asset_id, source) in sources {
            if !source.exists() {
                continue;
            }
            let Ok(proxy) = avcore::ensure_proxy(&source, &proxy_dir, PreviewQuality::default())
            else {
                continue;
            };
            let applied = with_current_project(&app, epoch, |session| {
                if let Some(asset) = session
                    .project
                    .media_library
                    .iter_mut()
                    .find(|a| a.id == asset_id)
                {
                    asset.proxy_path = Some(proxy);
                }
            });
            if applied.is_none() {
                return;
            }
        }
    });
}

/// Applies `change` only if the project open when the background work started is still the
/// open one, then publishes. `None` when the project was replaced in the meantime.
fn with_current_project<R: Runtime, T>(
    app: &AppHandle<R>,
    epoch: u64,
    change: impl FnOnce(&mut Session) -> T,
) -> Option<T> {
    let state = app.state::<AppState>();
    let mut session = state.session();
    if session.project_epoch != epoch {
        return None;
    }
    let out = change(&mut session);
    publish(app, &session);
    Some(out)
}

#[cfg(test)]
#[path = "media_test.rs"]
mod tests;
