use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use avcore::transcribe::TranscribeWord;
use avcore::{TranscribeError, TranscribeOutcome, TranscribeSegment};
use tauri::Listener;

use super::*;
use crate::testing::{fixture, harness, path_in, video_asset, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(TRANSCRIBE_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

fn wait_for(events: &Arc<Mutex<Vec<String>>>, needle: &str) {
    let started = Instant::now();
    while !events.lock().unwrap().iter().any(|e| e.contains(needle)) {
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "never saw {needle}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn word(text: &str, start: f64, end: f64) -> TranscribeWord {
    TranscribeWord {
        text: text.into(),
        start_secs: start,
        end_secs: end,
        confidence: 0.9,
    }
}

fn segment(start: f64, end: f64, text: &str, words: Vec<TranscribeWord>) -> TranscribeSegment {
    TranscribeSegment {
        start_secs: start,
        end_secs: end,
        text: text.into(),
        words,
    }
}

/// A harness with asset 1 in a saved project and a Whisper model the fake service "found".
fn ready() -> Harness {
    let h = harness();
    h.session().project.file_path = Some(path_in(&h.dir, "project.ocproj"));
    h.session().project.media_library.push(video_asset(1, 10.0));
    *h.services.whisper_model.lock().unwrap() = Some(fixture("audio.m4a"));
    h
}

fn transcribing(h: &Harness) -> bool {
    h.state().transcribing_asset_id.lock().unwrap().is_some()
}

#[test]
fn a_transcription_becomes_text_clips_and_a_searchable_document() {
    let h = ready();
    let events = collect_events(&h);
    *h.services.transcript.lock().unwrap() = Some(Ok(TranscribeOutcome::Completed(vec![
        segment(
            1.0,
            2.5,
            "hello world",
            vec![word("hello", 1.0, 1.5), word("world", 1.5, 2.5)],
        ),
        segment(3.0, 3.1, "short", Vec::new()),
    ])));
    transcribe_asset(h.handle(), 1).unwrap();
    wait_for(&events, "\"type\":\"done\"");

    let session = h.session();
    let clips = &session.project.timeline().tracks[0].text_clips;
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0].text, "hello world");
    assert_eq!(clips[0].start_secs, 1.0);
    assert_eq!(clips[0].duration_secs, 1.5);
    assert!(clips[0].highlight_enabled);
    assert_eq!(clips[0].words[1].start_secs, 0.5);
    assert_eq!(
        clips[1].duration_secs, 0.3,
        "very short segments get a minimum length"
    );
    assert!(!clips[1].highlight_enabled);
    drop(session);

    assert!(events.lock().unwrap()[0].contains("\"type\":\"started\""));
    assert!(!transcribing(&h));
    let document = get_transcript(h.state(), 1).unwrap().unwrap();
    assert_eq!(document.asset_id, 1);
    let hits = search_transcripts(h.state(), "world".into());
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].asset_name, "asset-1.mp4");
}

#[test]
fn a_cancelled_transcription_keeps_the_segments_it_had() {
    let h = ready();
    let events = collect_events(&h);
    *h.services.transcript.lock().unwrap() = Some(Ok(TranscribeOutcome::Cancelled(vec![segment(
        0.0,
        1.0,
        "partial",
        Vec::new(),
    )])));
    transcribe_asset(h.handle(), 1).unwrap();
    wait_for(&events, "\"type\":\"done\"");
    assert_eq!(h.session().project.timeline().tracks[0].text_clips.len(), 1);
}

#[test]
fn silence_is_reported_as_no_speech_and_adds_nothing() {
    let h = ready();
    let events = collect_events(&h);
    *h.services.transcript.lock().unwrap() = Some(Ok(TranscribeOutcome::Completed(Vec::new())));
    transcribe_asset(h.handle(), 1).unwrap();
    wait_for(&events, "\"type\":\"noSpeech\"");
    assert!(h.session().project.timeline().tracks.is_empty());
}

#[test]
fn a_failed_transcription_reports_why_and_frees_the_guard() {
    let h = ready();
    let events = collect_events(&h);
    *h.services.transcript.lock().unwrap() = Some(Err(TranscribeError::NoAudio));
    transcribe_asset(h.handle(), 1).unwrap();
    wait_for(&events, "\"type\":\"failed\"");
    assert!(!transcribing(&h));
}

#[test]
fn starting_is_refused_for_unknown_assets_missing_models_or_a_busy_worker() {
    let h = ready();
    assert_eq!(
        transcribe_asset(h.handle(), 99),
        Err("asset not found".to_string())
    );
    assert!(!transcribing(&h));

    *h.services.whisper_model.lock().unwrap() = None;
    assert_eq!(
        transcribe_asset(h.handle(), 1),
        Err("no Whisper model configured".to_string())
    );
    assert!(!transcribing(&h));

    *h.state().transcribing_asset_id.lock().unwrap() = Some(1);
    assert_eq!(
        transcribe_asset(h.handle(), 1),
        Err("a transcription is already running".to_string())
    );
}

#[test]
fn a_configured_model_file_wins_over_the_bundled_one() {
    let h = ready();
    h.prefs().view.whisper_model_path = fixture("video.mp4").display().to_string();
    assert_eq!(
        resolve_whisper_model_path(h.state().inner()),
        Some(fixture("video.mp4"))
    );
    h.prefs().view.whisper_model_path = path_in(&h.dir, "missing.bin").display().to_string();
    assert_eq!(
        resolve_whisper_model_path(h.state().inner()),
        Some(fixture("audio.m4a"))
    );
    h.prefs().view.whisper_model_path = "  ".into();
    assert_eq!(
        resolve_whisper_model_path(h.state().inner()),
        Some(fixture("audio.m4a"))
    );
}

#[test]
fn a_clip_less_asset_has_no_transcript_yet() {
    let h = ready();
    assert!(get_transcript(h.state(), 1).unwrap().is_none());
    assert!(search_transcripts(h.state(), "anything".into()).is_empty());
}

#[test]
fn seeking_to_a_transcript_word_moves_the_playhead_through_the_clip_mapping() {
    let h = harness();
    let (asset, clip) = h.add_fixture_clip();
    crate::commands::timeline::update_clip(
        h.handle(),
        clip,
        crate::commands::timeline::ClipPatch {
            speed_factor: Some(2.0),
            ..Default::default()
        },
    )
    .unwrap();
    let _ = h.drain_preview();
    assert_eq!(seek_to_transcript_word(h.handle(), asset, 4.0), Ok(true));
    assert_eq!(h.session().project.timeline().playhead_secs, 2.0);
    assert!(h
        .drain_preview()
        .iter()
        .any(|c| matches!(c, crate::preview_engine::Command::Seek(secs) if *secs == 2.0)));
    assert_eq!(seek_to_transcript_word(h.handle(), 999, 1.0), Ok(false));
}
