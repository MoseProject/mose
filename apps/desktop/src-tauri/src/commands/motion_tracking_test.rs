use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(MOTION_TRACK_EVENT, move |event| {
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
    h.state().motion_tracking_clip_id.lock().unwrap().is_some()
}

#[test]
fn tracking_a_real_clip_writes_position_keyframes() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let events = collect_events(&h);
    motion_track_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2, 0.1).unwrap();
    let count: usize = first_event(&events).parse().unwrap();
    assert!(count >= 2, "a clip with footage yields keyframes");
    assert!(!running(&h));
    let session = h.session();
    let tracked = session.project.timeline().tracks[0].clips[0]
        .position_keyframes
        .len();
    assert_eq!(tracked, count);
    assert!(session.dirty);
}

#[test]
fn out_of_range_inputs_are_clamped_rather_than_rejected() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    let events = collect_events(&h);
    motion_track_clip(h.handle(), clip, -3.0, 9.0, 0.0, 9.0, 0.0).unwrap();
    first_event(&events);
}

#[test]
fn a_busy_tracker_unknown_clips_and_missing_assets_are_refused() {
    let h = harness();
    let (asset, clip) = h.add_fixture_clip();
    *h.state().motion_tracking_clip_id.lock().unwrap() = Some(1);
    assert_eq!(
        motion_track_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2, 0.1),
        Err("a motion track is already running".to_string())
    );
    *h.state().motion_tracking_clip_id.lock().unwrap() = None;

    assert_eq!(
        motion_track_clip(h.handle(), 999, 0.5, 0.5, 0.2, 0.2, 0.1),
        Err("clip not found".to_string())
    );
    assert!(!running(&h));
    h.session().project.media_library.retain(|a| a.id != asset);
    assert_eq!(
        motion_track_clip(h.handle(), clip, 0.5, 0.5, 0.2, 0.2, 0.1),
        Err("asset not found".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn a_clip_with_no_footage_or_an_unreadable_source_tracks_nothing() {
    let h = harness();
    h.add_fixture_clip();
    let source = h.session().project.media_library[0].source_path.clone();
    let origin = Position { x: 0.0, y: 0.0 };
    assert!(motion_track_one(&source, 2.0, 2.0, origin, 0.5, 0.5, 0.2, 0.2, 0.1).is_empty());
    assert!(motion_track_one(
        &h.dir.join("missing.mp4"),
        0.0,
        3.0,
        origin,
        0.5,
        0.5,
        0.2,
        0.2,
        0.1
    )
    .is_empty());
}

fn flat_frame(value: u8) -> avcore::GrayFrame {
    avcore::GrayFrame {
        width: 32,
        height: 32,
        data: vec![value; 32 * 32],
    }
}

#[test]
fn fewer_than_two_decoded_frames_show_no_motion() {
    let origin = Position { x: 0.0, y: 0.0 };
    assert!(track_decoded_frames(Vec::new(), origin, 0.5, 0.5, 0.2, 0.2, 0.1).is_empty());
    assert!(
        track_decoded_frames(vec![(0.0, flat_frame(10))], origin, 0.5, 0.5, 0.2, 0.2, 0.1)
            .is_empty()
    );
}

#[test]
fn two_decoded_frames_produce_keyframes_at_their_time_fractions() {
    let origin = Position { x: 0.0, y: 0.0 };
    let keyframes = track_decoded_frames(
        vec![(0.0, flat_frame(10)), (1.0, flat_frame(10))],
        origin,
        0.5,
        0.5,
        0.2,
        0.2,
        0.1,
    );
    assert_eq!(keyframes.len(), 2);
    assert_eq!(keyframes[1].time_fraction, 1.0);
}
