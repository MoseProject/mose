// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Crash-recovery journal for work that has no project file to save into yet.
//!
//! A saved project is written to its own file by the autosave, so a crash costs at most a couple
//! of seconds. A project that was never saved has nowhere to go, so while it has unsaved changes
//! a [`RecoveryJournal`] keeps a snapshot of it under `<root>/<started>-<pid>/`.
//!
//! Each running app holds an OS lock on `<session>/lock`. When the process dies (crash,
//! `kill -9`, power cut) the OS releases the lock, so at the next launch any session directory
//! whose lock can be taken and that holds a snapshot is a [`RecoveryCandidate`]. Directories of
//! sessions that are still running are never touched.

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::persistence::{from_project_bytes, to_project_bytes, write_atomic, PersistError};
use crate::project::Project;

const LOCK_FILE: &str = "lock";
const SNAPSHOT_FILE: &str = "snapshot.ocproj";
/// How many directory names to try before giving up on creating a session.
const MAX_SESSION_ATTEMPTS: u32 = 100;

/// Where the recovery sessions live inside the app's data directory.
pub fn recovery_root(data_dir: &Path) -> PathBuf {
    data_dir.join("recovery")
}

/// One running app's recovery directory, locked for as long as this value lives.
pub struct RecoveryJournal {
    dir: PathBuf,
    lock: Option<File>,
}

impl RecoveryJournal {
    /// Creates this launch's session directory under `root` and locks it.
    pub fn start(root: &Path) -> io::Result<Self> {
        fs::create_dir_all(root)?;
        let started = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_secs());
        let pid = std::process::id();
        let mut last_error = None;
        for attempt in 0..MAX_SESSION_ATTEMPTS {
            let name = if attempt == 0 {
                format!("{started}-{pid}")
            } else {
                format!("{started}-{pid}-{attempt}")
            };
            let dir = root.join(name);
            match fs::create_dir(&dir) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            let lock = OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(dir.join(LOCK_FILE))?;
            match lock.try_lock() {
                Ok(()) => {
                    return Ok(Self {
                        dir,
                        lock: Some(lock),
                    })
                }
                // Another instance's startup scan looked at the fresh directory first.
                Err(TryLockError::WouldBlock) => {
                    last_error = Some(io::Error::from(io::ErrorKind::WouldBlock));
                }
                Err(TryLockError::Error(error)) => return Err(error),
            }
        }
        Err(last_error.unwrap_or_else(|| io::Error::from(io::ErrorKind::AlreadyExists)))
    }

    /// The session directory this journal writes into.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Replaces the snapshot with `project`. Atomic: a crash while writing keeps the previous one.
    pub fn write(&self, project: &Project) -> Result<(), PersistError> {
        let bytes = to_project_bytes(project)?;
        write_atomic(&self.dir.join(SNAPSHOT_FILE), &bytes).map_err(PersistError::Io)
    }

    /// Drops the snapshot: the work was saved or deliberately discarded.
    pub fn clear(&self) {
        let _ = fs::remove_file(self.dir.join(SNAPSHOT_FILE));
    }

    /// Whether a snapshot is on disk right now.
    pub fn has_snapshot(&self) -> bool {
        self.dir.join(SNAPSHOT_FILE).is_file()
    }
}

impl Drop for RecoveryJournal {
    /// A normal exit with nothing unsaved removes the session directory. One that still holds a
    /// snapshot (the OS ending the session, a quit that did not ask) keeps it, and the next
    /// launch offers it.
    fn drop(&mut self) {
        drop(self.lock.take());
        if !self.dir.join(SNAPSHOT_FILE).exists() {
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
}

/// Unsaved work left behind by a session that is no longer running.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryCandidate {
    /// The session directory's name; what the UI passes back to pick one.
    pub id: String,
    pub dir: PathBuf,
    pub project_name: String,
    /// When the snapshot was last written, in seconds since the Unix epoch.
    pub saved_unix_secs: u64,
}

impl RecoveryCandidate {
    /// Loads the snapshot as a project.
    pub fn load(&self) -> Result<Project, PersistError> {
        let bytes = fs::read(self.dir.join(SNAPSHOT_FILE)).map_err(PersistError::Io)?;
        from_project_bytes(&bytes)
    }

    /// Deletes the whole session directory.
    pub fn discard(&self) -> io::Result<()> {
        fs::remove_dir_all(&self.dir)
    }
}

/// Whether the session's owner is gone, meaning its lock can be taken.
fn lock_is_free(dir: &Path) -> bool {
    match OpenOptions::new().write(true).open(dir.join(LOCK_FILE)) {
        Ok(file) => file.try_lock().is_ok(),
        // A directory without a lock file was never fully created or was half removed.
        Err(error) => error.kind() == io::ErrorKind::NotFound,
    }
}

/// Lists the recoverable sessions under `root`, newest first. Leftover directories without a
/// readable snapshot are removed. `skip` is this launch's own journal directory.
pub fn scan_candidates(root: &Path, skip: Option<&Path>) -> Vec<RecoveryCandidate> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let dir = entry.path();
        if !dir.is_dir() || Some(dir.as_path()) == skip || !lock_is_free(&dir) {
            continue;
        }
        let snapshot = dir.join(SNAPSHOT_FILE);
        let candidate = fs::read(&snapshot)
            .ok()
            .and_then(|bytes| from_project_bytes(&bytes).ok())
            .and_then(|project| {
                let saved = fs::metadata(&snapshot).ok()?.modified().ok()?;
                Some(RecoveryCandidate {
                    id: entry.file_name().to_string_lossy().into_owned(),
                    dir: dir.clone(),
                    project_name: project.name,
                    saved_unix_secs: saved.duration_since(UNIX_EPOCH).ok()?.as_secs(),
                })
            });
        match candidate {
            Some(candidate) => found.push(candidate),
            None => {
                let _ = fs::remove_dir_all(&dir);
            }
        }
    }
    found.sort_by_key(|candidate| std::cmp::Reverse(candidate.saved_unix_secs));
    found
}
