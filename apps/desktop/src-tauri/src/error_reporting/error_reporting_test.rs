// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use std::collections::VecDeque;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;

/// One temporary queue directory shared by every test that goes through `queue_dir()`: the
/// override is a process-wide environment variable, so it is set once and never changed.
pub(crate) fn shared_queue_dir() -> &'static PathBuf {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir =
            std::env::temp_dir().join(format!("oca_desktop_error_queue_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var(QUEUE_DIR_ENV_VAR, &dir);
        dir
    })
}

/// The shared queue directory is process-wide, so the tests that use it take turns.
pub(crate) fn shared_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poison| poison.into_inner())
}

pub(crate) fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oca_desktop_er_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn report() -> avcore::ErrorReport {
    let mut builder = default_builder().into_inner().unwrap();
    builder.set_locale("pt-BR");
    builder.build_report(
        ErrorCode::ExportEncode,
        ErrorSeverity::Error,
        Operation::Export,
        RecoveryOutcome::Aborted,
        false,
        Some("frame 12 failed"),
    )
}

fn envelope_at(queued_at: u64) -> QueueEnvelope {
    QueueEnvelope::new(report(), queued_at)
}

/// A sender that answers from a script and counts its calls.
struct ScriptedSender {
    script: Mutex<VecDeque<DeliveryOutcome>>,
    calls: AtomicUsize,
}

impl ScriptedSender {
    fn new(outcomes: &[DeliveryOutcome]) -> Self {
        Self {
            script: Mutex::new(outcomes.iter().copied().collect()),
            calls: AtomicUsize::new(0),
        }
    }
}

impl EnvelopeSender for ScriptedSender {
    fn send(&self, _envelope: &QueueEnvelope) -> DeliveryOutcome {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or(DeliveryOutcome::TransientFailure)
    }
}

/// Answers the next request on a loopback port with `status` and returns the ingest URL.
fn serve_once(status: u16) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/api/1/envelope/", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        // Read up to the end of the headers, answer, then drain the rest of the upload so the
        // client never sees the connection reset while it is still sending.
        let mut request = Vec::new();
        let mut chunk = [0u8; 4096];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = stream.read(&mut chunk).unwrap_or(0);
            if read == 0 {
                break;
            }
            request.extend_from_slice(&chunk[..read]);
        }
        let response =
            format!("HTTP/1.1 {status} X\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = stream.write_all(response.as_bytes());
        let _ = stream.shutdown(std::net::Shutdown::Write);
        while stream.read(&mut chunk).unwrap_or(0) > 0 {}
    });
    url
}

#[test]
fn consent_defaults_to_disabled_and_serializes_in_snake_case() {
    assert_eq!(
        ErrorReportingConsent::default(),
        ErrorReportingConsent::Disabled
    );
    assert_eq!(
        serde_json::to_string(&ErrorReportingConsent::AlwaysSend).unwrap(),
        "\"always_send\""
    );
    let back: ErrorReportingConsent = serde_json::from_str("\"disabled\"").unwrap();
    assert_eq!(back, ErrorReportingConsent::Disabled);
}

#[test]
fn consent_decides_whether_a_reporter_exists_and_revoking_clears_the_queue() {
    let _turn = shared_lock();
    let dir = shared_queue_dir();
    assert!(seed_error_reporting("en", ErrorReportingConsent::Disabled).is_none());
    assert!(seed_error_reporting("pt-BR", ErrorReportingConsent::AlwaysSend).is_some());
    assert!(apply_consent_change(ErrorReportingConsent::AlwaysSend).is_some());
    one_shot_reporter().report(report());

    // Revoking consent deletes whatever was queued.
    std::fs::write(dir.join("pending.json"), b"{}").unwrap();
    assert!(apply_consent_change(ErrorReportingConsent::Disabled).is_none());
    assert!(!dir.join("pending.json").exists());
}

