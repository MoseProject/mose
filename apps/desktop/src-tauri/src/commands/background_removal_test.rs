use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, path_in, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(MATTE_GENERATION_EVENT, move |event| {
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
    h.state().matte_generating_clip_id.lock().unwrap().is_some()
}

/// A saved project with one fixture clip and a configured (fake) model.
fn ready() -> (Harness, u64, u64) {
    let h = harness();
    h.session().project.file_path = Some(path_in(&h.dir, "project.ocproj"));
    let (asset, clip) = h.add_fixture_clip();
    h.prefs().view.background_removal_model_path = "model.onnx".into();
    *h.services.person_alpha.lock().unwrap() = Some(0.6);
    (h, asset, clip)
}

#[test]
fn a_matte_is_generated_for_the_clip_and_switched_on() {
    let (h, _, clip) = ready();
    let events = collect_events(&h);
    generate_matte_for_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"type\":\"done\""));
    assert!(!running(&h));
    let session = h.session();
    let c = &session.project.timeline().tracks[0].clips[0];
    assert!(c.background_removal_enabled);
    assert!(std::path::Path::new(&c.background_removal_mask_path).exists());
    assert!(session.dirty);
}

#[test]
fn a_segmentation_failure_is_reported_and_leaves_the_clip_alone() {
    let (h, _, clip) = ready();
    *h.services.person_alpha.lock().unwrap() = None;
    let events = collect_events(&h);
    generate_matte_for_clip(h.handle(), clip).unwrap();
    let event = first_event(&events);
    assert!(event.contains("\"type\":\"failed\"") && event.contains("segmentation failed"));
    assert!(!running(&h));
    assert!(!h.session().project.timeline().tracks[0].clips[0].background_removal_enabled);
}

#[test]
fn starting_is_refused_without_a_model_a_clip_an_asset_or_when_busy() {
    let (h, asset, clip) = ready();
    *h.state().matte_generating_clip_id.lock().unwrap() = Some(1);
    assert_eq!(
        generate_matte_for_clip(h.handle(), clip),
        Err("a matte generation is already running".to_string())
    );
    *h.state().matte_generating_clip_id.lock().unwrap() = None;

    h.prefs().view.background_removal_model_path = "  ".into();
    assert_eq!(
        generate_matte_for_clip(h.handle(), clip),
        Err("no background-removal model configured".to_string())
    );
    assert!(!running(&h));
    h.prefs().view.background_removal_model_path = "model.onnx".into();

    assert_eq!(
        generate_matte_for_clip(h.handle(), 999),
        Err("clip not found".to_string())
    );
    assert!(!running(&h));
    h.session().project.media_library.retain(|a| a.id != asset);
    assert_eq!(
        generate_matte_for_clip(h.handle(), clip),
        Err("asset not found".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn footage_that_cannot_be_matted_reports_why() {
    let h = harness();
    *h.services.person_alpha.lock().unwrap() = Some(1.0);
    let dir = h.dir.join("mattes");
    let mask = dir.join("m.mp4");
    let source = crate::testing::fixture("video.mp4");
    let model = Path::new("model.onnx");
    let services = h.services.as_ref();
    assert_eq!(
        generate_matte_one(services, &source, 1.0, 1.0, model, &dir, &mask),
        Err("clip has zero duration".to_string())
    );
    assert!(generate_matte_one(
        services,
        &h.dir.join("missing.mp4"),
        0.0,
        2.0,
        model,
        &dir,
        &mask
    )
    .unwrap_err()
    .contains("failed to open source"));
    let blocker = h.dir.join("a-file");
    std::fs::write(&blocker, b"x").unwrap();
    let inside = blocker.join("inside");
    assert!(generate_matte_one(
        services,
        &source,
        0.0,
        2.0,
        model,
        &inside,
        &inside.join("m.mp4")
    )
    .unwrap_err()
    .contains("failed to create matte cache dir"));
}

fn solid(width: u32, height: u32) -> avcore::SampledFrame {
    avcore::SampledFrame {
        width,
        height,
        rgba: vec![128; (width * height * 4) as usize],
    }
}

#[test]
fn a_matte_needs_at_least_two_frames_of_the_same_size() {
    let h = harness();
    *h.services.person_alpha.lock().unwrap() = Some(1.0);
    let dir = h.dir.join("mattes");
    let mask = dir.join("m.mp4");
    let model = Path::new("model.onnx");
    let services = h.services.as_ref();
    assert_eq!(
        matte_from_frames(services, &[solid(64, 48)], 1.0, model, &dir, &mask),
        Err("couldn't decode enough frames to generate a matte".to_string())
    );
    assert!(matte_from_frames(services, &[], 1.0, model, &dir, &mask).is_err());
    let mixed = [solid(64, 48), solid(32, 32)];
    assert!(matte_from_frames(services, &mixed, 1.0, model, &dir, &mask).is_err());
}

#[test]
fn frames_of_another_size_are_skipped_when_enough_remain() {
    let h = harness();
    *h.services.person_alpha.lock().unwrap() = Some(1.0);
    let dir = h.dir.join("mattes");
    let mask = dir.join("m.mp4");
    let frames = [solid(64, 48), solid(32, 32), solid(64, 48), solid(64, 48)];
    let path = matte_from_frames(
        h.services.as_ref(),
        &frames,
        1.0,
        Path::new("m.onnx"),
        &dir,
        &mask,
    )
    .unwrap();
    assert!(path.exists());
}
