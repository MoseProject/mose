use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, Harness};

/// Collects the parsed `preview-tick` events the engine emits on the harness's app.
fn collect_ticks(h: &Harness) -> Arc<Mutex<Vec<serde_json::Value>>> {
    let ticks = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&ticks);
    h.app.listen(PREVIEW_TICK, move |event| {
        sink.lock()
            .unwrap()
            .push(serde_json::from_str(event.payload()).unwrap());
    });
    ticks
}

fn wait_for_tick(
    ticks: &Arc<Mutex<Vec<serde_json::Value>>>,
    what: &str,
    done: impl Fn(&serde_json::Value) -> bool,
) {
    let started = Instant::now();
    loop {
        if ticks.lock().unwrap().iter().any(&done) {
            return;
        }
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "timed out waiting for {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_engine_decodes_a_clip_and_publishes_frames_and_transport_state() {
    let h = harness();
    let ticks = collect_ticks(&h);
    h.add_fixture_clip();
    let engine = PreviewEngine::spawn(h.handle());
    assert!(engine.latest_frame().is_none());

    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "the first frame", |t| {
        t["hasClip"] == true && t["frameSeq"].as_u64().unwrap_or(0) > 0
    });
    assert!(engine.latest_frame().is_some());

    engine.seek(0.5);
    wait_for_tick(&ticks, "the seek", |t| {
        (t["playheadSecs"].as_f64().unwrap_or(0.0) - 0.5).abs() < 0.2
    });
    engine.step(1);
    engine.set_playing(true);
    wait_for_tick(&ticks, "playback", |t| t["playing"] == true);
    engine.set_playing(false);
    wait_for_tick(&ticks, "the pause", |t| {
        t["playing"] == false && t["playheadSecs"].as_f64().unwrap_or(0.0) > 0.0
    });
    assert!(engine.playhead() >= 0.0);
}

#[test]
fn an_empty_timeline_has_no_clip() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "an empty tick", |t| t["hasClip"] == false);
}

#[test]
fn a_source_that_cannot_be_decoded_is_reported_as_an_error_tick() {
    let h = harness();
    let ticks = collect_ticks(&h);
    h.add_fixture_clip();
    let corrupt = h.dir.join("corrupt.mp4");
    std::fs::write(&corrupt, b"this is not a video").unwrap();
    h.session().project.media_library[0].source_path = corrupt;
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "the error", |t| !t["error"].is_null());
}

#[test]
fn a_clip_whose_source_file_is_gone_previews_nothing() {
    let h = harness();
    let ticks = collect_ticks(&h);
    h.add_fixture_clip();
    h.session().project.media_library[0].source_path = h.dir.join("missing.mp4");
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "an empty tick", |t| t["hasClip"] == false);
}

#[test]
fn a_clip_whose_file_is_missing_but_proxy_exists_previews_from_the_proxy() {
    let mut asset = crate::testing::video_asset(1, 5.0);
    asset.source_path = PathBuf::from("gone.mp4");
    assert!(preview_source_path(&asset).is_none());
    asset.proxy_path = Some(crate::testing::fixture("video.mp4"));
    assert_eq!(
        preview_source_path(&asset),
        Some(crate::testing::fixture("video.mp4"))
    );
    asset.proxy_path = None;
    asset.source_path = crate::testing::fixture("video.mp4");
    assert_eq!(
        preview_source_path(&asset),
        Some(crate::testing::fixture("video.mp4"))
    );
}
