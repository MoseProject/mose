use std::sync::atomic::Ordering;

use avcore::{ExportAspectRatio, MarkerKind};

use super::*;
use crate::preview_engine::Command;
use crate::testing::{harness, path_in};

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
    let events = h.count_session_events();
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
    let events = h.count_session_events();
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
    let events = h.count_session_events();
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
fn a_recent_list_that_cannot_be_written_is_logged_not_propagated() {
    assert!(note_recent_with(|| Ok(())));
    assert!(!note_recent_with(|| Err("disk full".to_string())));
}

#[test]
fn file_names_drop_what_a_file_system_rejects() {
    assert_eq!(file_stem_for("My show"), "My show");
    assert_eq!(file_stem_for(r"a/b\c:d*e?"), "a b c d e");
    assert_eq!(file_stem_for("  trailing dot. "), "trailing dot");
    assert_eq!(file_stem_for("???"), "project");
    assert_eq!(file_stem_for(""), "project");
}

#[test]
fn creating_a_project_writes_it_at_once_opens_it_and_lists_it() {
    let h = harness();
    let path = path_in(&h.dir, "Boss fight.ocproj");
    crate::dialogs::scripted::answer(Some(vec![path.clone()]));

    let created = tauri::async_runtime::block_on(create_project(
        h.handle(),
        "Boss fight".into(),
        "Cut 1".into(),
    ));

    assert_eq!(created, Ok(true));
    assert!(path.exists(), "the file exists before any save");
    let session = h.session();
    assert_eq!(session.project.name, "Boss fight");
    assert_eq!(session.project.file_path.as_deref(), Some(path.as_path()));
    assert!(!session.dirty);
    drop(session);
    let on_disk = avcore::load_project_from_file(&path).unwrap();
    assert_eq!(on_disk.name, "Boss fight");
    assert_eq!(on_disk.sequences[0].name, "Cut 1");
    assert!(h
        .prefs()
        .view
        .recent_project_paths
        .contains(&path.display().to_string()));
}

#[test]
fn a_cancelled_dialog_creates_nothing_and_keeps_the_open_project() {
    let h = harness();
    new_project(h.handle(), "Keep me".into(), "Seq".into()).unwrap();
    crate::dialogs::scripted::answer(None);

    let created =
        tauri::async_runtime::block_on(create_project(h.handle(), "Other".into(), "Seq".into()));

    assert_eq!(created, Ok(false));
    assert_eq!(h.session().project.name, "Keep me");
}

#[test]
fn a_blank_name_is_refused_before_any_dialog() {
    let h = harness();
    new_project(h.handle(), "Keep me".into(), "Seq".into()).unwrap();
    let result =
        tauri::async_runtime::block_on(create_project(h.handle(), "   ".into(), "Seq".into()));
    assert_eq!(result, Err("projectNameEmpty".to_string()));
    assert_eq!(h.session().project.name, "Keep me");
}

#[test]
fn a_project_that_cannot_be_written_leaves_the_open_one_alone() {
    let h = harness();
    new_project(h.handle(), "Keep me".into(), "Seq".into()).unwrap();
    let blocked = path_in(&h.dir, "no-such-dir").join("x.ocproj");
    crate::dialogs::scripted::answer(Some(vec![blocked]));

    let result =
        tauri::async_runtime::block_on(create_project(h.handle(), "Doomed".into(), "Seq".into()));

    assert!(result.is_err());
    assert_eq!(h.session().project.name, "Keep me");
}

#[test]
fn opening_a_missing_project_reports_the_error() {
    let h = harness();
    let missing = path_in(&h.dir, "missing.ocproj");
    let result = tauri::async_runtime::block_on(open_project_path(
        h.handle(),
        missing.display().to_string(),
    ));
    assert_eq!(result, Err("projectMissing".to_string()));
    assert_eq!(h.session().project.name, "Untitled");
}

#[test]
fn opening_a_project_that_is_gone_drops_it_from_the_recent_list() {
    let h = harness();
    let missing = path_in(&h.dir, "vanished.ocproj").display().to_string();
    h.prefs().view.recent_project_paths = vec![missing.clone(), "other.ocproj".to_string()];

    let result = tauri::async_runtime::block_on(open_project_path(h.handle(), missing.clone()));

    assert_eq!(result, Err("projectMissing".to_string()));
    assert_eq!(
        h.prefs().view.recent_project_paths,
        vec!["other.ocproj".to_string()]
    );
}

#[test]
fn a_file_that_is_not_a_project_reports_the_parse_error_and_stays_in_the_list() {
    let h = harness();
    let broken = path_in(&h.dir, "broken.ocproj");
    std::fs::write(&broken, b"not a project").unwrap();
    h.prefs().view.recent_project_paths = vec![broken.display().to_string()];

    let result =
        tauri::async_runtime::block_on(open_project_path(h.handle(), broken.display().to_string()));

    let message = result.unwrap_err();
    assert_ne!(message, "projectMissing");
    assert_eq!(h.prefs().view.recent_project_paths.len(), 1);
}

#[test]
fn opening_asks_for_a_project_and_a_cancelled_dialog_opens_nothing() {
    let h = harness();
    let path = path_in(&h.dir, "dialog.ocproj");
    new_project(h.handle(), "Chosen".into(), "Cut".into()).unwrap();
    h.session().project.file_path = Some(path.clone());
    tauri::async_runtime::block_on(save_project(h.handle(), false)).unwrap();
    new_project(h.handle(), "Other".into(), "Cut".into()).unwrap();

    crate::dialogs::scripted::answer(None);
    assert_eq!(
        tauri::async_runtime::block_on(open_project(h.handle())),
        Ok(false)
    );
    assert_eq!(h.session().project.name, "Other");

    crate::dialogs::scripted::answer(Some(vec![path]));
    assert_eq!(
        tauri::async_runtime::block_on(open_project(h.handle())),
        Ok(true)
    );
    assert_eq!(h.session().project.name, "Chosen");
}

#[test]
fn saving_an_unsaved_project_asks_where_and_saving_as_asks_again() {
    let h = harness();
    let first = path_in(&h.dir, "first.ocproj");
    let second = path_in(&h.dir, "second.ocproj");

    crate::dialogs::scripted::answer(None);
    assert_eq!(
        tauri::async_runtime::block_on(save_project(h.handle(), false)),
        Ok(false)
    );
    assert!(h.session().project.file_path.is_none());

    crate::dialogs::scripted::answer(Some(vec![first.clone()]));
    assert_eq!(
        tauri::async_runtime::block_on(save_project(h.handle(), false)),
        Ok(true)
    );
    assert_eq!(
        h.session().project.file_path.as_deref(),
        Some(first.as_path())
    );

    crate::dialogs::scripted::answer(Some(vec![second.clone()]));
    assert_eq!(
        tauri::async_runtime::block_on(save_project(h.handle(), true)),
        Ok(true)
    );
    assert_eq!(
        h.session().project.file_path.as_deref(),
        Some(second.as_path())
    );
    assert!(first.exists() && second.exists());
}

#[test]
fn undoing_or_redoing_with_no_history_changes_nothing() {
    let h = harness();
    undo(h.handle()).unwrap();
    redo(h.handle()).unwrap();
    assert!(!h.session().view().can_undo);
    assert!(!h.session().view().can_redo);
}