#[test]
fn the_queue_stores_loads_orders_and_deletes_envelopes() {
    let dir = temp_dir("queue");
    let now = now_unix();
    let older = envelope_at(now - 100);
    let newer = envelope_at(now - 10);
    assert!(enqueue_to_disk_in(&dir.join("created-on-demand"), &newer).is_some());
    assert!(enqueue_to_disk_in(&dir, &newer).is_some());
    assert!(enqueue_to_disk_in(&dir, &older).is_some());
    // Other files and unreadable or invalid records are ignored or dropped while loading.
    std::fs::write(dir.join("notes.txt"), b"ignore me").unwrap();
    std::fs::write(dir.join("broken.json"), b"not json").unwrap();

    let loaded = load_queue_in(&dir);
    assert_eq!(loaded.len(), 2);
    assert!(loaded[0].queued_at_unix <= loaded[1].queued_at_unix);
    assert!(!dir.join("broken.json").exists());
    assert!(dir.join("notes.txt").exists());

    delete_envelope_in(&dir, &older.event_id);
    assert_eq!(load_queue_in(&dir).len(), 1);
    delete_all_queued_in(&dir);
    assert!(load_queue_in(&dir).is_empty());
    // A directory that does not exist is an empty queue, not an error.
    assert!(load_queue_in(&dir.join("missing")).is_empty());
    delete_all_queued_in(&dir.join("missing"));
    prune_queue_dir(&dir.join("missing"));
}

#[test]
fn queue_paths_follow_the_override() {
    let _turn = shared_lock();
    let shared = shared_queue_dir();
    assert_eq!(queue_dir(), *shared);
    std::fs::write(shared.join("probe.json"), b"{}").unwrap();
    delete_all_queued();
    // The background delivery worker may queue its own reports at the same time, so only the
    // file written here is checked.
    assert!(!shared.join("probe.json").exists());
    let _ = (queue_len(), load_queue());
    assert_eq!(envelope_path(shared, "abc"), shared.join("abc.json"));
}

#[test]
fn a_write_into_a_missing_folder_is_refused() {
    let dir = temp_dir("blocked");
    // A file where the directory should be makes the folder impossible to create.
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"x").unwrap();
    assert!(enqueue_to_disk_in(&blocker.join("child"), &envelope_at(now_unix())).is_none());
    assert!(write_envelope_atomic(&dir.join("absent"), &envelope_at(now_unix())).is_none());
}

#[test]
fn pruning_drops_expired_and_invalid_records_and_enforces_the_record_limit() {
    let dir = temp_dir("prune");
    let now = now_unix();
    // Expired a week ago, and one that cannot be parsed.
    write_envelope_atomic(&dir, &envelope_at(now - avcore::QUEUE_RETENTION_SECS - 100)).unwrap();
    std::fs::write(dir.join("garbage.json"), b"nope").unwrap();
    std::fs::write(dir.join("keep.txt"), b"not a record").unwrap();
    // More fresh records than the queue allows: the oldest go first.
    let mut ids = Vec::new();
    for index in 0..(avcore::QUEUE_MAX_RECORDS + 3) {
        let envelope = envelope_at(now - 1_000 + index as u64);
        write_envelope_atomic(&dir, &envelope).unwrap();
        ids.push(envelope.event_id);
    }
    prune_queue_dir(&dir);
    let remaining = load_queue_in(&dir);
    assert_eq!(remaining.len(), avcore::QUEUE_MAX_RECORDS);
    assert!(!dir.join("garbage.json").exists());
    assert!(dir.join("keep.txt").exists());
    // The three oldest of the fresh records are the ones removed.
    for old in &ids[..3] {
        assert!(!envelope_path(&dir, old).exists());
    }
}

#[test]
fn dsn_parsing_derives_the_ingest_url_and_rejects_malformed_values() {
    assert_eq!(
        parse_dsn("https://abc123@o1.ingest.example.io/4567"),
        Some((
            "abc123".to_owned(),
            "https://o1.ingest.example.io/api/4567/envelope/".to_owned()
        ))
    );
    for bad in [
        "http://abc@host/1",
        "https://host/1",
        "https://@host/1",
        "https://abc@/1",
        "https://abc@host",
        "https://abc@host/",
        "https://abc@host/notanumber",
        "https://abc@/12",
    ] {
        assert_eq!(parse_dsn(bad), None, "{bad}");
    }
}

