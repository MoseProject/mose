use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use avcore::FaceBox;
use tauri::Listener;

use super::*;
use crate::testing::{harness, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(AUTO_REFRAME_EVENT, move |event| {
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
            started.elapsed() < Duration::from_secs(60),
            "no event arrived"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn running(h: &Harness) -> bool {
    h.state().auto_reframing_clip_id.lock().unwrap().is_some()
}

/// A fixture clip in a portrait sequence, with a reframe model configured.
fn ready() -> (Harness, u64, u64) {
    let h = harness();
    let (asset, clip) = h.add_fixture_clip();
    h.session()
        .project
        .active_sequence_mut()
        .export_settings
        .aspect_ratio = avcore::ExportAspectRatio::Portrait;
    h.prefs().view.reframe_model_path = "model.onnx".into();
    (h, asset, clip)
}

fn face(x: f32, y: f32) -> FaceBox {
    FaceBox {
        x,
        y,
        w: 0.1,
        h: 0.1,
        score: 0.9,
    }
}

fn crop_of(h: &Harness) -> (f32, f32, f32, f32) {
    let s = h.session();
    let c = &s.project.timeline().tracks[0].clips[0];
    (c.crop_x, c.crop_y, c.crop_w, c.crop_h)
}

#[test]
fn a_seeded_clip_is_cropped_around_the_seed_without_any_model() {
    let (h, _, clip) = ready();
    h.prefs().view.reframe_model_path = String::new();
    h.session()
        .project
        .timeline_mut()
        .clip_mut(clip)
        .unwrap()
        .reframe_seed_point = Some((0.9, 0.5));
    let events = collect_events(&h);
    auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":true"));
    let (x, _, w, _) = crop_of(&h);
    assert!(w < 1.0, "portrait crop of a landscape clip is narrower");
    assert!(
        (x + w - 1.0).abs() < 1e-4,
        "a subject at the right edge pushes the crop to the edge"
    );
    assert!(!running(&h));
}

#[test]
fn a_detected_face_centers_the_crop_on_it() {
    let (h, _, clip) = ready();
    *h.services.faces.lock().unwrap() = Some(vec![face(0.0, 0.4)]);
    let events = collect_events(&h);
    auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":true"));
    let (x, ..) = crop_of(&h);
    assert_eq!(x, 0.0, "a face at the left edge pins the crop to the left");
}

#[test]
fn when_detection_fails_the_crop_stays_centered() {
    let (h, _, clip) = ready();
    *h.services.faces.lock().unwrap() = None;
    let events = collect_events(&h);
    auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":false"));
    let (x, _, w, _) = crop_of(&h);
    assert!((x - (1.0 - w) / 2.0).abs() < 1e-4);
}

#[test]
fn starting_is_refused_without_a_model_clip_resolution_or_when_busy() {
    let (h, asset, clip) = ready();
    let permit = AnalysisPermit::acquire(&h.handle()).unwrap();
    assert_eq!(
        auto_reframe_clip(h.handle(), clip),
        Err("analysisBusy".to_string())
    );
    assert_eq!(
        dynamic_auto_reframe_clip(h.handle(), clip),
        Err("analysisBusy".to_string())
    );
    drop(permit);

    assert_eq!(
        auto_reframe_clip(h.handle(), 999),
        Err("clip not found".to_string())
    );
    assert_eq!(
        dynamic_auto_reframe_clip(h.handle(), 999),
        Err("clip not found".to_string())
    );

    h.prefs().view.reframe_model_path = "  ".into();
    assert_eq!(
        auto_reframe_clip(h.handle(), clip),
        Err("no auto-reframe model configured".to_string())
    );
    assert_eq!(
        dynamic_auto_reframe_clip(h.handle(), clip),
        Err("no auto-reframe model configured".to_string())
    );
    h.prefs().view.reframe_model_path = "model.onnx".into();

    h.session()
        .project
        .media_library
        .iter_mut()
        .for_each(|a| a.resolution = None);
    assert_eq!(
        auto_reframe_clip(h.handle(), clip),
        Err("asset resolution unknown".to_string())
    );
    assert_eq!(
        dynamic_auto_reframe_clip(h.handle(), clip),
        Err("asset resolution unknown".to_string())
    );
    h.session().project.media_library.retain(|a| a.id != asset);
    assert_eq!(
        auto_reframe_clip(h.handle(), clip),
        Err("asset resolution unknown".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn a_dynamic_reframe_follows_a_seed_with_keyframes() {
    let (h, _, clip) = ready();
    h.session()
        .project
        .timeline_mut()
        .clip_mut(clip)
        .unwrap()
        .reframe_seed_point = Some((0.2, 0.5));
    let events = collect_events(&h);
    dynamic_auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":true"));
    assert!(!running(&h));
    let session = h.session();
    let c = &session.project.timeline().tracks[0].clips[0];
    assert!(!c.crop_x_keyframes.is_empty());
    assert!(session.dirty);
}

#[test]
fn a_dynamic_reframe_tracks_faces_across_sampled_frames() {
    let (h, _, clip) = ready();
    *h.services.faces.lock().unwrap() = Some(vec![face(0.6, 0.4)]);
    let events = collect_events(&h);
    dynamic_auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":true"));
}

#[test]
fn a_dynamic_reframe_without_faces_reports_no_subject() {
    let (h, _, clip) = ready();
    *h.services.faces.lock().unwrap() = None;
    let events = collect_events(&h);
    dynamic_auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":false"));
}

#[test]
fn a_dynamic_reframe_of_an_unreadable_source_reports_no_subject() {
    let (h, _, clip) = ready();
    h.session().project.media_library[0].source_path = h.dir.join("missing.mp4");
    let events = collect_events(&h);
    dynamic_auto_reframe_clip(h.handle(), clip).unwrap();
    assert!(first_event(&events).contains("\"subjectFound\":false"));
    assert!(!running(&h));
}

#[test]
fn large_frames_are_scaled_down_and_small_ones_kept() {
    let big = vec![200u8; (2000 * 1000 * 4) as usize];
    let (w, h, rgba) = downscale_frame_rgba(2000, 1000, big, 960);
    assert_eq!((w, h), (960, 480));
    assert_eq!(rgba.len(), (960 * 480 * 4) as usize);
    let small = vec![1u8; 4 * 4 * 4];
    assert_eq!(
        downscale_frame_rgba(4, 4, small.clone(), 960),
        (4, 4, small)
    );
}

#[test]
fn a_frame_is_extracted_from_a_real_video_and_a_missing_one_gives_none() {
    let source = crate::testing::fixture("video.mp4");
    let (w, h, rgba) = extract_frame(&source, 0.5).unwrap();
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    assert!(extract_frame(&source.with_file_name("missing.mp4"), 0.5).is_none());
}
