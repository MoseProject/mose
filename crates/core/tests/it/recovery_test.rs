// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use std::fs;

use avcore::recovery::{recovery_root, scan_candidates, RecoveryJournal};
use avcore::timeline::Timeline;
use avcore::{Project, Recency, Sequence, SequenceExportSettings};

fn project(name: &str) -> Project {
    Project {
        id: 1,
        name: name.to_owned(),
        last_edited: Recency::HoursAgo(0),
        summary: String::new(),
        media_library: Vec::new(),
        sequences: vec![Sequence {
            id: 1,
            name: "Main".to_owned(),
            timeline: Timeline {
                tracks: Vec::new(),
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: SequenceExportSettings::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}

#[test]
fn a_snapshot_round_trips_through_the_journal() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let journal = RecoveryJournal::start(&root).unwrap();
    journal.write(&project("Never saved")).unwrap();
    assert!(journal.has_snapshot());

    // A second launch sees the first one as running, so nothing is offered yet.
    assert!(scan_candidates(&root, None).is_empty());
    let own = journal.dir().to_path_buf();
    drop(journal);

    let candidates = scan_candidates(&root, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].project_name, "Never saved");
    assert_eq!(candidates[0].dir, own);
    assert_eq!(candidates[0].load().unwrap().name, "Never saved");
}

#[test]
fn a_running_session_is_never_offered_for_recovery() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let running = RecoveryJournal::start(&root).unwrap();
    running.write(&project("Still open")).unwrap();
    assert!(scan_candidates(&root, None).is_empty());
    assert!(running.has_snapshot());
}

#[test]
fn the_own_session_can_be_skipped_explicitly() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let first = RecoveryJournal::start(&root).unwrap();
    first.write(&project("Old")).unwrap();
    let stale = first.dir().to_path_buf();
    drop(first);
    let second = RecoveryJournal::start(&root).unwrap();
    let found = scan_candidates(&root, Some(second.dir()));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].dir, stale);
}

#[test]
fn clearing_the_snapshot_removes_the_session_on_exit() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let journal = RecoveryJournal::start(&root).unwrap();
    journal.write(&project("Saved later")).unwrap();
    journal.clear();
    assert!(!journal.has_snapshot());
    let dir = journal.dir().to_path_buf();
    drop(journal);
    assert!(!dir.exists());
    assert!(scan_candidates(&root, None).is_empty());
}

#[test]
fn a_session_that_exits_with_a_snapshot_keeps_it_for_the_next_launch() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let journal = RecoveryJournal::start(&root).unwrap();
    journal.write(&project("Unsaved at exit")).unwrap();
    let dir = journal.dir().to_path_buf();
    drop(journal);
    assert!(dir.join("snapshot.ocproj").is_file());
}

#[test]
fn a_newer_write_replaces_the_snapshot() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let journal = RecoveryJournal::start(&root).unwrap();
    journal.write(&project("First")).unwrap();
    journal.write(&project("Second")).unwrap();
    drop(journal);
    let candidates = scan_candidates(&root, None);
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].project_name, "Second");
}

#[test]
fn abandoned_directories_without_a_readable_snapshot_are_removed() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let empty = root.join("1-1");
    fs::create_dir_all(&empty).unwrap();
    let corrupt = root.join("2-2");
    fs::create_dir_all(&corrupt).unwrap();
    fs::write(corrupt.join("snapshot.ocproj"), b"not a project").unwrap();
    assert!(scan_candidates(&root, None).is_empty());
    assert!(!empty.exists());
    assert!(!corrupt.exists());
}

#[test]
fn discarding_a_candidate_deletes_its_directory() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let journal = RecoveryJournal::start(&root).unwrap();
    journal.write(&project("Throw away")).unwrap();
    drop(journal);
    let candidate = scan_candidates(&root, None).remove(0);
    candidate.discard().unwrap();
    assert!(!candidate.dir.exists());
}

#[test]
fn two_sessions_of_the_same_process_get_distinct_directories() {
    let data = tempfile::tempdir().unwrap();
    let root = recovery_root(data.path());
    let a = RecoveryJournal::start(&root).unwrap();
    let b = RecoveryJournal::start(&root).unwrap();
    assert_ne!(a.dir(), b.dir());
}