#[test]
fn the_sentry_payload_and_envelope_carry_the_report_fields() {
    let envelope = envelope_at(1_752_000_000);
    let payload = sentry_event_payload(&envelope.report, 1_752_000_000);
    assert_eq!(payload["level"], "error");
    assert_eq!(payload["timestamp"], 1_752_000_000u64);
    assert_eq!(payload["tags"]["operation"], "export");
    assert!(payload["message"]["formatted"]
        .as_str()
        .unwrap()
        .contains("during"));
    assert_eq!(payload["extra"]["oca_event_id"], envelope.report.event_id);

    let body = build_sentry_envelope("publickey", &envelope);
    let text = String::from_utf8(body).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 3);
    let header: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(header["dsn_key"], "publickey");
    let item: serde_json::Value = serde_json::from_str(lines[1]).unwrap();
    assert_eq!(item["type"], "event");
    assert_eq!(item["length"], lines[2].len());
}

#[test]
fn levels_codes_and_operations_have_stable_names() {
    assert_eq!(severity_level(ErrorSeverity::Warning), "warning");
    assert_eq!(severity_level(ErrorSeverity::Error), "error");
    assert_eq!(severity_level(ErrorSeverity::Fatal), "fatal");
    let report = report();
    assert_eq!(error_code_str(&report), "export_encode");
    assert_eq!(operation_str(&report), "export");
}

#[test]
fn delivery_retries_transient_failures_and_stops_on_a_final_answer() {
    let envelope = envelope_at(now_unix());
    let none: &[Duration] = &[Duration::ZERO, Duration::ZERO, Duration::ZERO];

    let delivered = ScriptedSender::new(&[DeliveryOutcome::Delivered]);
    assert!(deliver_with_backoffs(&delivered, &envelope, none));
    assert_eq!(delivered.calls.load(Ordering::SeqCst), 1);

    // A permanent failure is final: it is not retried but the record is dropped (true).
    let permanent = ScriptedSender::new(&[DeliveryOutcome::PermanentFailure]);
    assert!(deliver_with_backoffs(&permanent, &envelope, none));

    let unconfigured = ScriptedSender::new(&[DeliveryOutcome::NotConfigured]);
    assert!(!deliver_with_backoffs(&unconfigured, &envelope, none));

    let recovers = ScriptedSender::new(&[
        DeliveryOutcome::TransientFailure,
        DeliveryOutcome::TransientFailure,
        DeliveryOutcome::Delivered,
    ]);
    assert!(deliver_with_backoffs(&recovers, &envelope, none));
    assert_eq!(recovers.calls.load(Ordering::SeqCst), 3);

    let retry_then_permanent = ScriptedSender::new(&[
        DeliveryOutcome::TransientFailure,
        DeliveryOutcome::PermanentFailure,
    ]);
    assert!(deliver_with_backoffs(
        &retry_then_permanent,
        &envelope,
        none
    ));
    let retry_then_unconfigured = ScriptedSender::new(&[
        DeliveryOutcome::TransientFailure,
        DeliveryOutcome::NotConfigured,
    ]);
    assert!(!deliver_with_backoffs(
        &retry_then_unconfigured,
        &envelope,
        none
    ));

    // Out of retries: the record stays queued (false).
    let always_down = ScriptedSender::new(&[]);
    assert!(!deliver_with_backoffs(&always_down, &envelope, none));
    assert_eq!(always_down.calls.load(Ordering::SeqCst), 1 + none.len());
}

