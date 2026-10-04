use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::Listener;

use super::*;
use crate::testing::{harness, Harness};

fn collect_events(h: &Harness) -> Arc<Mutex<Vec<String>>> {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);
    h.app.listen(UPDATE_CHECK_EVENT, move |event| {
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

fn release(version: &str) -> avcore::LatestRelease {
    avcore::LatestRelease {
        version: version.into(),
        html_url: "https://example.com/release".into(),
        auto_update_available: true,
    }
}

#[test]
fn a_newer_release_is_announced_with_its_details() {
    let h = harness();
    let events = collect_events(&h);
    *h.services.release.lock().unwrap() = Some(Ok(release("999.0.0")));
    check_for_update(h.handle());
    let event = next_event(&events);
    assert!(event.contains("\"type\":\"newerVersionAvailable\""));
    assert!(event.contains("999.0.0"));
    assert!(event.contains("https://example.com/release") && event.contains("html_url"));
    assert!(event.contains("\"auto_update_available\":true"));
}

#[test]
fn the_current_release_is_reported_as_up_to_date() {
    let h = harness();
    let events = collect_events(&h);
    *h.services.release.lock().unwrap() = Some(Ok(release(env!("CARGO_PKG_VERSION"))));
    check_for_update(h.handle());
    assert!(next_event(&events).contains("\"type\":\"upToDate\""));
}

#[test]
fn a_failed_check_is_reported() {
    let h = harness();
    let events = collect_events(&h);
    check_for_update(h.handle());
    assert!(next_event(&events).contains("\"type\":\"failed\""));
}

#[test]
fn installing_reports_the_installed_version_or_that_nothing_changed() {
    let h = harness();
    let events = collect_events(&h);
    *h.services.update.lock().unwrap() = Some(Ok(avcore::ApplyUpdateOutcome::Updated {
        version: "2.0.0".into(),
    }));
    install_available_update(h.handle(), "2.0.0".into());
    let event = next_event(&events);
    assert!(event.contains("\"type\":\"installed\"") && event.contains("2.0.0"));

    let h = harness();
    let events = collect_events(&h);
    *h.services.update.lock().unwrap() = Some(Ok(avcore::ApplyUpdateOutcome::UpToDate));
    install_available_update(h.handle(), "1.0.0".into());
    assert!(next_event(&events).contains("\"type\":\"upToDate\""));
}

#[test]
fn a_failed_install_is_reported() {
    let h = harness();
    let events = collect_events(&h);
    install_available_update(h.handle(), "2.0.0".into());
    assert!(next_event(&events).contains("\"type\":\"installFailed\""));
}

#[test]
fn restarting_starts_the_new_executable_and_exits_this_one() {
    let h = harness();
    restart_after_update(h.handle()).unwrap();
    assert_eq!(h.services.restarts.load(Ordering::SeqCst), 1);
    assert_eq!(*h.services.exits.lock().unwrap(), vec![0]);
}

#[test]
fn a_restart_that_cannot_start_reports_the_error_and_keeps_running() {
    let h = harness();
    *h.services.restart.lock().unwrap() =
        Some(Err(avcore::ApplyUpdateError::Restart("denied".into())));
    let error = restart_after_update(h.handle()).unwrap_err();
    assert!(error.contains("denied"));
    assert!(h.services.exits.lock().unwrap().is_empty());
}
