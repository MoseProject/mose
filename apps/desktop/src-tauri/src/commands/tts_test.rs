use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, path_in, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(TTS_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

fn next_event(events: &Arc<Mutex<Vec<String>>>) -> String {
    let started = Instant::now();
    loop {
        if let Some(event) = events.lock().unwrap().first().cloned() {
            return event;
        }
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "no event arrived"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Configures a voice whose `.onnx.json` sidecar exists (the fake synthesizer never reads the
/// model itself).
fn configure_voice(h: &Harness) {
    let model = path_in(&h.dir, "voice.onnx");
    std::fs::write(
        format!("{}.json", model.display()),
        r#"{"audio":{"sample_rate":22050},"espeak":{"voice":"en"},"phoneme_id_map":{}}"#,
    )
    .unwrap();
    h.prefs().view.tts_model_path = model.display().to_string();
}

fn generating(h: &Harness) -> bool {
    *h.state().tts_generating.lock().unwrap()
}

#[test]
fn generated_speech_is_written_as_a_wav_and_imported() {
    let h = harness();
    configure_voice(&h);
    let events = collect_events(&h);
    *h.services.speech.lock().unwrap() = Some(Ok(vec![0.25; 4410]));
    generate_tts(h.handle(), "  hello there  ".into()).unwrap();
    assert!(next_event(&events).contains("\"type\":\"done\""));
    h.wait_until("the speech file to be imported", |s| {
        !s.project.media_library.is_empty()
    });
    let session = h.session();
    let asset = &session.project.media_library[0];
    assert!(asset.file_name.starts_with("tts_") && asset.file_name.ends_with(".wav"));
    assert!(!generating(&h));
}

#[test]
fn blank_text_or_a_missing_voice_is_refused_and_releases_the_guard() {
    let h = harness();
    assert_eq!(
        generate_tts(h.handle(), "hello".into()),
        Err("blank text or no voice model configured".to_string())
    );
    assert!(!generating(&h));
    configure_voice(&h);
    assert_eq!(
        generate_tts(h.handle(), "   ".into()),
        Err("blank text or no voice model configured".to_string())
    );
    assert!(!generating(&h));
}

#[test]
fn only_one_generation_runs_at_a_time() {
    let h = harness();
    configure_voice(&h);
    *h.state().tts_generating.lock().unwrap() = true;
    assert_eq!(
        generate_tts(h.handle(), "hello".into()),
        Err("a text-to-speech generation is already running".to_string())
    );
}

#[test]
fn a_synthesis_failure_is_reported() {
    let h = harness();
    configure_voice(&h);
    let events = collect_events(&h);
    *h.services.speech.lock().unwrap() = Some(Err(avcore::TtsError::NoAudio));
    generate_tts(h.handle(), "...".into()).unwrap();
    let event = next_event(&events);
    assert!(event.contains("\"type\":\"failed\"") && event.contains("no speakable content"));
    assert!(!generating(&h));
}

#[test]
fn a_voice_without_its_config_file_fails_before_synthesizing() {
    let h = harness();
    h.prefs().view.tts_model_path = path_in(&h.dir, "no-config.onnx").display().to_string();
    let events = collect_events(&h);
    generate_tts(h.handle(), "hello".into()).unwrap();
    assert!(next_event(&events).contains("\"type\":\"failed\""));
}

#[test]
fn speech_files_are_written_next_to_the_preferences() {
    let dir = tts_output_dir();
    assert_eq!(dir.file_name().unwrap(), "tts_output");
    assert_eq!(dir.parent(), crate::prefs::prefs_path().parent());
}