#[test]
fn the_default_retry_schedule_answers_at_once_for_final_outcomes() {
    let envelope = envelope_at(now_unix());
    let unconfigured = ScriptedSender::new(&[DeliveryOutcome::NotConfigured]);
    assert!(!deliver_with_retry(&unconfigured, &envelope));
    let delivered = ScriptedSender::new(&[DeliveryOutcome::Delivered]);
    assert!(deliver_with_retry(&delivered, &envelope));
}

#[test]
fn the_sentry_sender_maps_http_statuses_to_outcomes() {
    let envelope = envelope_at(now_unix());
    let sender = |url: Option<String>| SentryEnvelopeSender {
        ingest_url: url,
        public_key: Some("key".to_owned()),
    };
    assert_eq!(sender(None).send(&envelope), DeliveryOutcome::NotConfigured);
    for (status, expected) in [
        (200, DeliveryOutcome::Delivered),
        (429, DeliveryOutcome::TransientFailure),
        (503, DeliveryOutcome::TransientFailure),
        (400, DeliveryOutcome::PermanentFailure),
    ] {
        let outcome = sender(Some(serve_once(status))).send(&envelope);
        assert_eq!(outcome, expected, "status {status}");
    }
    // Nothing listening: the connection fails, which is worth retrying.
    let closed = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/", closed.local_addr().unwrap());
    drop(closed);
    assert_eq!(
        sender(Some(url)).send(&envelope),
        DeliveryOutcome::TransientFailure
    );
}

#[test]
fn the_sender_reads_its_dsn_from_the_environment() {
    // Only one test touches this variable.
    std::env::remove_var(DSN_ENV_VAR);
    let unset = SentryEnvelopeSender::from_env();
    assert!(unset.ingest_url.is_none() && unset.public_key.is_none());
    std::env::set_var(DSN_ENV_VAR, "https://key@host.example/12");
    let configured = SentryEnvelopeSender::from_env();
    assert_eq!(configured.public_key.as_deref(), Some("key"));
    assert_eq!(
        configured.ingest_url.as_deref(),
        Some("https://host.example/api/12/envelope/")
    );
    std::env::set_var(DSN_ENV_VAR, "not a dsn");
    assert!(SentryEnvelopeSender::from_env().ingest_url.is_none());
    std::env::remove_var(DSN_ENV_VAR);
}

/// A sender that cannot deliver the one report it is told to keep and delivers the rest.
struct KeepOneSender {
    keep: String,
    seen: Mutex<Vec<String>>,
}

impl EnvelopeSender for KeepOneSender {
    fn send(&self, envelope: &QueueEnvelope) -> DeliveryOutcome {
        self.seen.lock().unwrap().push(envelope.event_id.clone());
        if envelope.event_id == self.keep {
            DeliveryOutcome::NotConfigured
        } else {
            DeliveryOutcome::Delivered
        }
    }
}

#[test]
fn the_delivery_worker_delivers_the_queue_then_new_reports_and_keeps_failures() {
    let _turn = shared_lock();
    let dir = shared_queue_dir();
    delete_all_queued_in(dir);
    // A record left over from an earlier run is delivered first and removed.
    let leftover = envelope_at(now_unix());
    write_envelope_atomic(dir, &leftover).unwrap();

    let delivered_report = report();
    let kept_report = report();
    let (delivered_id, kept_id) = (
        delivered_report.event_id.clone(),
        kept_report.event_id.clone(),
    );
    let sender = Arc::new(KeepOneSender {
        keep: kept_id.clone(),
        seen: Mutex::new(Vec::new()),
    });
    let (tx, rx) = mpsc::unbounded_channel();
    spawn_delivery_worker(rx, sender.clone());
    tx.send(delivered_report).unwrap();
    tx.send(kept_report).unwrap();
    drop(tx);

    // Wait for the worker to handle both new reports.
    for _ in 0..200 {
        let seen = sender.seen.lock().unwrap();
        if seen.contains(&delivered_id) && seen.contains(&kept_id) {
            break;
        }
        drop(seen);
        std::thread::sleep(Duration::from_millis(25));
    }
    std::thread::sleep(Duration::from_millis(100));
    assert!(!envelope_path(dir, &leftover.event_id).exists());
    assert!(!envelope_path(dir, &delivered_id).exists());
    // The report the sender could not deliver stays on disk for the next launch.
    assert!(envelope_path(dir, &kept_id).exists());
    delete_all_queued_in(dir);
}

