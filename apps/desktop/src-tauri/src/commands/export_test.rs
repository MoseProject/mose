use avcore::{ExportJobStatus, GpuEncoderPreference};

use super::*;
use crate::testing::{harness, path_in, Harness};

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
    let id = start_export(h.handle(), output.display().to_string(), false, None).unwrap();
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
    start_export(h.handle(), output.display().to_string(), true, None).unwrap();
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
    assert!(start_export(h.handle(), output.display().to_string(), false, None).is_err());
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
        None,
    )
    .unwrap();
    let second = start_export(
        h.handle(),
        path_in(&h.dir, "b.mp4").display().to_string(),
        false,
        None,
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

    let result = start_export(h.handle(), source.display().to_string(), false, None);

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
    start_export(h.handle(), output.clone(), false, None).unwrap();

    assert_eq!(
        start_export(h.handle(), output.clone(), false, None),
        Err("outputInQueue".to_string())
    );

    wait_exports(&h);
    // Once the first job is done the file may be replaced by a new export.
    assert!(start_export(h.handle(), output, false, None).is_ok());
    wait_exports(&h);
}

#[test]
fn renaming_a_clash_with_a_media_file_exports_beside_it_instead() {
    let h = harness();
    h.add_fixture_clip();
    let source = own_copy_of_the_media(&h);
    start_export(h.handle(), source.display().to_string(), true, None).unwrap();
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

#[test]
fn the_target_joins_folder_and_name_and_tells_if_it_exists() {
    let h = harness();
    let folder = h.dir.display().to_string();
    std::fs::write(path_in(&h.dir, "taken.mp4"), b"x").unwrap();

    let taken = export_target(folder.clone(), "taken".to_string()).unwrap();
    assert!(taken.exists);
    assert_eq!(
        taken.path,
        path_in(&h.dir, "taken.mp4").display().to_string()
    );
    // The extension is not doubled, and characters a file name cannot hold become `_`.
    assert!(
        export_target(folder.clone(), "taken.MP4".to_string())
            .unwrap()
            .exists
    );
    let fresh = export_target(folder.clone(), " a/b:c? ".to_string()).unwrap();
    assert!(!fresh.exists);
    assert!(fresh.path.ends_with("a_b_c_.mp4"));
}

#[test]
fn a_target_without_folder_or_name_is_a_coded_error() {
    let h = harness();
    let folder = h.dir.display().to_string();
    assert_eq!(
        export_target(" ".into(), "x".into()).unwrap_err(),
        "noFolder"
    );
    assert_eq!(
        export_target(path_in(&h.dir, "missing").display().to_string(), "x".into()).unwrap_err(),
        "folderMissing"
    );
    assert_eq!(
        export_target(folder.clone(), " .mp4".into()).unwrap_err(),
        "noName"
    );
    assert_eq!(export_target(folder, "...".into()).unwrap_err(), "noName");
}

#[test]
fn the_export_canvas_follows_the_aspect_ratio() {
    let h = harness();
    h.add_fixture_clip();
    let original = export_canvas(h.state(), avcore::ExportAspectRatio::Original, None).unwrap();
    assert!(original.duration_secs > 0.0 && original.bitrate_mbps > 0.0);
    let portrait = export_canvas(h.state(), avcore::ExportAspectRatio::Portrait, None).unwrap();
    assert_eq!((portrait.width, portrait.height), (1080, 1920));
}

#[test]
fn a_custom_bitrate_and_height_reach_the_queued_job_and_forbid_the_copy() {
    let h = harness();
    h.add_fixture_clip();
    let output = path_in(&h.dir, "custom.mp4");
    let options = crate::export::ExportOptions {
        bitrate_mbps: Some(2.0),
        max_height: Some(144),
    };
    start_export(
        h.handle(),
        output.display().to_string(),
        false,
        Some(options),
    )
    .unwrap();
    wait_exports(&h);
    assert_eq!(
        h.state().exports.snapshot()[0].status,
        ExportJobStatus::Done
    );
    let probed = avcore::probe_media(&output).unwrap();
    assert_eq!(probed.resolution.map(|(_, height)| height), Some(144));
}

#[test]
fn an_unusable_bitrate_is_refused_before_anything_is_queued() {
    let h = harness();
    h.add_fixture_clip();
    for mbps in [0.0, -1.0, f32::NAN, 500.0] {
        let options = crate::export::ExportOptions {
            bitrate_mbps: Some(mbps),
            max_height: None,
        };
        let result = start_export(
            h.handle(),
            path_in(&h.dir, "bad.mp4").display().to_string(),
            false,
            Some(options),
        );
        assert_eq!(result, Err("invalidBitrate".to_string()));
    }
    assert!(list_exports(h.state()).is_empty());
}

#[test]
fn a_larger_height_exports_the_picture_scaled_up() {
    let h = harness();
    h.add_fixture_clip();
    let original = export_canvas(h.state(), avcore::ExportAspectRatio::Original, None).unwrap();
    let output = path_in(&h.dir, "up.mp4");
    let options = crate::export::ExportOptions {
        bitrate_mbps: None,
        max_height: Some(original.height + 144),
    };
    start_export(
        h.handle(),
        output.display().to_string(),
        false,
        Some(options),
    )
    .unwrap();
    wait_exports(&h);
    assert_eq!(
        h.state().exports.snapshot()[0].status,
        ExportJobStatus::Done
    );
    let probed = avcore::probe_media(&output).unwrap();
    assert_eq!(
        probed.resolution.map(|(_, height)| height),
        Some((original.height + 144) & !1)
    );
}
