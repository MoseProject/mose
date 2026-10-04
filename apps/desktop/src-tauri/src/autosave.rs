//! Recovery-file helpers shared with the egui autosave naming and timestamp semantics.

use std::path::{Path, PathBuf};

use std::time::Duration;

use tauri::{Emitter, Manager};

use crate::prefs::autosave_path;
use crate::state::{publish, AppState};

/// How long the project must stay untouched before it is saved: a burst of edits (dragging a
/// clip, typing a value) becomes one write.
pub const SAVE_DEBOUNCE: Duration = Duration::from_secs(2);
/// How often the saver looks for settled changes.
const SAVE_TICK: Duration = Duration::from_millis(500);
/// Emitted when saving the project in the background fails, once per run of failures.
pub const AUTOSAVE_FAILED_EVENT: &str = "autosave-failed";

/// What one pass of the saver did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveOutcome {
    /// Nothing changed since the last save.
    Idle,
    /// Changes exist but are still coming in.
    Waiting,
    /// The project has no file yet, so there is nowhere to save to.
    NoFile,
    Saved,
    Failed,
}

/// What the saver remembers between passes.
#[derive(Default)]
pub(crate) struct Saver {
    /// The edit count that was last written (or found to need no write).
    saved_seq: u64,
    /// The previous attempt failed, so the next failure is not announced again.
    failing: bool,
}

/// Writes the open project to its own file once its changes have settled for `debounce`, and
/// marks it as saved when nothing changed meanwhile. A project that has never been saved to a
/// file is left alone: its first save is the user's choice of where it lives.
pub(crate) fn save_when_settled<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    debounce: Duration,
    saver: &mut Saver,
) -> SaveOutcome {
    let state = app.state::<AppState>();
    let (seq, quiet_for) = state.edits.snapshot();
    if seq == saver.saved_seq {
        return SaveOutcome::Idle;
    }
    if quiet_for < debounce {
        return SaveOutcome::Waiting;
    }
    let (project, path) = {
        let session = state.session();
        if !session.dirty {
            saver.saved_seq = seq;
            return SaveOutcome::Idle;
        }
        let Some(path) = session.project.file_path.clone() else {
            saver.saved_seq = seq;
            return SaveOutcome::NoFile;
        };
        let mut project = session.project.clone();
        // The live playhead, as a manual save would record it.
        project.timeline_mut().playhead_secs = state.preview.playhead();
        (project, path)
    };
    // Written outside the session lock, so editing never waits for the disk.
    match avcore::save_project_to_file(&project, &path) {
        Ok(()) => {
            mark_saved(app, seq, &path);
            saver.saved_seq = seq;
            saver.failing = false;
            SaveOutcome::Saved
        }
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "failed to save the project");
            if !saver.failing {
                let _ = app.emit(AUTOSAVE_FAILED_EVENT, error.to_string());
            }
            saver.failing = true;
            SaveOutcome::Failed
        }
    }
}

/// Clears the unsaved mark after a write of edit `seq` to `path`, unless the project changed or
/// was replaced while the file was being written. Whether it was cleared.
fn mark_saved<R: tauri::Runtime>(app: &tauri::AppHandle<R>, seq: u64, path: &Path) -> bool {
    let state = app.state::<AppState>();
    let mut session = state.session();
    if state.edits.seq() != seq || session.project.file_path.as_deref() != Some(path) {
        return false;
    }
    session.dirty = false;
    publish(app, &session);
    true
}

/// Starts the background saver: every half second it saves the project if its changes settled.
pub fn spawn_saver<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    std::thread::spawn(move || {
        let mut saver = Saver::default();
        loop {
            std::thread::sleep(SAVE_TICK);
            save_when_settled(&app, SAVE_DEBOUNCE, &mut saver);
        }
    });
}

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

#[cfg(test)]
mod saver_tests {
    use std::sync::{Arc, Mutex};

    use avcore::ExportAspectRatio;
    use tauri::Listener;

    use super::*;
    use crate::testing::{harness, path_in, Harness};

    /// A harness whose project already lives in a file, like any project made from the start page.
    fn project_in_a_file(h: &Harness, name: &str) -> PathBuf {
        let path = path_in(&h.dir, name);
        crate::commands::new_project(h.handle(), "Saved".into(), "Seq".into()).unwrap();
        h.session().project.file_path = Some(path.clone());
        avcore::save_project_to_file(&h.session().project, &path).unwrap();
        h.session().dirty = false;
        path
    }

