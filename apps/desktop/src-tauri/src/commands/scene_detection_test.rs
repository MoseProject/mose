use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use avcore::SceneCut;
use tauri::Listener;

use super::*;
use crate::commands::timeline::{update_clip, ClipPatch};
use crate::testing::{harness, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(SCENE_CUT_EVENT, move |event| {
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
    h.state()
        .scene_cut_detection_clip_id
        .lock()
        .unwrap()
        .is_some()
}

#[test]
fn detection_on_a_real_clip_reports_its_cut_count_and_frees_the_guard() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let events = collect_events(&h);
    detect_scene_cuts_for_clip(h.handle(), clip).unwrap();
    let count: usize = first_event(&events).parse().unwrap();
    assert!(!running(&h));
    let markers = h.session().project.timeline().markers.len();
    assert_eq!(markers, count);
}

#[test]
fn a_busy_detector_unknown_clips_and_missing_assets_are_refused() {
    let h = harness();
    let (asset, clip) = h.add_fixture_clip();
    *h.state().scene_cut_detection_clip_id.lock().unwrap() = Some(1);
    assert_eq!(
        detect_scene_cuts_for_clip(h.handle(), clip),
        Err("a scene-cut detection is already running".to_string())
    );
    *h.state().scene_cut_detection_clip_id.lock().unwrap() = None;

    assert_eq!(
        detect_scene_cuts_for_clip(h.handle(), 999),
        Err("clip not found".to_string())
    );
    assert!(!running(&h));

    h.session().project.media_library.retain(|a| a.id != asset);
    assert_eq!(
        detect_scene_cuts_for_clip(h.handle(), clip),
        Err("asset not found".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn a_clip_with_no_footage_or_an_unreadable_source_finds_no_cuts() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let source = h.session().project.media_library[0].source_path.clone();
    assert!(detect_scene_cuts_one(&source, 2.0, 2.0).is_empty());
    assert!(detect_scene_cuts_one(&h.dir.join("missing.mp4"), 0.0, 3.0).is_empty());
    assert!(detect_scene_cuts_one(&source, 0.0, 0.01).is_empty());
    let _ = clip;
}

#[test]
fn detected_cuts_become_numbered_chapter_markers_on_the_timeline() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    update_clip(
        h.handle(),
        clip,
        ClipPatch {
            speed_factor: Some(2.0),
            ..Default::default()
        },
    )
    .unwrap();
    let mut session = h.session();
    let cuts = [
        SceneCut {
            at_secs: 1.0,
            score: 0.9,
        },
        SceneCut {
            at_secs: 3.0,
            score: 0.8,
        },
    ];
    apply_detected_scene_cuts(session.project.timeline_mut(), clip, &cuts);
    apply_detected_scene_cuts(session.project.timeline_mut(), clip, &cuts[..1]);
    let markers = &session.project.timeline().markers;
    assert_eq!(markers.len(), 3);
    assert_eq!(markers[0].position_secs, 0.5);
    assert_eq!(markers[1].position_secs, 1.5);
    assert_eq!(markers[0].label, "Chapter 1");
    assert_eq!(markers[2].label, "Chapter 3");
}

#[test]
fn applying_cuts_to_a_missing_or_stalled_clip_adds_nothing() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let cuts = [SceneCut {
        at_secs: 1.0,
        score: 0.9,
    }];
    let mut session = h.session();
    apply_detected_scene_cuts(session.project.timeline_mut(), 999, &cuts);
    session
        .project
        .timeline_mut()
        .clip_mut(clip)
        .unwrap()
        .speed_factor = 0.0;
    apply_detected_scene_cuts(session.project.timeline_mut(), clip, &cuts);
    assert!(session.project.timeline().markers.is_empty());
}

#[test]
fn detected_cuts_are_published_as_markers_and_no_cuts_publish_nothing() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let events = h.count_session_events();
    publish_cuts(&h.handle(), clip, &[]);
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 0);
    publish_cuts(
        &h.handle(),
        clip,
        &[SceneCut {
            at_secs: 1.0,
            score: 0.9,
        }],
    );
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert_eq!(h.session().project.timeline().markers.len(), 1);
    assert!(h.session().dirty);
}
