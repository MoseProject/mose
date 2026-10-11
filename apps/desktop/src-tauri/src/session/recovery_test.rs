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

use std::time::Duration;

use super::*;
use crate::testing::{harness, Harness};

const NOW: Duration = Duration::ZERO;
const NEVER: Duration = Duration::from_secs(3600);

fn started(h: &Harness) -> PathBuf {
    let data = h.dir.join("data");
    install(&h.handle(), &data);
    data
}

fn edit(h: &Harness, name: &str) {
    h.session().project.name = name.to_string();
    h.session().dirty = true;
    h.state().edits.touch();
}

/// What a crash leaves behind: the journal is released without the clean-exit cleanup.
fn crash(h: &Harness) {
    lock_or_recover(&h.state().recovery).journal = None;
}

#[test]
fn without_a_journal_nothing_is_kept() {
    let h = harness();
    edit(&h, "Lost");
    assert_eq!(
        journal_pass(&h.handle(), NOW, NOW),
        JournalOutcome::Disabled
    );
}

#[test]
fn a_never_saved_project_is_kept_once_its_changes_settle() {
    let h = harness();
    started(&h);
    edit(&h, "Gameplay cut");
    assert_eq!(
        journal_pass(&h.handle(), NEVER, NEVER),
        JournalOutcome::Waiting
    );
    assert_eq!(
        journal_pass(&h.handle(), NOW, NEVER),
        JournalOutcome::Written
    );
    // Nothing changed since: no second write.
    assert_eq!(journal_pass(&h.handle(), NOW, NEVER), JournalOutcome::Idle);
}

#[test]
fn changes_that_never_settle_are_still_written_after_the_ceiling() {
    let h = harness();
    started(&h);
    edit(&h, "Busy");
    assert_eq!(
        journal_pass(&h.handle(), NEVER, NEVER),
        JournalOutcome::Waiting
    );
    assert_eq!(
        journal_pass(&h.handle(), NEVER, NOW),
        JournalOutcome::Written
    );
}

#[test]
fn the_next_launch_is_offered_the_work_a_crash_left_behind() {
    let h = harness();
    let data = started(&h);
    edit(&h, "Gameplay cut");
    journal_pass(&h.handle(), NOW, NEVER);
    crash(&h);

    install(&h.handle(), &data);
    let found = candidates(&h.handle());
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].project_name, "Gameplay cut");
}

#[test]
fn a_running_launch_does_not_offer_its_own_snapshot() {
    let h = harness();
    started(&h);
    edit(&h, "Open now");
    journal_pass(&h.handle(), NOW, NEVER);
    assert!(candidates(&h.handle()).is_empty());
}

#[test]
fn saving_the_project_drops_the_snapshot() {
    let h = harness();
    let data = started(&h);
    edit(&h, "Soon saved");
    journal_pass(&h.handle(), NOW, NEVER);
    {
        let mut session = h.session();
        session.project.file_path = Some(h.dir.join("cut.ocproj"));
        session.dirty = false;
    }
    assert_eq!(journal_pass(&h.handle(), NOW, NEVER), JournalOutcome::Idle);
    crash(&h);
    install(&h.handle(), &data);
    assert!(candidates(&h.handle()).is_empty());
}

#[test]
fn a_project_with_a_file_is_left_to_the_autosave() {
    let h = harness();
    let data = started(&h);
    h.session().project.file_path = Some(h.dir.join("cut.ocproj"));
    edit(&h, "Has a file");
    assert_eq!(journal_pass(&h.handle(), NOW, NEVER), JournalOutcome::Idle);
    crash(&h);
    install(&h.handle(), &data);
    assert!(candidates(&h.handle()).is_empty());
}

#[test]
fn leaving_the_work_behind_on_purpose_drops_the_snapshot() {
    let h = harness();
    let data = started(&h);
    edit(&h, "Thrown away");
    journal_pass(&h.handle(), NOW, NEVER);
    discard_snapshot(&h.handle());
    crash(&h);
    install(&h.handle(), &data);
    assert!(candidates(&h.handle()).is_empty());
}

#[test]
fn recovering_restores_an_unsaved_project_and_keeps_it_safe() {
    let h = harness();
    let data = started(&h);
    edit(&h, "Gameplay cut");
    journal_pass(&h.handle(), NOW, NEVER);
    crash(&h);
    install(&h.handle(), &data);
    let id = candidates(&h.handle()).remove(0).id;

    h.session().project.name = "Something else".to_string();
    recover(&h.handle(), &id).unwrap();

    {
        let session = h.session();
        assert_eq!(session.project.name, "Gameplay cut");
        assert!(session.project.file_path.is_none());
        assert!(session.dirty);
    }
    // The old session is gone and this launch's own journal holds the work now.
    assert!(candidates(&h.handle()).is_empty());
    crash(&h);
    install(&h.handle(), &data);
    assert_eq!(candidates(&h.handle())[0].project_name, "Gameplay cut");
}

#[test]
fn discarding_removes_the_work_for_good() {
    let h = harness();
    let data = started(&h);
    edit(&h, "Unwanted");
    journal_pass(&h.handle(), NOW, NEVER);
    crash(&h);
    install(&h.handle(), &data);
    let id = candidates(&h.handle()).remove(0).id;
    discard(&h.handle(), &id).unwrap();
    assert!(candidates(&h.handle()).is_empty());
    assert!(discard(&h.handle(), &id).is_err());
}

#[test]
fn an_unknown_recovery_is_refused() {
    let h = harness();
    started(&h);
    assert!(recover(&h.handle(), "../escape").is_err());
}
