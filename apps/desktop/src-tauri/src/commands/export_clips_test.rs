use avcore::ExportJobStatus;

use super::*;
use crate::testing::{harness, path_in, Harness};

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

fn two_clips(h: &Harness) -> Vec<u64> {
    let (_, first) = h.add_fixture_clip();
    let (_, second) = h.add_fixture_clip();
    vec![first, second]
}

#[test]
fn each_selected_clip_gets_a_numbered_name_in_the_folder() {
    let h = harness();
    let ids = two_clips(&h);
    let folder = h.dir.display().to_string();
    let targets =
        export_clips_targets(h.state(), ids.clone(), folder.clone(), "Montage".into()).unwrap();
    assert_eq!(targets.len(), 2);
    assert_eq!(
        targets[0].path,
        path_in(&h.dir, "Montage - video - 01.mp4")
            .display()
            .to_string()
    );
    assert!(targets[1].path.ends_with("Montage - video - 02.mp4"));
    assert!(targets.iter().all(|target| !target.exists));
    // No prefix leaves that part out; an existing file is reported.
    std::fs::write(path_in(&h.dir, "video - 01.mp4"), b"x").unwrap();
    let plain = export_clips_targets(h.state(), ids, folder, "  ".into()).unwrap();
    assert!(plain[0].exists && !plain[1].exists);
}

#[test]
fn a_missing_folder_or_selection_is_a_coded_error() {
    let h = harness();
    let ids = two_clips(&h);
    let folder = h.dir.display().to_string();
    assert_eq!(
        export_clips_targets(h.state(), ids.clone(), " ".into(), "x".into()).unwrap_err(),
        "noFolder"
    );
    assert_eq!(
        export_clips_targets(
            h.state(),
            ids,
            path_in(&h.dir, "gone").display().to_string(),
            "x".into()
        )
        .unwrap_err(),
        "folderMissing"
    );
    assert_eq!(
        export_clips_targets(h.state(), vec![9999], folder, "x".into()).unwrap_err(),
        "noClips"
    );
}

#[test]
fn every_selected_clip_is_rendered_to_its_own_file() {
    let h = harness();
    let ids = two_clips(&h);
    let folder = h.dir.display().to_string();
    let jobs = start_export_clips(h.handle(), ids, folder, "Cut".into(), false, None).unwrap();
    assert_eq!(jobs.len(), 2);
    wait_exports(&h);
    let snapshot = h.state().exports.snapshot();
    assert!(snapshot
        .iter()
        .all(|job| job.status == ExportJobStatus::Done));
    for name in ["Cut - video - 01.mp4", "Cut - video - 02.mp4"] {
        let probed = avcore::probe_media(&path_in(&h.dir, name)).unwrap();
        assert!(probed.duration_secs > 0.0, "{name}");
    }
    // The project itself is left as it was: both clips are still on the timeline.
    let session = h.session();
    let clips: usize = session
        .project
        .timeline()
        .tracks
        .iter()
        .map(|t| t.clips.len())
        .sum();
    assert_eq!(clips, 2);
}

#[test]
fn an_existing_file_is_kept_when_asked_to_rename() {
    let h = harness();
    let ids = two_clips(&h);
    let taken = path_in(&h.dir, "Cut - video - 01.mp4");
    std::fs::write(&taken, b"keep me").unwrap();
    start_export_clips(
        h.handle(),
        ids,
        h.dir.display().to_string(),
        "Cut".into(),
        true,
        None,
    )
    .unwrap();
    wait_exports(&h);
    assert_eq!(std::fs::read(&taken).unwrap(), b"keep me");
    assert_eq!(h.state().exports.snapshot().len(), 2);
}

#[test]
fn nothing_is_queued_when_the_selection_has_no_video_clip() {
    let h = harness();
    two_clips(&h);
    let result = start_export_clips(
        h.handle(),
        vec![4242],
        h.dir.display().to_string(),
        "Cut".into(),
        false,
        None,
    );
    assert_eq!(result, Err("noClips".to_string()));
    assert!(h.state().exports.snapshot().is_empty());
}

#[test]
fn the_canvas_of_a_selection_sums_the_clip_lengths() {
    let h = harness();
    let ids = two_clips(&h);
    let one = crate::commands::export::export_canvas(
        h.state(),
        avcore::ExportAspectRatio::Original,
        Some(vec![ids[0]]),
    )
    .unwrap();
    let both = crate::commands::export::export_canvas(
        h.state(),
        avcore::ExportAspectRatio::Original,
        Some(ids),
    )
    .unwrap();
    assert!((both.duration_secs - 2.0 * one.duration_secs).abs() < 1e-6);
    assert_eq!((both.width, both.height), (one.width, one.height));
}
