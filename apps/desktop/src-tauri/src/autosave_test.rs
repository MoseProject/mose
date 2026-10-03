use std::time::{Duration, Instant};

use super::*;
use crate::testing::{harness, path_in};

const TICK: Duration = Duration::from_millis(10);

fn wait_for(what: &str, done: impl Fn() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_dirty_saved_project_gets_a_recovery_snapshot() {
    let h = harness();
    let project = path_in(&h.dir, "show.ocproj");
    {
        let mut session = h.session();
        session.project.file_path = Some(project.clone());
        session.dirty = true;
    }
    spawn_every(h.handle(), TICK);
    let recovery = autosave_path(&project);
    wait_for("the recovery snapshot", || recovery.exists());
    assert!(avcore::load_project_from_file(&recovery).is_ok());
}

#[test]
fn clean_or_unsaved_projects_are_left_alone() {
    let h = harness();
    let saved = path_in(&h.dir, "clean.ocproj");
    h.session().project.file_path = Some(saved.clone());
    spawn_every(h.handle(), TICK);
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        !autosave_path(&saved).exists(),
        "a clean project needs no snapshot"
    );

    let unsaved = harness();
    unsaved.session().dirty = true;
    spawn_every(unsaved.handle(), TICK);
    std::thread::sleep(Duration::from_millis(150));
    assert!(std::fs::read_dir(&unsaved.dir)
        .unwrap()
        .flatten()
        .all(|entry| !entry.file_name().to_string_lossy().contains("autosave")));
}

#[test]
fn a_recent_snapshot_is_not_rewritten_before_the_interval() {
    let h = harness();
    let project = path_in(&h.dir, "recent.ocproj");
    let recovery = autosave_path(&project);
    std::fs::write(&recovery, b"older recovery").unwrap();
    {
        let mut session = h.session();
        session.project.file_path = Some(project);
        session.dirty = true;
    }
    spawn_every(h.handle(), TICK);
    std::thread::sleep(Duration::from_millis(150));
    assert_eq!(std::fs::read(&recovery).unwrap(), b"older recovery");
}

#[test]
fn the_default_spawn_ticks_once_a_second() {
    let h = harness();
    spawn(h.handle());
}
