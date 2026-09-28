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

//! Periodic project recovery and autosave restoration.

use std::path::Path;
use std::time::{Duration, Instant};

use eframe::egui;
use tracing::{debug, error, info};

use crate::components;
use crate::i18n::Text;

use super::App;

impl App {
    /// Writes the active project to a `<name>.autosave.ocproj` recovery file next to the
    /// project's own save file, subject to a 2-second idle debounce and a 30-second forced-save
    /// ceiling. Skips silently if the project has never been saved (no `file_path` yet) or
    /// hasn't changed. Serializes on the calling thread (fast, in-memory) then writes on a
    /// background thread so the UI never blocks on file I/O.
    pub(super) fn pump_autosave(&mut self) {
        if !self.project_dirty || self.open_projects.is_empty() {
            return;
        }
        let Some(file_path) = self.active_project().file_path.clone() else {
            return;
        };
        let now = Instant::now();
        let debounce_done = self
            .last_edit_instant
            .map(|t| now.duration_since(t) >= Duration::from_secs(2))
            .unwrap_or(false);
        let ceiling_hit = self
            .last_autosave_instant
            .map(|t| now.duration_since(t) >= Duration::from_secs(30))
            .unwrap_or(false);
        if !debounce_done && !ceiling_hit {
            return;
        }
        self.sync_panel_layout_into_active_project();
        let bytes = match avcore::persistence::to_ocproj_bytes(self.active_project()) {
            Ok(b) => b,
            Err(_) => return,
        };
        let autosave_path = avcore::app_paths::autosave_path(&file_path);
        debug!(path = %autosave_path.display(), "writing autosave");
        self.project_dirty = false;
        self.last_autosave_instant = Some(now);
        std::thread::spawn(move || {
            if let Err(e) = std::fs::write(&autosave_path, &bytes) {
                error!(path = %autosave_path.display(), error = %e, "autosave write failed");
            }
        });
    }

    /// If a newer autosave was found when the active project was opened ([`autosave_restore_pending`]
    /// is set), shows a modal offering to restore or discard it. Restore replaces the active
    /// project's data in place (keeping its `file_path` and id). Discard deletes the autosave file.
    pub(super) fn pump_autosave_restore(&mut self, ctx: &egui::Context) {
        let Some(autosave_path) = self.autosave_restore_pending.clone() else {
            return;
        };
        let locale = self.locale;
        let modal = egui::Modal::new(egui::Id::new("autosave_restore"));
        let response = modal.show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(Text::AutosaveFound.tr(locale));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if components::primary_button(ui, Text::AutosaveRestore.tr(locale)).clicked() {
                    if let Ok(mut restored) = avcore::load_project_from_file(&autosave_path) {
                        let file_path = self.active_project().file_path.clone();
                        let id = self.active_project().id;
                        restored.file_path = file_path;
                        restored.id = id;
                        info!(path = %autosave_path.display(), "autosave restored");
                        *self.active_project_mut() = restored;
                        self.project_dirty = false;
                        self.load_panel_layout_for_active_project();
                    }
                    self.autosave_restore_pending = None;
                }
                if ui.button(Text::AutosaveDiscard.tr(locale)).clicked() {
                    info!(path = %autosave_path.display(), "autosave discarded");
                    let _ = std::fs::remove_file(&autosave_path);
                    self.autosave_restore_pending = None;
                }
            });
        });
        if response.should_close() {
            self.autosave_restore_pending = None;
        }
    }

    /// Flags that a project with `file_path` should offer autosave restoration on open, if
    /// `<file_path>.autosave.ocproj` exists and is newer than the project file itself.
    pub fn check_autosave_on_open(&mut self, file_path: &Path) {
        let autosave_path = avcore::app_paths::autosave_path(&file_path);
        if autosave_is_newer(&autosave_path, file_path) {
            self.autosave_restore_pending = Some(autosave_path);
        }
    }
}

/// Returns `true` if `autosave_path` exists and has a modification time strictly newer than
/// `project_path`. Returns `false` if either file's metadata can't be read or the timestamps
/// can't be compared.
fn autosave_is_newer(autosave_path: &Path, project_path: &Path) -> bool {
    let Ok(as_meta) = std::fs::metadata(autosave_path) else {
        return false;
    };
    let Ok(proj_meta) = std::fs::metadata(project_path) else {
        return true;
    };
    let Ok(as_time) = as_meta.modified() else {
        return false;
    };
    let Ok(proj_time) = proj_meta.modified() else {
        return false;
    };
    as_time > proj_time
}

#[cfg(test)]
#[path = "autosave/autosave_test.rs"]
mod tests;
