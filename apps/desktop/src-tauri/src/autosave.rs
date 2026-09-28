//! Recovery-file helpers shared with the egui autosave naming and timestamp semantics.

use std::path::{Path, PathBuf};

use tauri::Manager;

use crate::prefs::autosave_path;
use crate::state::AppState;

pub fn is_newer(project_path: &Path) -> Option<PathBuf> {
    let autosave = autosave_path(project_path);
    let autosave_time = std::fs::metadata(&autosave).ok()?.modified().ok()?;
    match std::fs::metadata(project_path).and_then(|metadata| metadata.modified()) {
        Ok(project_time) if autosave_time <= project_time => None,
        _ => Some(autosave),
    }
}

/// Runs outside Tauri's UI thread and writes the same recovery file as the egui shell.
pub fn spawn(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
        let state = app.state::<AppState>();
        let minutes = state.prefs().view.autosave_minutes.max(1);
        let mut session = state.session();
        if !session.dirty {
            continue;
        }
        let Some(path) = session.project.file_path.clone() else {
            continue;
        };
        // The worker's interval is intentionally simple: a dirty saved project gets a fresh
        // recovery snapshot every configured number of minutes until it is explicitly saved.
        // `modified` provides the rotation marker without adding a second persistence format.
        let recovery = autosave_path(&path);
        let due = std::fs::metadata(&recovery)
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|time| time.elapsed().ok())
            .is_none_or(|elapsed| {
                elapsed >= std::time::Duration::from_secs(u64::from(minutes) * 60)
            });
        if !due {
            continue;
        }
        let Ok(bytes) = avcore::to_ocproj_bytes(&session.project) else {
            continue;
        };
        if let Err(error) = std::fs::write(&recovery, bytes) {
            tracing::warn!(path = %recovery.display(), %error, "failed to write autosave");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_autosave_is_not_recoverable() {
        let project = std::env::temp_dir().join("oca_desktop_missing_autosave.ocproj");
        assert!(is_newer(&project).is_none());
    }
}
