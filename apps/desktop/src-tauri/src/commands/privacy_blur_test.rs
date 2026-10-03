use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, path_in, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(PRIVACY_BLUR_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

fn first_event(events: &Arc<Mutex<Vec<String>>>) -> String {
    let started = Instant::now();
    loop {
        if let Some(event) = events.lock().unwrap().first().cloned() {
            return event;
        }
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "no event arrived"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn running(h: &Harness) -> bool {
    h.state()
        .privacy_blur_generating_clip_id
        .lock()
        .unwrap()
        .is_some()
}

/// A harness with a saved project (so the matte lands in the scratch dir) and one fixture clip.
fn ready() -> (Harness, u64, u64) {
    let h = harness();
    h.session().project.file_path = Some(path_in(&h.dir, "project.ocproj"));
    let (asset, clip) = h.add_fixture_clip();
    (h, asset, clip)
}

#[test]
fn a_blurred_region_is_tracked_into_a_matte_and_enabled_on_the_clip() {
    let (h, _, clip) = ready();
    let events = collect_events(&h);
    apply_privacy_blur_to_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2).unwrap();
    assert!(first_event(&events).contains("\"type\":\"done\""));
    assert!(!running(&h));
    let session = h.session();
    let c = &session.project.timeline().tracks[0].clips[0];
    assert!(c.privacy_blur_enabled);
    assert_eq!(c.privacy_blur_seed_vertices.len(), 4);
    assert_eq!(
        (
            c.privacy_blur_seed_center_x_frac,
            c.privacy_blur_seed_center_y_frac
        ),
        (0.5, 0.5)
    );
    assert!(std::path::Path::new(&c.privacy_blur_mask_path).exists());
    assert!(session.dirty);
}

#[test]
fn a_busy_worker_unknown_clips_and_missing_assets_are_refused() {
    let (h, asset, clip) = ready();
    *h.state().privacy_blur_generating_clip_id.lock().unwrap() = Some(1);
    assert_eq!(
        apply_privacy_blur_to_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2),
        Err("a privacy blur is already running".to_string())
    );
    *h.state().privacy_blur_generating_clip_id.lock().unwrap() = None;

    assert_eq!(
        apply_privacy_blur_to_clip(h.handle(), 999, 0.5, 0.5, 0.2, 0.2),
        Err("clip not found".to_string())
    );
    assert!(!running(&h));
    h.session().project.media_library.retain(|a| a.id != asset);
    assert_eq!(
        apply_privacy_blur_to_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2),
        Err("asset not found".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn a_source_that_cannot_be_decoded_reports_a_failure() {
    let (h, _, clip) = ready();
    h.session().project.media_library[0].source_path = h.dir.join("missing.mp4");
    let events = collect_events(&h);
    apply_privacy_blur_to_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2).unwrap();
    let event = first_event(&events);
    assert!(event.contains("\"type\":\"failed\"") && event.contains("failed to open source"));
    assert!(!running(&h));
    assert!(!h.session().project.timeline().tracks[0].clips[0].privacy_blur_enabled);
}

#[test]
fn a_clip_with_no_footage_cannot_be_blurred() {
    let h = harness();
    let dir = h.dir.join("mattes");
    let source = crate::testing::fixture("video.mp4");
    assert_eq!(
        apply_privacy_blur_one(
            &source,
            1.0,
            1.0,
            0.5,
            0.5,
            0.2,
            0.2,
            &dir,
            &dir.join("m.mp4")
        ),
        Err("clip has zero duration".to_string())
    );
}

#[test]
fn a_matte_that_cannot_be_written_reports_the_error() {
    let h = harness();
    let blocker = h.dir.join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let source = crate::testing::fixture("video.mp4");
    let result = apply_privacy_blur_one(
        &source,
        0.0,
        2.0,
        0.5,
        0.5,
        0.2,
        0.2,
        &blocker.join("inside"),
        &blocker.join("inside").join("m.mp4"),
    );
    assert!(result
        .unwrap_err()
        .contains("failed to create matte cache dir"));
}
