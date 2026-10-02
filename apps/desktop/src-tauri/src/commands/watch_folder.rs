//! Watch-folder auto-processing (CF-02) -- long-running background thread that polls a folder
//! for new gameplay recordings, waits for each to stabilize, then normalizes it via
//! `avcore::process_watched_file` -- mirrors `ui::App`'s `watch_folder.rs` exactly (same
//! `StabilityTracker` polling loop), pushing every state change through Tauri events instead of
//! a per-frame `pump_*` channel drain. No core changes needed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use avcore::LoudnessMetrics;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::state::AppState;

pub const WATCH_FOLDER_EVENT: &str = "watch-folder-event";
const POLL_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum WatchFolderEventPayload {
    Detected {
        path: String,
    },
    Stabilizing {
        path: String,
    },
    Processing {
        path: String,
    },
    Progress {
        path: String,
        percent: u8,
    },
    Done {
        path: String,
        before: LoudnessMetrics,
        after: LoudnessMetrics,
    },
    Failed {
        path: String,
        message: String,
    },
}

/// Shows a folder picker -- what the watch-folder screen's "Choose folder..." button does.
/// `None` if the dialog is cancelled.
#[tauri::command]
pub fn pick_watch_folder() -> Option<String> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|folder| folder.display().to_string())
}

/// Starts the watch loop against `path` on a background thread -- what the watch-folder
/// screen's "Start watching" button does. Stops any run already in flight against a different
/// folder first (only one watch runs at a time, same as the egui shell).
#[tauri::command]
pub fn start_watching_folder(app: AppHandle, path: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    stop_watch_thread(&state);
    let stop = Arc::new(AtomicBool::new(false));
    *state
        .watch_folder_stop
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = Some(Arc::clone(&stop));

    let worker_app = app.clone();
    let watch_path = PathBuf::from(path);
    std::thread::spawn(move || watch_loop(watch_path, stop, worker_app));
    Ok(())
}

/// Stops the background watch loop, if running -- what the screen's "Stop watching" button does.
#[tauri::command]
pub fn stop_watching_folder(state: State<'_, AppState>) {
    stop_watch_thread(&state);
}

fn stop_watch_thread(state: &AppState) {
    if let Some(stop) = state
        .watch_folder_stop
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
    {
        stop.store(true, Ordering::Relaxed);
    }
}

/// Imports a completed row's cleaned-up output (`avcore::output_path_for`'s deterministic
/// `processed/` path) into the active project's media library, through the existing
/// `import_media` pipeline -- what the screen's "Add to project" button does on a `Done` row.
#[tauri::command]
pub fn add_watched_file_to_project(app: AppHandle, watch_path: String, source_path: String) {
    let output = avcore::output_path_for(
        Path::new(&watch_path),
        avcore::DEFAULT_OUTPUT_SUBFOLDER,
        Path::new(&source_path),
    );
    crate::commands::media::import_media(app, vec![output.display().to_string()]);
}

fn watch_loop(watch_path: PathBuf, stop: Arc<AtomicBool>, app: AppHandle) {
    let stable_for = Duration::from_secs(avcore::DEFAULT_STABLE_SECS);
    let mut tracker = avcore::StabilityTracker::new();
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut errored: HashSet<PathBuf> = HashSet::new();

    while !stop.load(Ordering::Relaxed) {
        let Ok(entries) = std::fs::read_dir(&watch_path) else {
            std::thread::sleep(POLL_INTERVAL);
            continue;
        };

        for entry in entries.flatten() {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let path = entry.path();
            if !path.is_file() || !avcore::is_video_file(&path) {
                continue;
            }
            let output =
                avcore::output_path_for(&watch_path, avcore::DEFAULT_OUTPUT_SUBFOLDER, &path);
            if output.exists() {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if seen.insert(path.clone()) {
                emit(
                    &app,
                    WatchFolderEventPayload::Detected {
                        path: display(&path),
                    },
                );
            }

            let stable = tracker.poll(&path, metadata.len(), Instant::now(), stable_for);
            if !stable {
                errored.remove(&path);
                emit(
                    &app,
                    WatchFolderEventPayload::Stabilizing {
                        path: display(&path),
                    },
                );
                continue;
            }
            if errored.contains(&path) {
                continue;
            }

            emit(
                &app,
                WatchFolderEventPayload::Processing {
                    path: display(&path),
                },
            );
            let progress_app = app.clone();
            let progress_path = path.clone();
            let result = avcore::process_watched_file(
                &path,
                &output,
                avcore::DEFAULT_TARGET_LUFS,
                &stop,
                move |percent| {
                    emit(
                        &progress_app,
                        WatchFolderEventPayload::Progress {
                            path: display(&progress_path),
                            percent,
                        },
                    );
                },
            );

            match result {
                Ok(Some((before, after))) => {
                    tracker.forget(&path);
                    emit(
                        &app,
                        WatchFolderEventPayload::Done {
                            path: display(&path),
                            before,
                            after,
                        },
                    );
                }
                Ok(None) => {}
                Err(e) => {
                    errored.insert(path.clone());
                    emit(
                        &app,
                        WatchFolderEventPayload::Failed {
                            path: display(&path),
                            message: e.to_string(),
                        },
                    );
                }
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

fn display(path: &Path) -> String {
    path.display().to_string()
}

fn emit(app: &AppHandle, payload: WatchFolderEventPayload) {
    let _ = app.emit(WATCH_FOLDER_EVENT, payload);
}
