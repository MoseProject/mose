// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! Closing the window must not throw away work without asking. When the project has unsaved
//! changes or an export is still queued or rendering, the close is held back and the webview is
//! asked to confirm; [`quit_app`] is what it calls once the user agrees.

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

pub const CLOSE_REQUESTED_EVENT: &str = "close-requested";

/// Why closing needs a confirmation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseRequest {
    /// The open project has changes that were never saved.
    pub dirty: bool,
    /// At least one export is waiting or running and would be lost.
    pub exporting: bool,
}

/// What would be lost if the app closed right now; `None` when nothing would.
pub fn pending_loss<R: Runtime>(app: &AppHandle<R>) -> Option<CloseRequest> {
    let state = app.state::<AppState>();
    let dirty = state.session().dirty;
    let exporting = state.exports.snapshot().iter().any(|job| {
        matches!(
            job.status,
            avcore::ExportJobStatus::Queued
                | avcore::ExportJobStatus::Rendering { .. }
                | avcore::ExportJobStatus::Paused { .. }
        )
    });
    (dirty || exporting).then_some(CloseRequest { dirty, exporting })
}

/// Called when the window is asked to close: when something would be lost, asks the webview to
/// confirm and returns `true` so the close is prevented.
pub fn guard_close<R: Runtime>(app: &AppHandle<R>) -> bool {
    // Whatever the debounce still holds goes to disk first; only what cannot be saved is asked
    // about (a project with no file yet, or a write that failed).
    crate::autosave::flush_now(app);
    let Some(request) = pending_loss(app) else {
        return false;
    };
    let _ = app.emit(CLOSE_REQUESTED_EVENT, request);
    true
}

/// Closes the app for good, after the user confirmed.
#[tauri::command]
pub fn quit_app<R: Runtime>(app: AppHandle<R>) {
    app.exit(0);
}

#[cfg(test)]
#[path = "close_guard_test.rs"]
mod tests;
