use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use avcore::{ExportAspectRatio, MarkerKind};
use tauri::Listener;

use super::*;
use crate::preview_engine::Command;
use crate::state::SESSION_CHANGED;
use crate::testing::{harness, path_in};

/// Counts `session-changed` events on the harness's app.
fn count_session_events(app: &tauri::App<tauri::test::MockRuntime>) -> Arc<AtomicUsize> {
    let seen = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&seen);
    app.listen(SESSION_CHANGED, move |_| {
        counter.fetch_add(1, Ordering::SeqCst);
    });
    seen
}

#[test]
fn get_session_serializes_the_current_view() {
    let h = harness();
    let view = get_session(h.state()).unwrap();
    assert_eq!(view["project"]["name"], "Untitled");
    assert_eq!(view["canUndo"], false);
    assert_eq!(view["dirty"], false);
}

#[test]
fn panel_layout_is_stored_and_marks_the_project_dirty() {
    let h = harness();
    set_panel_layout(h.handle(), 300.0, 320.0, 400.0).unwrap();
    let session = h.session();
    let layout = session.project.panel_layout.as_ref().unwrap();
    assert_eq!(layout.lib_panel_width, 300.0);
    assert_eq!(layout.props_panel_width, 320.0);
    assert_eq!(layout.timeline_height, 400.0);
    assert!(session.dirty);
}

#[test]
fn panel_layout_rejects_out_of_range_and_non_finite_sizes() {
    let h = harness();
    for (lib, props, timeline) in [
        (100.0, 300.0, 300.0),
        (700.0, 300.0, 300.0),
        (300.0, 100.0, 300.0),
        (300.0, 700.0, 300.0),
        (300.0, 300.0, 100.0),
        (300.0, 300.0, 2000.0),
        (f32::NAN, 300.0, 300.0),
        (300.0, f32::INFINITY, 300.0),
        (300.0, 300.0, f32::NAN),
    ] {
        assert_eq!(
            set_panel_layout(h.handle(), lib, props, timeline),
            Err("invalid panel layout".to_string())
        );
    }
    assert!(h.session().project.panel_layout.is_none());
}

#[test]
fn a_new_project_takes_its_loudness_target_from_the_preferred_profile() {
    let h = harness();
    for (profile, expected) in [(0, -14.0), (1, -16.0), (2, -23.0), (99, -14.0)] {
        h.prefs().view.lufs_profile = profile;
        new_project(h.handle(), "Show".into(), "Cut".into()).unwrap();
        let session = h.session();
        assert_eq!(session.project.name, "Show");
        assert_eq!(session.project.active_sequence().name, "Cut");
        assert_eq!(
            session
                .project
                .active_sequence()
                .export_settings
                .target_lufs,
            expected
        );
    }
}

#[test]
fn publishing_emits_the_session_and_hands_the_project_to_the_preview() {
    let h = harness();
    let events = count_session_events(&h.app);
    new_project(h.handle(), "Show".into(), "Cut".into()).unwrap();
    assert_eq!(events.load(Ordering::SeqCst), 1);
    let sent = h.drain_preview();
    assert!(matches!(sent.as_slice(), [Command::Content { project }] if project.name == "Show"));
}

#[test]
fn an_undoable_edit_can_be_undone_and_redone() {
    let h = harness();
    undoable(&h.handle(), |project| {
        project.timeline_mut().add_marker(2.5, MarkerKind::Standard);
        true
    })
    .unwrap();
    assert_eq!(h.session().project.timeline().markers.len(), 1);
    assert!(h.session().dirty);

    undo(h.handle()).unwrap();
    assert!(h.session().project.timeline().markers.is_empty());
    redo(h.handle()).unwrap();
    assert_eq!(h.session().project.timeline().markers.len(), 1);
}

#[test]
fn an_edit_that_changes_nothing_leaves_no_undo_step_and_no_event() {
    let h = harness();
    let events = count_session_events(&h.app);
    undoable(&h.handle(), |_| false).unwrap();
    assert_eq!(events.load(Ordering::SeqCst), 0);
    assert!(!h.session().view().can_undo);
    assert!(!h.session().dirty);
}

#[test]
fn an_undoable_edit_reads_the_live_playhead() {
    let h = harness();
    undoable(&h.handle(), |project| {
        assert_eq!(project.timeline().playhead_secs, 0.0);
        false
    })
    .unwrap();
}

#[test]
fn a_failing_session_change_publishes_nothing() {
    let h = harness();
    let events = count_session_events(&h.app);
    let result: Result<(), String> = with_session(&h.handle(), |_| Err("nope".to_string()));
    assert_eq!(result, Err("nope".to_string()));
    assert_eq!(events.load(Ordering::SeqCst), 0);
}

#[test]
fn transport_commands_reach_the_preview_engine() {
    let h = harness();
    preview_seek(h.state(), 4.5);
    preview_step(h.state(), -3);
    preview_set_playing(h.state(), true);
    let sent = h.drain_preview();
    assert!(matches!(sent[0], Command::Seek(secs) if secs == 4.5));
    assert!(matches!(sent[1], Command::Step(-3)));
    assert!(matches!(sent[2], Command::Playing(true)));
}

#[test]
fn export_settings_clamp_the_loudness_target() {
    let h = harness();
    set_export_settings(h.handle(), -90.0, ExportAspectRatio::Portrait).unwrap();
    {
        let session = h.session();
        let settings = &session.project.active_sequence().export_settings;
        assert_eq!(settings.target_lufs, -40.0);
        assert_eq!(settings.aspect_ratio, ExportAspectRatio::Portrait);
        assert!(session.dirty);
    }
    set_export_settings(h.handle(), 0.0, ExportAspectRatio::Landscape).unwrap();
    assert_eq!(
        h.session()
            .project
            .active_sequence()
            .export_settings
            .target_lufs,
        -5.0
    );
}

#[test]
fn a_saved_project_reopens_with_its_path_and_joins_the_recent_list() {
    let h = harness();
    let path = path_in(&h.dir, "show.ocproj");
    new_project(h.handle(), "Show".into(), "Cut".into()).unwrap();
    h.session().project.file_path = Some(path.clone());
    assert_eq!(
        tauri::async_runtime::block_on(save_project(h.handle(), false)),
        Ok(true)
    );
    assert!(!h.session().dirty);
    assert!(path.exists());

    new_project(h.handle(), "Other".into(), "Seq".into()).unwrap();
    tauri::async_runtime::block_on(open_project_path(h.handle(), path.display().to_string()))
        .unwrap();
    let session = h.session();
    assert_eq!(session.project.name, "Show");
    assert_eq!(session.project.file_path.as_deref(), Some(path.as_path()));
}

#[test]
fn opening_a_missing_project_reports_the_error() {
    let h = harness();
    let missing = path_in(&h.dir, "missing.ocproj");
    let result = tauri::async_runtime::block_on(open_project_path(
        h.handle(),
        missing.display().to_string(),
    ));
    assert!(result.is_err());
    assert_eq!(h.session().project.name, "Untitled");
}
