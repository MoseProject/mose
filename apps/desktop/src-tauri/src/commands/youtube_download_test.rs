use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{fixture, harness, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(YOUTUBE_DOWNLOAD_EVENT, move |event| {
        sink.lock().unwrap().push(event.payload().to_string());
    });
    events
}

fn wait_for(events: &Arc<Mutex<Vec<String>>>, needle: &str) {
    let started = Instant::now();
    while !events.lock().unwrap().iter().any(|e| e.contains(needle)) {
        assert!(
            started.elapsed() < Duration::from_secs(60),
            "never saw {needle}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn running(h: &Harness) -> bool {
    h.state().youtube_download_cancel.lock().unwrap().is_some()
}

/// Makes the helper "installed" and scripts what the next download does.
fn script(
    h: &Harness,
    run: impl FnOnce(
            &str,
            YoutubeDownloadTarget,
            &AtomicBool,
            &mut dyn FnMut(f32),
        ) -> Result<PathBuf, avcore::YoutubeDownloadError>
        + Send
        + 'static,
) {
    h.services.youtube_available.store(true, Ordering::SeqCst);
    *h.services.youtube.lock().unwrap() = Some(Box::new(run));
}

fn start(h: &Harness, url: &str, format: &str, quality: &str) -> Result<(), String> {
    download_youtube(h.handle(), url.into(), format.into(), quality.into())
}

#[test]
fn formats_and_qualities_map_to_download_targets() {
    for (quality, expected) in [
        ("360", Mp4Quality::P360),
        ("480", Mp4Quality::P480),
        ("720", Mp4Quality::P720),
        ("1080", Mp4Quality::P1080),
        ("best", Mp4Quality::Best),
        ("anything", Mp4Quality::Best),
    ] {
        assert_eq!(
            parse_target("mp4", quality),
            Some(YoutubeDownloadTarget::Mp4(expected))
        );
    }
    for (quality, expected) in [
        ("128", Mp3Bitrate::K128),
        ("320", Mp3Bitrate::K320),
        ("192", Mp3Bitrate::K192),
        ("anything", Mp3Bitrate::K192),
    ] {
        assert_eq!(
            parse_target("mp3", quality),
            Some(YoutubeDownloadTarget::Mp3(expected))
        );
    }
    assert_eq!(parse_target("avi", "720"), None);
}

#[test]
fn the_command_refuses_blank_urls_unknown_formats_and_a_missing_helper() {
    let h = harness();
    assert!(!is_yt_dlp_available(h.state()));
    assert_eq!(
        start(&h, "   ", "mp4", "best"),
        Err("blank URL".to_string())
    );
    assert_eq!(
        start(&h, "https://example.com/v", "avi", "best"),
        Err("invalid format".to_string())
    );
    assert_eq!(
        start(&h, "https://example.com/v", "mp4", "best"),
        Err("yt-dlp is not installed".to_string())
    );
    assert!(!running(&h));
    h.services.youtube_available.store(true, Ordering::SeqCst);
    assert!(is_yt_dlp_available(h.state()));
}

#[test]
fn a_finished_download_reports_progress_then_done_and_imports_the_file() {
    let h = harness();
    let events = collect_events(&h);
    let file = fixture("audio.m4a");
    script(&h, move |url, target, _cancel, progress| {
        assert_eq!(url, "https://example.com/v");
        assert_eq!(target, YoutubeDownloadTarget::Mp3(Mp3Bitrate::K320));
        progress(0.5);
        Ok(file)
    });
    start(&h, " https://example.com/v ", "mp3", "320").unwrap();
    wait_for(&events, "\"type\":\"done\"");
    let seen = events.lock().unwrap().clone();
    assert!(seen
        .iter()
        .any(|e| e.contains("\"type\":\"progress\"") && e.contains("0.5")));
    h.wait_until("the downloaded file to be imported", |s| {
        !s.project.media_library.is_empty()
    });
    assert!(!running(&h));
}

#[test]
fn a_failed_download_reports_the_message() {
    let h = harness();
    let events = collect_events(&h);
    script(&h, |_, _, _, _| {
        Err(avcore::YoutubeDownloadError::Failed("no network".into()))
    });
    start(&h, "https://example.com/v", "mp4", "720").unwrap();
    wait_for(&events, "\"type\":\"failed\"");
    assert!(events
        .lock()
        .unwrap()
        .iter()
        .any(|e| e.contains("no network")));
    assert!(!running(&h));
}

#[test]
fn only_one_download_runs_at_a_time_and_cancelling_stops_it() {
    let h = harness();
    let events = collect_events(&h);
    script(&h, |_, _, cancel, _| {
        while !cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(5));
        }
        Err(avcore::YoutubeDownloadError::Failed("cancelled".into()))
    });
    start(&h, "https://example.com/v", "mp4", "best").unwrap();
    assert!(running(&h));
    assert_eq!(
        start(&h, "https://example.com/other", "mp4", "best"),
        Err("a download is already running".to_string())
    );

    cancel_youtube_download(h.state());
    wait_for(&events, "cancelled");
}

#[test]
fn cancelling_with_nothing_running_does_nothing() {
    let h = harness();
    cancel_youtube_download(h.state());
    assert!(!running(&h));
}

#[test]
fn downloads_land_next_to_the_preferences() {
    let dir = youtube_downloads_dir();
    assert_eq!(dir.file_name().unwrap(), "youtube_downloads");
    assert_eq!(dir.parent(), crate::prefs::prefs_path().parent());
}
