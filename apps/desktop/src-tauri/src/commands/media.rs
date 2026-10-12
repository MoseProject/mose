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
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use super::{import_gate, with_session};
use crate::state::{publish, AppState, Session};

/// Emitted when a file could not be read as media, so the import does not fail in silence.
pub const IMPORT_FAILED_EVENT: &str = "import-failed";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportFailedPayload {
    pub file_name: String,
}

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

/// Removes an asset from the media library. Refused with `assetInUse` while any clip still
/// plays it, so the timeline never ends up with a dangling reference.
#[tauri::command]
pub fn remove_asset<R: Runtime>(app: AppHandle<R>, asset_id: u64) -> Result<(), String> {
    with_session(&app, |session| {
        if session.project.asset_in_use(asset_id) {
            return Err("assetInUse".to_string());
        }
        if session.project.remove_asset(asset_id) {
            session.dirty = true;
            // An older undo step could bring back a clip of this asset, which is gone now.
            session.undo.clear();
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

/// Imports one file and returns the id of its asset, or `None` when it could not be read (or the
/// project was replaced meanwhile).
fn import_one<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
    epoch: u64,
    proxy_dir: &Path,
) -> Option<u64> {
    let probed = match avcore::probe_media(path) {
        Ok(probed) => probed,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to probe media");
            let file_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let _ = app.emit(IMPORT_FAILED_EVENT, ImportFailedPayload { file_name });
            return None;
        }
    };
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();

    let audio_streams = probed.streams.audio.clone();
    let Some((asset_id, kind)) = with_current_project(app, epoch, |session| {
        let id = session.allocate_asset_id();
        let asset = probed.into_media_asset(id, file_name, path.to_path_buf());
        let kind = asset.kind;
        session.project.media_library.push(asset);
        session.dirty = true;
        (id, kind)
    }) else {
        return None;
    };

    // A recording with several audio tracks (OBS: game, microphone, chat) gets one audio asset per
    // track after the first, which stays with the video.
    if audio_streams.len() > 1 {
        import_extra_tracks(app, path, audio_streams, epoch, proxy_dir);
    }

    // Cheapest and most visible first, each result published as soon as it exists: on a
    // multi-hour recording the proxy encode takes far longer than everything else together, and
    // the timeline's waveform used to wait behind it.
    // Many files imported together take turns (see `import_gate`): their audio passes a few at a
    // time, then their proxies a few at a time, in the order they were imported.
    {
        let _turn = import_gate::analysis_gate().enter();
        let waveform_peaks = avcore::generate_waveform(path)
            .inspect_err(|error| tracing::warn!(%error, "failed to compute waveform"))
            .ok();
        update_asset(app, epoch, asset_id, |asset| {
            asset.waveform_peaks = waveform_peaks
        });

        let loudness = avcore::measure_loudness(path)
            .inspect_err(|error| tracing::warn!(%error, "failed to measure loudness"))
            .ok();
        update_asset(app, epoch, asset_id, |asset| asset.loudness = loudness);
    }

    if kind == MediaKind::Video {
        let _turn = import_gate::proxy_gate().enter();
        let proxy_path = avcore::ensure_proxy(path, proxy_dir, PreviewQuality::default())
            .inspect_err(|error| tracing::warn!(%error, "failed to generate proxy"))
            .ok();
        update_asset(app, epoch, asset_id, |asset| asset.proxy_path = proxy_path);
    }
    Some(asset_id)
}

/// Copies every audio track of `path` after the first out as a file of its own and imports each as
/// an audio asset named after the track. Runs on one background thread, a track at a time: each
/// copy reads the whole recording.
fn import_extra_tracks<R: Runtime>(
    app: &AppHandle<R>,
    path: &Path,
    audio_streams: Vec<avcore::AudioStreamInfo>,
    epoch: u64,
    proxy_dir: &Path,
) {
    let app = app.clone();
    let origin = path.to_path_buf();
    let proxy_dir = proxy_dir.to_path_buf();
    std::thread::spawn(move || {
        let stem = origin
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        for (position, stream) in audio_streams.iter().enumerate().skip(1) {
            let copy = {
                let _turn = import_gate::analysis_gate().enter();
                avcore::ensure_audio_stream_file(&origin, stream.stream_index, &proxy_dir)
            };
            let copy = match copy {
                Ok(copy) => copy,
                Err(error) => {
                    tracing::warn!(%error, track = stream.stream_index, "failed to copy an audio track");
                    continue;
                }
            };
            let Some(asset_id) = import_one(&app, &copy, epoch, &proxy_dir) else {
                continue;
            };
            let label = if stream.title.is_empty() {
                format!("Track {}", position + 1)
            } else {
                stream.title.clone()
            };
            let source = avcore::AudioStreamSource {
                origin: origin.clone(),
                stream_index: stream.stream_index,
            };
            update_asset(&app, epoch, asset_id, |asset| {
                asset.file_name = format!("{stem} - {label}");
                asset.audio_stream = Some(source);
            });
        }
    });
}

fn update_asset<R: Runtime>(
    app: &AppHandle<R>,
    epoch: u64,
    asset_id: u64,
    change: impl FnOnce(&mut avcore::MediaAsset),
) {
    with_current_project(app, epoch, |session| {
        if let Some(asset) = session
            .project
            .media_library
            .iter_mut()
            .find(|a| a.id == asset_id)
        {
            change(asset);
        }
    });
}

/// Re-attaches editing proxies after a project load. `ensure_proxy` returns an existing cache
/// entry without re-encoding, so this is cheap for media proxied before.
pub fn rehydrate_proxies<R: Runtime>(app: &AppHandle<R>, epoch: u64) {
    rehydrate_audio_tracks(app, epoch);
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

/// Makes again the audio-track copies a loaded project lost (a cleared cache, another machine):
/// each asset made from a track of a recording whose own file is gone is copied out of the
/// recording again and pointed at the new file.
fn rehydrate_audio_tracks<R: Runtime>(app: &AppHandle<R>, epoch: u64) {
    let (lost, proxy_dir) = {
        let state = app.state::<AppState>();
        let session = state.session();
        let lost: Vec<(u64, avcore::AudioStreamSource)> = session
            .project
            .media_library
            .iter()
            .filter_map(|asset| {
                let source = asset.audio_stream.clone()?;
                (!asset.source_path.exists() && source.origin.exists())
                    .then_some((asset.id, source))
            })
            .collect();
        (lost, avcore::proxy::cache_dir_for_project(&session.project))
    };
    if lost.is_empty() {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        for (asset_id, source) in lost {
            let Ok(copy) =
                avcore::ensure_audio_stream_file(&source.origin, source.stream_index, &proxy_dir)
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
                    asset.source_path = copy;
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
