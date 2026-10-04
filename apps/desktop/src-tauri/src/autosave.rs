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
/// Whether the recovery snapshot at `recovery` is missing or at least `minutes` old.
fn snapshot_due(recovery: &Path, minutes: u8) -> bool {
    std::fs::metadata(recovery)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.elapsed().ok())
        .is_none_or(|elapsed| elapsed >= std::time::Duration::from_secs(u64::from(minutes) * 60))
}

pub fn spawn<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    spawn_every(app, std::time::Duration::from_secs(1));
}

/// [`spawn`] checking every `tick` instead of every second, for tests.
pub(crate) fn spawn_every<R: tauri::Runtime>(app: tauri::AppHandle<R>, tick: std::time::Duration) {
    std::thread::spawn(move || loop {
        std::thread::sleep(tick);
        let state = app.state::<AppState>();
        let minutes = state.prefs().view.autosave_minutes.max(1);
        let session = state.session();
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
        if !snapshot_due(&recovery, minutes) {
            continue;
        }
        // Serialize and write outside the session lock so editing commands never wait on disk.
        let project = session.project.clone();
        drop(session);
        if let Ok(bytes) = avcore::to_ocproj_bytes(&project) {
            if let Err(error) = avcore::write_atomic(&recovery, &bytes) {
                tracing::warn!(path = %recovery.display(), %error, "failed to write autosave");
            }
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

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "oca_desktop_autosave_{name}_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn age(path: &Path, seconds: u64) {
        let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        let when = std::time::SystemTime::now() - std::time::Duration::from_secs(seconds);
        file.set_modified(when).unwrap();
    }

    #[test]
    fn a_snapshot_is_due_when_missing_or_older_than_the_interval() {
        let dir = scratch_dir("due");
        let snapshot = dir.join("recovery.ocproj");
        assert!(snapshot_due(&snapshot, 5), "no snapshot yet");

        std::fs::write(&snapshot, b"x").unwrap();
        assert!(!snapshot_due(&snapshot, 5), "just written");
        age(&snapshot, 4 * 60);
        assert!(!snapshot_due(&snapshot, 5), "four minutes is within five");
        age(&snapshot, 6 * 60);
        assert!(snapshot_due(&snapshot, 5), "six minutes is past five");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn an_autosave_is_recoverable_only_when_newer_than_the_saved_project() {
        let dir = scratch_dir("newer");
        let project = dir.join("game.ocproj");
        let autosave = autosave_path(&project);
        std::fs::write(&autosave, b"recovery").unwrap();

        // The project file is missing, so the autosave is all there is.
        assert_eq!(is_newer(&project), Some(autosave.clone()));

        std::fs::write(&project, b"saved").unwrap();
        age(&autosave, 600);
        assert_eq!(
            is_newer(&project),
            None,
            "the project was saved after the autosave"
        );

        age(&project, 1200);
        assert_eq!(is_newer(&project), Some(autosave));
        let _ = std::fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
#[path = "autosave_test.rs"]
mod spawn_tests;
