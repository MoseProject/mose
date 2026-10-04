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

/// The fixture clip repeated `times` times back to back (it is one second long).
fn repeated_fixture_project(h: &Harness, times: usize) -> avcore::Project {
    h.add_fixture_clip();
    for _ in 1..times {
        crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    }
    h.session().project.clone()
}

#[test]
fn playback_runs_to_the_end_of_the_timeline_and_stops_by_itself() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let project = repeated_fixture_project(&h, 3);
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(project);
    wait_for_tick(&ticks, "the first frame", |t| {
        t["hasClip"] == true && t["frameSeq"].as_u64().unwrap_or(0) > 0
    });
    engine.set_playing(true);
    wait_for_tick(&ticks, "playback", |t| t["playing"] == true);
    // Past the two-second health window, then to the end at three seconds.
    wait_for_tick(&ticks, "the end", |t| {
        t["playing"] == false && t["playheadSecs"].as_f64().unwrap_or(0.0) > 2.5
    });
}

#[test]
fn seeking_and_stepping_before_any_content_still_move_the_playhead() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let engine = PreviewEngine::spawn(h.handle());
    engine.seek(2.5);
    engine.step(1);
    wait_for_tick(&ticks, "the seek", |_| true);
    let started = Instant::now();
    while (engine.playhead() - 2.5).abs() > 1e-9 {
        assert!(
            started.elapsed() < Duration::from_secs(20),
            "the playhead never moved"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn replacing_the_content_updates_the_scene_and_a_new_canvas_restarts_the_player() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let mut project = repeated_fixture_project(&h, 1);
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(project.clone());
    wait_for_tick(&ticks, "the first frame", |t| {
        t["frameSeq"].as_u64().unwrap_or(0) > 0
    });

    let before = ticks
        .lock()
        .unwrap()
        .iter()
        .map(|t| t["frameSeq"].as_u64().unwrap_or(0))
        .max()
        .unwrap_or(0);

    // The same canvas with a longer timeline: the running player takes the new scene.
    crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "a frame of the longer timeline", |t| {
        t["frameSeq"].as_u64().unwrap_or(0) > before
    });

    // A portrait export changes the canvas, so a fresh player is built.
    let after_longer = ticks
        .lock()
        .unwrap()
        .iter()
        .map(|t| t["frameSeq"].as_u64().unwrap_or(0))
        .max()
        .unwrap_or(0);
    project.active_sequence_mut().export_settings.aspect_ratio =
        avcore::ExportAspectRatio::Portrait;
    engine.set_content(project);
    wait_for_tick(&ticks, "a frame on the new canvas", |t| {
        t["frameSeq"].as_u64().unwrap_or(0) > after_longer
    });
}

#[test]
fn a_scene_that_needs_approximations_still_plays() {
    let h = harness();
    let ticks = collect_ticks(&h);
    h.add_fixture_clip();
    {
        let mut session = h.session();
        let clip = &mut session.project.timeline_mut().tracks[0].clips[0];
        clip.transition_in = avcore::timeline::TransitionType::Slide;
        clip.transition_duration_secs = 0.5;
        clip.position_keyframes = vec![
            avcore::keyframe::Keyframe {
                time_fraction: 0.0,
                value: avcore::keyframe::Position { x: 0.0, y: 0.0 },
            },
            avcore::keyframe::Keyframe {
                time_fraction: 1.0,
                value: avcore::keyframe::Position { x: 0.2, y: 0.0 },
            },
        ];
    }
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "a frame", |t| {
        t["hasClip"] == true && t["frameSeq"].as_u64().unwrap_or(0) > 0
    });
}

#[test]
fn a_compound_clip_whose_nested_media_is_gone_still_previews_what_it_can() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let (_, clip) = h.add_fixture_clip();
    crate::commands::timeline::create_compound_clip(h.handle(), clip, vec![clip]).unwrap();
    h.session().project.media_library[0].source_path = h.dir.join("missing.mp4");
    let engine = PreviewEngine::spawn(h.handle());
    engine.set_content(h.session().project.clone());
    wait_for_tick(&ticks, "a tick", |_| true);
}

/// An audio output that keeps time on the wall clock and records what the loop asked of it.
struct FakeAudio {
    origin: Instant,
    anchor: f64,
}

static AUDIO_LOG: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn log(entry: &'static str) {
    AUDIO_LOG.lock().unwrap().push(entry);
}

impl crate::bush_adapter::AudioOutput for FakeAudio {
    fn seek_to(&mut self, secs: f64) {
        log("seek_to");
        self.origin = Instant::now();
        self.anchor = secs;
    }

    fn peaks(&self) -> (f32, f32) {
        (0.4, 0.6)
    }

    fn position_secs(&self) -> f64 {
        self.anchor + self.origin.elapsed().as_secs_f64()
    }

    fn pump(&mut self) {
        log("pump");
    }

    fn clear(&mut self) {
        log("clear");
    }

    fn set_scene(&mut self, _scene: bush::SceneSnapshot) {
        log("set_scene");
    }

    fn underflow_callbacks(&self) -> u64 {
        0
    }
}

fn open_fake_audio(
    _scene: &bush::SceneSnapshot,
) -> Option<Box<dyn crate::bush_adapter::AudioOutput>> {
    Some(Box::new(FakeAudio {
        origin: Instant::now(),
        anchor: 0.0,
    }))
}

#[test]
fn playback_follows_the_audio_clock_and_tells_the_output_what_happens() {
    let h = harness();
    let ticks = collect_ticks(&h);
    let project = repeated_fixture_project(&h, 3);
    let engine = PreviewEngine::spawn_with(h.handle(), |rx, app, latest, playhead| {
        crate::bush_adapter::run_with_audio(rx, app, latest, playhead, open_fake_audio)
    });
    engine.set_content(project.clone());
    wait_for_tick(&ticks, "the first frame", |t| {
        t["frameSeq"].as_u64().unwrap_or(0) > 0
    });

    engine.set_playing(true);
    wait_for_tick(&ticks, "the level meter", |t| {
        t["playing"] == true && t["peakL"].as_f64().unwrap_or(0.0) > 0.3
    });
    // A scene change while playing reaches the output, and a seek restarts its queue.
    crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    engine.set_content(h.session().project.clone());
    engine.seek(0.5);
    engine.set_playing(false);
    wait_for_tick(&ticks, "the pause", |t| t["playing"] == false);
    engine.set_playing(true);
    wait_for_tick(&ticks, "the end of the timeline", |t| {
        t["playing"] == false && t["playheadSecs"].as_f64().unwrap_or(0.0) > 3.0
    });

    let seen = AUDIO_LOG.lock().unwrap().clone();
    for entry in ["seek_to", "pump", "clear", "set_scene"] {
        assert!(
            seen.contains(&entry),
            "the output was never asked to {entry}: {seen:?}"
        );
    }
}
