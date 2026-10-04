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
