use serde_json::json;

use crate::testing::harness;

#[test]
fn a_command_runs_through_the_real_handler_with_camel_case_arguments() {
    let h = harness();
    let view = h.ipc("get_session", json!({})).unwrap();
    assert_eq!(view["project"]["name"], "Untitled");

    h.ipc(
        "set_panel_layout",
        json!({ "libraryWidth": 300.0, "inspectorWidth": 320.0, "timelineHeight": 400.0 }),
    )
    .unwrap();
    assert_eq!(
        h.session()
            .project
            .panel_layout
            .as_ref()
            .unwrap()
            .lib_panel_width,
        300.0
    );
}

#[test]
fn a_failing_command_rejects_with_its_message() {
    let h = harness();
    let error = h
        .ipc(
            "set_panel_layout",
            json!({ "libraryWidth": 1.0, "inspectorWidth": 320.0, "timelineHeight": 400.0 }),
        )
        .unwrap_err();
    assert_eq!(error, json!("invalid panel layout"));
    assert!(h.ipc("no_such_command", json!({})).is_err());
}

#[test]
fn the_dialog_commands_answer_a_cancelled_dialog_through_the_handler() {
    let h = harness();
    assert_eq!(h.ipc("open_project", json!({})).unwrap(), json!(false));
    assert_eq!(
        h.ipc("save_project", json!({ "saveAs": false })).unwrap(),
        json!(false)
    );
    assert_eq!(h.ipc("pick_export_path", json!({})).unwrap(), json!(null));
    assert_eq!(
        h.ipc("export_active_sequence_srt", json!({})).unwrap(),
        json!(false)
    );
    h.ipc("import_media_dialog", json!({})).unwrap();
    assert!(h
        .ipc(
            "open_project_path",
            json!({ "path": h.dir.join("missing.ocproj") })
        )
        .is_err());
}

// The commands added for the frame rate, the crash-recovery journal and the control channel,
// called the way the webview calls them: through the real handler, with camelCase arguments.

#[test]
fn the_frame_rate_is_set_and_cleared_through_the_handler() {
    let h = harness();
    h.ipc(
        "set_project_frame_rate",
        json!({ "frameRate": { "num": 30000, "den": 1001 } }),
    )
    .unwrap();
    assert_eq!(
        h.session().project.frame_rate,
        Some(avcore::FrameRate::new(30_000, 1_001))
    );
    // It reaches the webview in the published session.
    let view = h.ipc("get_session", json!({})).unwrap();
    assert_eq!(
        view["project"]["frame_rate"],
        json!({ "num": 30000, "den": 1001 })
    );

    h.ipc("set_project_frame_rate", json!({ "frameRate": null }))
        .unwrap();
    assert_eq!(h.session().project.frame_rate, None);
    let view = h.ipc("get_session", json!({})).unwrap();
    assert_eq!(view["project"]["frame_rate"], json!(null));
}

#[test]
fn an_invalid_frame_rate_is_rejected_through_the_handler() {
    let h = harness();
    let error = h
        .ipc(
            "set_project_frame_rate",
            json!({ "frameRate": { "num": 0, "den": 1 } }),
        )
        .unwrap_err();
    assert_eq!(error, json!("invalid frame rate"));
    assert_eq!(h.session().project.frame_rate, None);
}

#[test]
fn leftover_work_is_listed_recovered_and_discarded_through_the_handler() {
    let h = harness();
    // Start this launch's journal, edit a never-saved project, then "crash" and relaunch.
    let data = h.dir.join("data");
    crate::recovery::install(&h.handle(), &data);
    h.session().project.name = "Gameplay cut".to_string();
    h.session().dirty = true;
    h.state().edits.touch();
    crate::recovery::journal_pass(
        &h.handle(),
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(3600),
    );
    crate::state::lock_or_recover(&h.state().recovery).simulate_crash();
    crate::recovery::install(&h.handle(), &data);

    let listed = h.ipc("list_recoveries", json!({})).unwrap();
    assert_eq!(listed.as_array().unwrap().len(), 1, "{listed}");
    assert_eq!(listed[0]["projectName"], "Gameplay cut");
    let id = listed[0]["id"].as_str().unwrap().to_owned();

    h.session().project.name = "Something else".to_string();
    h.ipc("recover_session", json!({ "id": id })).unwrap();
    assert_eq!(h.session().project.name, "Gameplay cut");
    assert!(h.session().project.file_path.is_none());
    assert!(h.session().dirty);
    assert_eq!(h.ipc("list_recoveries", json!({})).unwrap(), json!([]));
}

#[test]
fn discarding_leftover_work_deletes_it_and_unknown_ids_are_refused() {
    let h = harness();
    let data = h.dir.join("data");
    crate::recovery::install(&h.handle(), &data);
    h.session().dirty = true;
    h.state().edits.touch();
    crate::recovery::journal_pass(
        &h.handle(),
        std::time::Duration::ZERO,
        std::time::Duration::from_secs(3600),
    );
    crate::state::lock_or_recover(&h.state().recovery).simulate_crash();
    crate::recovery::install(&h.handle(), &data);

    let id = h.ipc("list_recoveries", json!({})).unwrap()[0]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    h.ipc("discard_recovery", json!({ "id": id })).unwrap();
    assert_eq!(h.ipc("list_recoveries", json!({})).unwrap(), json!([]));
    assert!(h.ipc("discard_recovery", json!({ "id": id })).is_err());
    assert!(h
        .ipc("recover_session", json!({ "id": "../../etc" }))
        .is_err());
}

#[test]
fn the_control_reply_command_takes_camel_case_arguments_and_ignores_unknown_requests() {
    let h = harness();
    h.ipc(
        "control_reply",
        json!({ "requestId": 12345, "ok": true, "value": null }),
    )
    .unwrap();
    h.ipc(
        "control_reply",
        json!({ "requestId": 12346, "ok": false, "value": "no such command" }),
    )
    .unwrap();
}