    fn edit(h: &Harness, lufs: f32) {
        crate::commands::set_export_settings(h.handle(), lufs, ExportAspectRatio::Original)
            .unwrap();
    }

    fn lufs_on_disk(path: &Path) -> f32 {
        avcore::load_project_from_file(path)
            .unwrap()
            .active_sequence()
            .export_settings
            .target_lufs
    }

    #[test]
    fn an_edit_is_written_to_the_project_file_and_marks_it_saved() {
        let h = harness();
        let path = project_in_a_file(&h, "auto.ocproj");
        edit(&h, -23.0);
        assert!(h.session().dirty);

        let mut saver = Saver::default();
        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::Saved
        );

        assert_eq!(lufs_on_disk(&path), -23.0);
        assert!(!h.session().dirty);
        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::Idle,
            "nothing new to save"
        );
    }

    #[test]
    fn a_burst_of_edits_waits_until_it_settles_and_is_written_once() {
        let h = harness();
        let path = project_in_a_file(&h, "burst.ocproj");
        let mut saver = Saver::default();

        edit(&h, -20.0);
        edit(&h, -21.0);
        edit(&h, -22.0);
        assert_eq!(
            save_when_settled(&h.handle(), Duration::from_secs(3600), &mut saver),
            SaveOutcome::Waiting
        );
        assert_ne!(
            lufs_on_disk(&path),
            -22.0,
            "nothing written while edits keep coming"
        );
        assert!(h.session().dirty);

        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::Saved
        );
        assert_eq!(lufs_on_disk(&path), -22.0, "the last state wins");
    }

    #[test]
    fn a_project_without_a_file_is_never_written() {
        let h = harness();
        crate::commands::new_project(h.handle(), "Loose".into(), "Seq".into()).unwrap();
        edit(&h, -20.0);

        let mut saver = Saver::default();
        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::NoFile
        );
        assert!(
            h.session().dirty,
            "still unsaved, the user has to pick a place"
        );
    }

    #[test]
    fn a_failed_write_keeps_the_project_dirty_and_is_announced_once() {
        let h = harness();
        project_in_a_file(&h, "x.ocproj");
        let blocked = path_in(&h.dir, "no-such-dir").join("x.ocproj");
        h.session().project.file_path = Some(blocked);
        let seen = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&seen);
        h.app.listen(AUTOSAVE_FAILED_EVENT, move |event| {
            sink.lock().unwrap().push(event.payload().to_string());
        });
        edit(&h, -20.0);

        let mut saver = Saver::default();
        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::Failed
        );
        assert_eq!(
            save_when_settled(&h.handle(), Duration::ZERO, &mut saver),
            SaveOutcome::Failed,
            "tried again on the next pass"
        );
        assert!(h.session().dirty);
        assert_eq!(
            seen.lock().unwrap().len(),
            1,
            "one notice for the whole run of failures"
        );
    }

    #[test]
    fn an_edit_made_while_writing_leaves_the_project_dirty() {
        let h = harness();
        let path = project_in_a_file(&h, "race.ocproj");
        edit(&h, -20.0);
        let seq_written = h.state().edits.seq();

        edit(&h, -25.0);

        assert!(!mark_saved(&h.handle(), seq_written, &path));
        assert!(h.session().dirty);
    }

    #[test]
    fn a_project_replaced_while_writing_is_not_marked_saved() {
        let h = harness();
        let path = project_in_a_file(&h, "old.ocproj");
        edit(&h, -20.0);
        let seq = h.state().edits.seq();
        crate::commands::new_project(h.handle(), "Another".into(), "Seq".into()).unwrap();
        h.session().dirty = true;

        assert!(!mark_saved(&h.handle(), seq, &path));
        assert!(h.session().dirty);
    }

    #[test]
    fn the_saved_file_records_the_live_playhead() {
        let h = harness();
        let path = project_in_a_file(&h, "playhead.ocproj");
        h.set_playhead(7.5);
        edit(&h, -20.0);

        let mut saver = Saver::default();
        save_when_settled(&h.handle(), Duration::ZERO, &mut saver);

        let loaded = avcore::load_project_from_file(&path).unwrap();
        assert_eq!(loaded.timeline().playhead_secs, 7.5);
    }

    #[test]
    fn the_edit_clock_counts_edits_and_measures_quiet_time() {
        let clock = crate::state::EditClock::default();
        assert_eq!(clock.seq(), 0);
        clock.touch();
        clock.touch();
        let (seq, quiet) = clock.snapshot();
        assert_eq!(seq, 2);
        assert!(quiet < Duration::from_secs(5));
    }
}
