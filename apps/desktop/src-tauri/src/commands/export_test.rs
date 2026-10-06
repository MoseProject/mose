use avcore::{ExportJobStatus, GpuEncoderPreference};

use super::*;
use crate::testing::{harness, path_in, Harness};

/// Existing synchronous assertions await the command's asynchronous preparation boundary.
fn start_export(
    app: tauri::AppHandle<tauri::test::MockRuntime>,
    path: String,
    rename: bool,
) -> Result<u64, String> {
    tauri::async_runtime::block_on(super::start_export(app, path, rename))
}

/// Waits until every queued export has finished one way or the other.
fn wait_exports(h: &Harness) {
    let started = std::time::Instant::now();
    while h.state().exports.snapshot().iter().any(|job| {
        matches!(
            job.status,
            ExportJobStatus::Queued
                | ExportJobStatus::Rendering { .. }
                | ExportJobStatus::Paused { .. }
        )
    }) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(120),
            "export never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

#[test]
fn the_export_dialog_reports_the_chosen_path_and_whether_it_exists() {
    let h = harness();
    let existing = path_in(&h.dir, "taken.mp4");
    std::fs::write(&existing, b"x").unwrap();

    crate::dialogs::scripted::answer(Some(vec![existing.clone()]));
    let target = tauri::async_runtime::block_on(pick_export_path(h.handle()))
        .unwrap()
        .unwrap();
    assert!(target.exists);
    assert_eq!(target.path, existing.display().to_string());

    crate::dialogs::scripted::answer(Some(vec![path_in(&h.dir, "fresh.mp4")]));
    let target = tauri::async_runtime::block_on(pick_export_path(h.handle()))
        .unwrap()
        .unwrap();
    assert!(!target.exists);

    crate::dialogs::scripted::answer(None);
    assert!(tauri::async_runtime::block_on(pick_export_path(h.handle()))
        .unwrap()
        .is_none());
}

#[test]
fn an_export_renders_the_active_sequence_to_the_path() {
    let h = harness();
    h.add_fixture_clip();
    let output = path_in(&h.dir, "show.mp4");
    let id = start_export(h.handle(), output.display().to_string(), false).unwrap();
    wait_exports(&h);
    let jobs = list_exports(h.state());
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, id);
    assert_eq!(jobs[0].status, ExportJobStatus::Done);
    assert!(output.exists());
}

#[test]
fn exporting_onto_an_existing_file_can_keep_it_by_renaming_the_output() {
    let h = harness();
    h.add_fixture_clip();
    let output = path_in(&h.dir, "show.mp4");
    std::fs::write(&output, b"keep me").unwrap();
    start_export(h.handle(), output.display().to_string(), true).unwrap();
    wait_exports(&h);
    assert_eq!(std::fs::read(&output).unwrap(), b"keep me");
    let jobs = list_exports(h.state());
    assert_ne!(jobs[0].output_path, output.display().to_string());
    assert!(std::path::Path::new(&jobs[0].output_path).exists());
}

#[test]
fn exporting_an_empty_timeline_is_refused() {
    let h = harness();
    let output = path_in(&h.dir, "none.mp4");
    assert!(start_export(h.handle(), output.display().to_string(), false).is_err());
    assert!(list_exports(h.state()).is_empty());
}

#[test]
fn queue_controls_reach_the_queue() {
    let h = harness();
    h.add_fixture_clip();
    let first = start_export(
        h.handle(),
        path_in(&h.dir, "a.mp4").display().to_string(),
        false,
    )
    .unwrap();
    let second = start_export(
        h.handle(),
        path_in(&h.dir, "b.mp4").display().to_string(),
        false,
    )
    .unwrap();
    match_export_loudness(h.handle(), -23.0);
    pause_export(h.handle(), second);
    resume_export(h.handle(), second);
    cancel_export(h.handle(), second);
    pause_export(h.handle(), first);
    resume_export(h.handle(), first);
    wait_exports(&h);
    remove_export(h.handle(), first);
    assert!(list_exports(h.state()).iter().all(|job| job.id != first));
}

#[test]
fn encoder_and_worker_settings_are_stored() {
    let h = harness();
    assert_eq!(
        get_gpu_encoder_preference(h.state()),
        GpuEncoderPreference::Auto
    );
    set_gpu_encoder_preference(h.state(), GpuEncoderPreference::Cpu).unwrap();
    assert_eq!(
        get_gpu_encoder_preference(h.state()),
        GpuEncoderPreference::Cpu
    );
    set_export_worker_count(h.state(), 2).unwrap();
}

#[test]
fn subtitles_are_written_where_the_dialog_points_and_a_cancel_writes_nothing() {
    let h = harness();
    let target = path_in(&h.dir, "show.srt");
    crate::dialogs::scripted::answer(Some(vec![target.clone()]));
    assert_eq!(
        tauri::async_runtime::block_on(export_active_sequence_srt(h.handle())),
        Ok(true)
    );
    assert!(target.exists());

    crate::dialogs::scripted::answer(None);
    assert_eq!(
        tauri::async_runtime::block_on(export_active_sequence_srt(h.handle())),
        Ok(false)
    );

    crate::dialogs::scripted::answer(Some(vec![h.dir.join("missing").join("show.srt")]));
    assert!(tauri::async_runtime::block_on(export_active_sequence_srt(h.handle())).is_err());
}

#[test]
fn exporting_over_a_media_file_of_the_project_is_refused() {
    let h = harness();
    h.add_fixture_clip();
    let source = own_copy_of_the_media(&h);
    let before = std::fs::read(&source).unwrap();

    let result = start_export(h.handle(), source.display().to_string(), false);

    assert_eq!(result, Err("outputIsSource".to_string()));
    assert!(list_exports(h.state()).is_empty());
    assert_eq!(
        std::fs::read(&source).unwrap(),
        before,
        "the media is untouched"
    );
}

#[test]
fn exporting_to_the_same_file_as_a_pending_job_is_refused() {
    let h = harness();
    h.add_fixture_clip();
    let output = path_in(&h.dir, "twice.mp4").display().to_string();
    start_export(h.handle(), output.clone(), false).unwrap();

    assert_eq!(
        start_export(h.handle(), output.clone(), false),
        Err("outputInQueue".to_string())
    );

    wait_exports(&h);
    // Once the first job is done the file may be replaced by a new export.
    assert!(start_export(h.handle(), output, false).is_ok());
    wait_exports(&h);
}

#[test]
fn renaming_a_clash_with_a_media_file_exports_beside_it_instead() {
    let h = harness();
    h.add_fixture_clip();
    let source = own_copy_of_the_media(&h);
    start_export(h.handle(), source.display().to_string(), true).unwrap();
    wait_exports(&h);
    let jobs = list_exports(h.state());
    assert_ne!(jobs[0].output_path, source.display().to_string());
}

/// Points the library's only asset at a copy inside the scratch dir, so a test that would lose
/// its media if the guard failed destroys the copy and never the shipped fixture.
fn own_copy_of_the_media(h: &Harness) -> std::path::PathBuf {
    let copy = path_in(&h.dir, "copy.mp4");
    let original = h.session().project.media_library[0].source_path.clone();
    std::fs::copy(original, &copy).unwrap();
    h.session().project.media_library[0].source_path = copy.clone();
    copy
}