pub(crate) fn crash_file(dir: &Path, timestamp: u64, body: &str) {
    std::fs::write(dir.join(format!("crash_{timestamp}.txt")), body).unwrap();
}

pub(crate) const CRASH_BODY: &str =
    "oca v1.2.3 crash report\nlocation: src/main.rs:10\nmessage: boom\nbacktrace:\nframe 1\nframe 2\n";

#[test]
fn the_latest_unreviewed_crash_is_found_and_parsed() {
    let dir = temp_dir("crash-select");
    crash_file(&dir, 100, CRASH_BODY);
    crash_file(&dir, 300, &CRASH_BODY.replace("boom", "latest"));
    std::fs::write(dir.join("crash_notanumber.txt"), "x").unwrap();
    std::fs::write(dir.join("other.txt"), "x").unwrap();
    assert!(find_latest_unreviewed_crash(&dir.join("missing"), 0).is_none());
    // Everything is older than the last reviewed crash.
    assert!(find_latest_unreviewed_crash(&dir, 300).is_none());

    let crash = find_latest_unreviewed_crash(&dir, 150).unwrap();
    assert_eq!(crash.timestamp, 300);
    assert_eq!(crash.app_version, "1.2.3");
    assert_eq!(crash.location, "src/main.rs:10");
    assert_eq!(crash.message, "latest");
    assert_eq!(crash.backtrace, "frame 1\nframe 2");
    assert_eq!(crash.report.release, "oca-1.2.3");
    assert_eq!(crash.report.app_version, "1.2.3");

    // A newest file that cannot be parsed hides the older ones: nothing is shown.
    crash_file(&dir, 400, "malformed body");
    assert!(find_latest_unreviewed_crash(&dir, 150).is_none());
}

#[test]
fn crash_reports_must_have_the_expected_shape() {
    let parsed = parse_crash_report(CRASH_BODY).unwrap();
    assert_eq!(parsed.0, "1.2.3");
    // The backtrace is optional.
    let without = parse_crash_report("oca v1.0 crash report\nlocation: a\nmessage: b\n").unwrap();
    assert_eq!(without.3, "");
    for bad in [
        "",
        "not oca v1 crash report\nlocation: a\nmessage: b\n",
        "oca v1 crash report\nmessage: b\n",
        "oca v1 crash report\nlocation: a\n",
        "oca v1.0 crash\nlocation: a\nmessage: b\n",
    ] {
        assert!(parse_crash_report(bad).is_none(), "{bad:?}");
    }
}

#[test]
fn a_crash_review_previews_and_sends_its_report() {
    let _turn = shared_lock();
    shared_queue_dir();
    let dir = temp_dir("crash-send");
    crash_file(&dir, 500, CRASH_BODY);
    let crash = find_latest_unreviewed_crash(&dir, 0).unwrap();

    let preview = crash_review_payload_preview(&crash).expect("a valid report previews");
    assert!(preview.contains("\"level\": \"fatal\""));
    send_crash_once(&crash);

    // An invalid report is neither previewed nor sent.
    let mut broken = crash.clone();
    broken.report.session_id = String::new();
    assert!(crash_review_payload_preview(&broken).is_none());
    send_crash_once(&broken);
}

#[test]
fn the_report_builder_is_shared_and_seeded_with_a_start_breadcrumb() {
    let first = builder() as *const _;
    let second = builder() as *const _;
    assert_eq!(first, second);
    seed_error_reporting("en", ErrorReportingConsent::Disabled);
    seed_error_reporting("pt-BR", ErrorReportingConsent::Disabled);
    let guard = builder().lock().unwrap_or_else(|p| p.into_inner());
    assert!(!guard.breadcrumbs().is_empty());
}
