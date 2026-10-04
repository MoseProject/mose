//! YouTube download via the isolated `ytbridge` binary -- background-thread download +
//! post-download import, mirroring `ui::App`'s `youtube_download.rs` exactly. No core changes
//! needed.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use avcore::{Mp3Bitrate, Mp4Quality, YoutubeDownloadTarget};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::state::AppState;

pub const YOUTUBE_DOWNLOAD_EVENT: &str = "youtube-download-event";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum YoutubeDownloadEventPayload {
    Progress { fraction: f32 },
    Done,
    Failed { message: String },
}

fn parse_target(format: &str, quality: &str) -> Option<YoutubeDownloadTarget> {
    match format {
        "mp4" => {
            let q = match quality {
                "360" => Mp4Quality::P360,
                "480" => Mp4Quality::P480,
                "720" => Mp4Quality::P720,
                "1080" => Mp4Quality::P1080,
                _ => Mp4Quality::Best,
            };
            Some(YoutubeDownloadTarget::Mp4(q))
        }
        "mp3" => {
            let b = match quality {
                "128" => Mp3Bitrate::K128,
                "320" => Mp3Bitrate::K320,
                _ => Mp3Bitrate::K192,
            };
            Some(YoutubeDownloadTarget::Mp3(b))
        }
        _ => None,
    }
}

/// Whether `ytbridge` (and yt-dlp) is available -- checked before offering the download modal's
/// "Baixar" button.
#[tauri::command]
pub fn is_yt_dlp_available(state: State<'_, AppState>) -> bool {
    state.services.is_yt_dlp_available()
}

/// Downloads `url` on a background thread at `format`/`quality` (`format`: `"mp4"`|`"mp3"`;
/// `quality`: `"360"`/`"480"`/`"720"`/`"1080"`/`"best"` for mp4, `"128"`/`"192"`/`"320"` for
/// mp3) -- what the download modal's "Baixar" button does. On success the file is imported into
/// the media library like any other. Errors (already running, blank URL, tool missing) surface
/// synchronously; progress/result arrive via [`YOUTUBE_DOWNLOAD_EVENT`].
#[tauri::command]
pub fn download_youtube<R: Runtime>(
    app: AppHandle<R>,
    url: String,
    format: String,
    quality: String,
) -> Result<(), String> {
    let url = url.trim().to_string();
    if url.is_empty() {
        return Err("blank URL".to_string());
    }
    let Some(target) = parse_target(&format, &quality) else {
        return Err("invalid format".to_string());
    };
    let services = app.state::<AppState>().services.clone();
    if !services.is_yt_dlp_available() {
        return Err("yt-dlp is not installed".to_string());
    }
    let state = app.state::<AppState>();
    {
        let mut running = state
            .youtube_download_cancel
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if running.is_some() {
            return Err("a download is already running".to_string());
        }
        let cancel = Arc::new(AtomicBool::new(false));
        *running = Some(Arc::clone(&cancel));
    }
    let out_dir = youtube_downloads_dir();

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let cancel = {
            let state = worker_app.state::<AppState>();
            let guard = state
                .youtube_download_cancel
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            guard.clone().expect("cancel flag set just above")
        };
        let progress_app = worker_app.clone();
        let result = services.download_youtube(&url, target, &out_dir, &cancel, &mut |fraction| {
            let _ = progress_app.emit(
                YOUTUBE_DOWNLOAD_EVENT,
                YoutubeDownloadEventPayload::Progress { fraction },
            );
        });
        {
            let state = worker_app.state::<AppState>();
            *state
                .youtube_download_cancel
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = None;
        }
        match result {
            Ok(path) => {
                crate::commands::media::import_media(
                    worker_app.clone(),
                    vec![path.display().to_string()],
                );
                let _ = worker_app.emit(YOUTUBE_DOWNLOAD_EVENT, YoutubeDownloadEventPayload::Done);
            }
            Err(error) => {
                tracing::error!(%error, "youtube download failed");
                let _ = worker_app.emit(
                    YOUTUBE_DOWNLOAD_EVENT,
                    YoutubeDownloadEventPayload::Failed {
                        message: error.to_string(),
                    },
                );
            }
        }
    });
    Ok(())
}

/// Cancels the in-flight download, if any -- what the modal's Cancel button does.
#[tauri::command]
pub fn cancel_youtube_download(state: State<'_, AppState>) {
    if let Some(cancel) = state
        .youtube_download_cancel
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .as_ref()
    {
        cancel.store(true, Ordering::Relaxed);
    }
}

fn youtube_downloads_dir() -> PathBuf {
    avcore::app_paths::prefs_path()
        .parent()
        .map(|d| d.join("youtube_downloads"))
        .unwrap_or_else(|| PathBuf::from("youtube_downloads"))
}

#[cfg(test)]
#[path = "youtube_download_test.rs"]
mod tests;
