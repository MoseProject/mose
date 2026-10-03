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
    assert_eq!(
        download_youtube(h.handle(), "   ".into(), "mp4".into(), "best".into()),
        Err("blank URL".to_string())
    );
    assert_eq!(
        download_youtube(
            h.handle(),
            "https://example.com/v".into(),
            "avi".into(),
            "best".into()
        ),
        Err("invalid format".to_string())
    );
    // No `ytbridge` sits next to the test binary.
    assert_eq!(is_yt_dlp_available(), false);
    assert_eq!(
        download_youtube(
            h.handle(),
            "https://example.com/v".into(),
            "mp4".into(),
            "best".into()
        ),
        Err("yt-dlp is not installed".to_string())
    );
    assert!(!running(&h));
}

#[test]
fn a_finished_download_reports_progress_then_done_and_imports_the_file() {
    let h = harness();
    let events = collect_events(&h);
    let file = fixture("audio.m4a");
    start_download(
        h.handle(),
        " https://example.com/v ".into(),
        "mp3".into(),
        "320".into(),
        true,
        move |url, target, _dir, _cancel, progress| {
            assert_eq!(url, "https://example.com/v");
            assert_eq!(target, YoutubeDownloadTarget::Mp3(Mp3Bitrate::K320));
            progress(0.5);
            Ok(file)
        },
    )
    .unwrap();
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
    start_download(
        h.handle(),
        "https://example.com/v".into(),
        "mp4".into(),
        "720".into(),
        true,
        |_, _, _, _, _| Err(avcore::YoutubeDownloadError::Failed("no network".into())),
    )
    .unwrap();
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
    start_download(
        h.handle(),
        "https://example.com/v".into(),
        "mp4".into(),
        "best".into(),
        true,
        |_, _, _, cancel, _| {
            while !cancel.load(std::sync::atomic::Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(avcore::YoutubeDownloadError::Failed("cancelled".into()))
        },
    )
    .unwrap();
    assert!(running(&h));
    assert_eq!(
        start_download(
            h.handle(),
            "https://example.com/other".into(),
            "mp4".into(),
            "best".into(),
            true,
            |_, _, _, _, _| unreachable!("a second download must not start"),
        ),
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
