//! Update check -- mirrors `ui::App`'s `update_check.rs` exactly, entirely through existing
//! `avcore::update_check` functions (release-asset naming is already generic to the whole app,
//! not `crates/ui`-specific, so no packaging change is needed for this to work). Auto-install
//! itself (`avcore::apply_update`) only actually supports Linux AppImage/native-binary atomic
//! replacement today -- same platform limitation the egui shell already has, not something this
//! port adds or removes.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

pub const UPDATE_CHECK_EVENT: &str = "update-check-event";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum UpdateCheckEventPayload {
    NewerVersionAvailable {
        version: String,
        html_url: String,
        auto_update_available: bool,
    },
    UpToDate,
    Failed,
    Installed {
        version: String,
    },
    InstallFailed,
}

/// Checks GitHub Releases for a newer published version on a background thread -- what the
/// About screen's check does. Result arrives via [`UPDATE_CHECK_EVENT`].
#[tauri::command]
pub fn check_for_update<R: Runtime>(app: AppHandle<R>) {
    let services = app.state::<AppState>().services.clone();
    std::thread::spawn(move || {
        let release = match services.fetch_latest_release() {
            Ok(release) => release,
            Err(_) => {
                let _ = app.emit(UPDATE_CHECK_EVENT, UpdateCheckEventPayload::Failed);
                return;
            }
        };
        let event = if avcore::is_newer(env!("CARGO_PKG_VERSION"), &release.version) {
            UpdateCheckEventPayload::NewerVersionAvailable {
                version: release.version,
                html_url: release.html_url,
                auto_update_available: release.auto_update_available,
            }
        } else {
            UpdateCheckEventPayload::UpToDate
        };
        let _ = app.emit(UPDATE_CHECK_EVENT, event);
    });
}

/// Downloads and atomically installs `version` on a background thread -- only actually supported
/// on Linux (AppImage/native binary) today; elsewhere `avcore::apply_update` returns
/// `UnsupportedPlatform` and this reports [`UpdateCheckEventPayload::InstallFailed`]. Does not
/// restart the process itself.
#[tauri::command]
pub fn install_available_update<R: Runtime>(app: AppHandle<R>, version: String) {
    let services = app.state::<AppState>().services.clone();
    std::thread::spawn(move || match services.apply_update(&version) {
        Ok(avcore::ApplyUpdateOutcome::Updated { version }) => {
            tracing::info!(%version, "application update installed");
            let _ = app.emit(
                UPDATE_CHECK_EVENT,
                UpdateCheckEventPayload::Installed { version },
            );
        }
        Ok(avcore::ApplyUpdateOutcome::UpToDate) => {
            let _ = app.emit(UPDATE_CHECK_EVENT, UpdateCheckEventPayload::UpToDate);
        }
        Err(error) => {
            tracing::error!(%error, "application update failed");
            let _ = app.emit(UPDATE_CHECK_EVENT, UpdateCheckEventPayload::InstallFailed);
        }
    });
}

/// Starts the newly installed executable, preserving command-line arguments, then exits this
/// process -- what the About screen's "Restart" button does once an update is installed.
#[tauri::command]
pub fn restart_after_update<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    app.state::<AppState>()
        .services
        .restart_application()
        .map_err(|e| e.to_string())?;
    app.exit(0);
    Ok(())
}
