use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{fixture, harness, Harness};

const FAST: WatchTiming = WatchTiming {
    poll: Duration::from_millis(20),
    stable_for: Duration::ZERO,
};

/// Collects the JSON payload of every watch-folder event the harness's app emits.
fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(WATCH_FOLDER_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

fn wait_for(events: &Arc<Mutex<Vec<String>>>, needle: &str) {
    let started = Instant::now();
    while !events.lock().unwrap().iter().any(|e| e.contains(needle)) {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "never saw {needle}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn watched_dir(h: &Harness, name: &str) -> PathBuf {
    let dir = h.dir.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn the_folder_dialog_returns_the_chosen_folder_or_nothing() {
    crate::dialogs::scripted::answer(Some(vec![PathBuf::from("C:/clips")]));
    assert_eq!(
        pick_watch_folder(),
        Some(PathBuf::from("C:/clips").display().to_string())
    );
    crate::dialogs::scripted::answer(None);
    assert_eq!(pick_watch_folder(), None);
}

#[test]
fn a_new_video_is_detected_normalized_and_reported() {
    let h = harness();
    let dir = watched_dir(&h, "watch");
    let source = dir.join("clip.mp4");
    std::fs::copy(fixture("video.mp4"), &source).unwrap();
    let events = collect_events(&h);

    start_watching(h.handle(), dir.clone(), FAST);
    wait_for(&events, "\"type\":\"done\"");
    stop_watching_folder(h.state());

    let seen = events.lock().unwrap().clone();
    for kind in ["detected", "processing", "done"] {
        assert!(
            seen.iter()
                .any(|e| e.contains(&format!("\"type\":\"{kind}\""))),
            "{kind}"
        );
    }
    let output = avcore::output_path_for(&dir, avcore::DEFAULT_OUTPUT_SUBFOLDER, &source);
    assert!(output.exists());

    add_watched_file_to_project(
        h.handle(),
        dir.display().to_string(),
        source.display().to_string(),
    );
    h.wait_until("the processed file to be imported", |s| {
        s.project.media_library.iter().any(|a| a.loudness.is_some())
    });
}

#[test]
fn a_file_that_cannot_be_processed_is_reported_once() {
    let h = harness();
    let dir = watched_dir(&h, "bad");
    std::fs::write(dir.join("broken.mp4"), b"this is not a video").unwrap();
    std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();
    let events = collect_events(&h);

    start_watching(h.handle(), dir, FAST);
    wait_for(&events, "\"type\":\"failed\"");
    std::thread::sleep(Duration::from_millis(200));
    stop_watching_folder(h.state());

    let failures = events
        .lock()
        .unwrap()
        .iter()
        .filter(|e| e.contains("\"type\":\"failed\""))
        .count();
    assert_eq!(failures, 1);
}

#[test]
fn a_file_still_growing_is_reported_as_stabilizing() {
    let h = harness();
    let dir = watched_dir(&h, "growing");
    std::fs::write(dir.join("recording.mp4"), b"partial").unwrap();
    let events = collect_events(&h);
    let timing = WatchTiming {
        poll: Duration::from_millis(20),
        stable_for: Duration::from_secs(3600),
    };

    start_watching(h.handle(), dir, timing);
    wait_for(&events, "\"type\":\"stabilizing\"");
    stop_watching_folder(h.state());
    assert!(!events
        .lock()
        .unwrap()
        .iter()
        .any(|e| e.contains("\"type\":\"processing\"")));
}

#[test]
fn a_folder_that_does_not_exist_is_waited_on_quietly() {
    let h = harness();
    let events = collect_events(&h);
    start_watching(h.handle(), h.dir.join("not-there"), FAST);
    std::thread::sleep(Duration::from_millis(150));
    stop_watching_folder(h.state());
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn starting_a_new_watch_stops_the_previous_one() {
    let h = harness();
    start_watching_folder(h.handle(), h.dir.join("one").display().to_string()).unwrap();
    let first = h.state().watch_folder_stop.lock().unwrap().clone().unwrap();
    start_watching_folder(h.handle(), h.dir.join("two").display().to_string()).unwrap();
    assert!(first.load(Ordering::Relaxed));
    stop_watching_folder(h.state());
    assert!(h.state().watch_folder_stop.lock().unwrap().is_none());
    stop_watching_folder(h.state());
}

#[test]
fn the_default_timing_matches_the_documented_cadence() {
    let timing = WatchTiming::default();
    assert_eq!(timing.poll, POLL_INTERVAL);
    assert_eq!(
        timing.stable_for,
        Duration::from_secs(avcore::DEFAULT_STABLE_SECS)
    );
}
