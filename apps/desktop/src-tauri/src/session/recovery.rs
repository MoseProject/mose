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

//! Keeps unsaved work that has no project file yet. A saved project is written to its own file by
//! [`crate::autosave`]; a project that was never saved has nowhere to go, so while it has unsaved
//! changes a snapshot of it goes into this launch's [`avcore::recovery::RecoveryJournal`], and the
//! next launch offers it back if this one died first.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use avcore::recovery::{recovery_root, scan_candidates, RecoveryCandidate, RecoveryJournal};
use tauri::{AppHandle, Manager, Runtime};

use crate::state::{lock_or_recover, AppState};

/// How long the project must stay untouched before the snapshot is written.
pub const JOURNAL_DEBOUNCE: Duration = Duration::from_secs(1);
/// The longest a snapshot lags behind while edits keep coming. Writing one costs a clone and a
/// few milliseconds of encoding (outside the session lock), so a short bound is cheap, and it is
/// the most work a crash can take from a project that was never saved.
pub const JOURNAL_CEILING: Duration = Duration::from_secs(3);
/// How often the writer looks for changes.
const JOURNAL_TICK: Duration = Duration::from_millis(500);

/// This launch's journal and what it last wrote.
#[derive(Default)]
pub struct Recovery {
    journal: Option<RecoveryJournal>,
    root: Option<PathBuf>,
    /// The edit count the snapshot stands for.
    written_seq: Option<u64>,
    /// When the first change not yet in a snapshot was noticed.
    pending_since: Option<Instant>,
}

impl Recovery {
    /// What a crash does to the journal: it is released without the clean-exit cleanup, so its
    /// snapshot stays for the next launch. Tests only.
    #[cfg(test)]
    pub(crate) fn simulate_crash(&mut self) {
        self.journal = None;
    }

    /// Where the leftovers of earlier launches are looked for, and this launch's own directory.
    fn locations(&self) -> Option<(&Path, &Path)> {
        Some((self.root.as_deref()?, self.journal.as_ref()?.dir()))
    }
}

/// What one pass of the writer did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum JournalOutcome {
    /// Nothing to keep, or the snapshot is already current.
    Idle,
    /// Unsaved changes exist but are still coming in.
    Waiting,
    /// No journal could be started, so nothing is kept.
    Disabled,
    Written,
    Failed,
}

/// Starts this launch's journal under `data_dir`. A failure leaves the app working without one.
pub(crate) fn install<R: Runtime>(app: &AppHandle<R>, data_dir: &Path) {
    let root = recovery_root(data_dir);
    let state = app.state::<AppState>();
    let mut recovery = lock_or_recover(&state.recovery);
    match RecoveryJournal::start(&root) {
        Ok(journal) => {
            recovery.journal = Some(journal);
            recovery.root = Some(root);
        }
        Err(error) => tracing::warn!(%error, "could not start the recovery journal"),
    }
}

/// Keeps the snapshot in step with the open project, once its changes have settled for
/// `debounce` or have waited `ceiling`.
pub(crate) fn journal_pass<R: Runtime>(
    app: &AppHandle<R>,
    debounce: Duration,
    ceiling: Duration,
) -> JournalOutcome {
    let state = app.state::<AppState>();
    let (seq, quiet_for) = state.edits.snapshot();
    let project = {
        let session = state.session();
        let mut recovery = lock_or_recover(&state.recovery);
        if recovery.journal.is_none() {
            return JournalOutcome::Disabled;
        }
        if !session.dirty || session.project.file_path.is_some() {
            // Saved, or saved to a file of its own: the snapshot has nothing left to protect.
            if let Some(journal) = recovery.journal.as_ref().filter(|j| j.has_snapshot()) {
                journal.clear();
            }
            return JournalOutcome::Idle;
        }
        if recovery.written_seq == Some(seq) {
            return JournalOutcome::Idle;
        }
        let pending_since = *recovery.pending_since.get_or_insert_with(Instant::now);
        if quiet_for < debounce && pending_since.elapsed() < ceiling {
            return JournalOutcome::Waiting;
        }
        session.project.clone()
    };
    write_snapshot(&state, &project, Some(seq))
}

/// Writes `project` into the journal, outside the session lock.
fn write_snapshot(state: &AppState, project: &avcore::Project, seq: Option<u64>) -> JournalOutcome {
    let mut recovery = lock_or_recover(&state.recovery);
    let Some(journal) = recovery.journal.as_ref() else {
        return JournalOutcome::Disabled;
    };
    match journal.write(project) {
        Ok(()) => {
            recovery.written_seq = seq;
            recovery.pending_since = None;
            JournalOutcome::Written
        }
        Err(error) => {
            tracing::warn!(%error, "could not write the recovery snapshot");
            JournalOutcome::Failed
        }
    }
}

/// Drops the snapshot because the user chose to leave the unsaved work behind. A later edit
/// starts a new one.
pub(crate) fn discard_snapshot<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let recovery = lock_or_recover(&state.recovery);
    if let Some(journal) = &recovery.journal {
        journal.clear();
    }
}

/// Starts the background writer.
pub fn spawn<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(JOURNAL_TICK);
        journal_pass(&app, JOURNAL_DEBOUNCE, JOURNAL_CEILING);
    });
}

/// The unsaved work earlier launches left behind, newest first.
pub(crate) fn candidates<R: Runtime>(app: &AppHandle<R>) -> Vec<RecoveryCandidate> {
    let state = app.state::<AppState>();
    let recovery = lock_or_recover(&state.recovery);
    match recovery.locations() {
        Some((root, own)) => scan_candidates(root, Some(own)),
        None => Vec::new(),
    }
}

/// Puts the work from `id` into the open session as an unsaved project, snapshots it into this
/// launch's journal, and only then removes the old one, so a crash in between loses nothing.
pub(crate) fn recover<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    let candidate = candidates(app)
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| "that recovery is no longer available".to_string())?;
    let mut project = candidate.load().map_err(|error| error.to_string())?;
    project.file_path = None;
    let state = app.state::<AppState>();
    {
        let mut session = state.session();
        session.replace_project(project);
        session.dirty = true;
        crate::state::publish(app, &session);
        let snapshot = session.project.clone();
        drop(session);
        if write_snapshot(&state, &snapshot, None) == JournalOutcome::Failed {
            return Err(
                "the recovered work could not be kept safe; it was not removed".to_string(),
            );
        }
    }
    candidate.discard().map_err(|error| error.to_string())
}

/// Deletes the leftover work `id` for good.
pub(crate) fn discard<R: Runtime>(app: &AppHandle<R>, id: &str) -> Result<(), String> {
    let candidate = candidates(app)
        .into_iter()
        .find(|candidate| candidate.id == id)
        .ok_or_else(|| "that recovery is no longer available".to_string())?;
    candidate.discard().map_err(|error| error.to_string())
}

#[cfg(test)]
#[path = "recovery_test.rs"]
mod tests;
