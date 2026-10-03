use std::sync::atomic::Ordering;

use super::*;
use crate::error_reporting::tests::{
    crash_file, shared_lock, shared_queue_dir, temp_dir, CRASH_BODY,
};
use crate::testing::{harness, Harness};

/// Stages a crash report from `CRASH_BODY` for review, as `main` does at launch.
fn stage_crash(h: &Harness, timestamp: u64) {
    let dir = temp_dir(&format!("cmd-crash-{timestamp}"));
    crash_file(&dir, timestamp, CRASH_BODY);
    let crash = error_reporting::find_latest_unreviewed_crash(&dir, 0).unwrap();
    *h.state().pending_crash_review.lock().unwrap() = Some(crash);
}

fn pending(h: &Harness) -> bool {
    h.state().pending_crash_review.lock().unwrap().is_some()
}

#[test]
fn no_crash_means_nothing_to_review() {
    let h = harness();
    assert!(get_pending_crash_review(h.state()).is_none());
    dismiss_crash_review(h.handle()).unwrap();
    assert_eq!(h.prefs().view.last_reviewed_crash_unix, 0);
}

#[test]
fn a_staged_crash_is_shown_with_the_payload_that_would_be_sent() {
    let h = harness();
    stage_crash(&h, 700);
    let view = get_pending_crash_review(h.state()).unwrap();
    assert_eq!(view.timestamp, 700);
    assert_eq!(view.app_version, "1.2.3");
    assert_eq!(view.location, "src/main.rs:10");
    assert_eq!(view.message, "boom");
    assert!(view.payload_preview.contains("\"level\": \"fatal\""));
}

#[test]
fn a_report_that_does_not_validate_previews_as_a_dash() {
    let h = harness();
    stage_crash(&h, 701);
    h.state()
        .pending_crash_review
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .report
        .session_id = String::new();
    assert_eq!(
        get_pending_crash_review(h.state()).unwrap().payload_preview,
        "-"
    );
}

#[test]
fn dismissing_marks_the_crash_reviewed_without_sending() {
    let h = harness();
    stage_crash(&h, 702);
    dismiss_crash_review(h.handle()).unwrap();
    assert!(!pending(&h));
    assert_eq!(h.prefs().view.last_reviewed_crash_unix, 702);
}

#[test]
fn send_once_sends_then_marks_the_crash_reviewed() {
    let h = harness();
    stage_crash(&h, 703);
    send_crash_once(h.handle()).unwrap();
    assert!(!pending(&h));
    assert_eq!(h.prefs().view.last_reviewed_crash_unix, 703);
    send_crash_once(h.handle()).unwrap();
}

#[test]
fn always_send_turns_reporting_on_queues_the_crash_and_marks_it_reviewed() {
    let _turn = shared_lock();
    shared_queue_dir();
    let h = harness();
    stage_crash(&h, 704);
    always_send_crash(h.handle()).unwrap();
    assert_eq!(
        h.prefs().view.error_reporting_consent,
        ErrorReportingConsent::AlwaysSend
    );
    assert!(h.state().error_reporter.lock().unwrap().is_some());
    assert!(!pending(&h));
    assert_eq!(h.prefs().view.last_reviewed_crash_unix, 704);
    error_reporting::delete_all_queued();
}

#[test]
fn always_send_drops_a_report_that_does_not_validate() {
    let _turn = shared_lock();
    shared_queue_dir();
    let h = harness();
    stage_crash(&h, 705);
    h.state()
        .pending_crash_review
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .report
        .session_id = String::new();
    always_send_crash(h.handle()).unwrap();
    assert!(!pending(&h));
    error_reporting::delete_all_queued();
}

#[test]
fn always_send_without_a_crash_only_changes_the_consent() {
    let _turn = shared_lock();
    shared_queue_dir();
    let h = harness();
    always_send_crash(h.handle()).unwrap();
    assert_eq!(
        h.prefs().view.error_reporting_consent,
        ErrorReportingConsent::AlwaysSend
    );
    error_reporting::delete_all_queued();
}

#[test]
fn revoking_consent_removes_the_reporter_and_clears_the_queue() {
    let _turn = shared_lock();
    shared_queue_dir();
    let h = harness();
    set_error_reporting_consent(h.handle(), ErrorReportingConsent::AlwaysSend).unwrap();
    assert!(h.state().error_reporter.lock().unwrap().is_some());
    set_error_reporting_consent(h.handle(), ErrorReportingConsent::Disabled).unwrap();
    assert!(h.state().error_reporter.lock().unwrap().is_none());
    assert_eq!(get_queued_report_count(), 0);
    delete_queued_reports();
    assert_eq!(
        h.prefs().view.error_reporting_consent,
        ErrorReportingConsent::Disabled
    );
    let _ = Ordering::SeqCst;
}
