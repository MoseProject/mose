//! ER-01A/ER-01B port (mirrors `ui::App`'s `error_reporting.rs`/`crash_review.rs` orchestration
//! -- the schema/sanitization/validation itself already lives in `avcore` and is not duplicated
//! here, only reused). Consent stays two-state (`Disabled`/`AlwaysSend`, default `Disabled`):
//! with no reporter held, nothing is built, validated, queued, or sent -- the same "consent
//! disabled means nothing runs at all" guarantee the egui shell's own doc comment describes.
//!
//! Scope note: unlike the egui shell, this port does not retrofit every command's error path
//! through a central `report_error` call (ER-01A's per-call-site instrumentation) -- that is a
//! much larger, separate refactor across every `commands/*.rs` file. What *is* ported: the
//! post-crash "Send once / Always send / Do not send" review (the visible ER-01B feature) and
//! the consent-gated delivery infrastructure it needs.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use avcore::{
    Breadcrumb, ErrorCode, ErrorReporter, ErrorSeverity, Operation, QueueEnvelope, RecoveryOutcome,
    ReleaseMetadata,
};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

/// The persisted ER-01B consent decision -- separate from `telemetry_enabled` (local-only,
/// never sent). Shared `prefs.oc` key with the egui shell's `PrefsState.error_reporting_consent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorReportingConsent {
    #[default]
    Disabled,
    AlwaysSend,
}

static BUILDER: OnceLock<Mutex<avcore::ErrorReportBuilder>> = OnceLock::new();

fn default_builder() -> Mutex<avcore::ErrorReportBuilder> {
    Mutex::new(avcore::ErrorReportBuilder::new(ReleaseMetadata {
        release: format!("oca-{}", env!("CARGO_PKG_VERSION")),
        build_channel: if cfg!(debug_assertions) {
            "debug"
        } else {
            "stable"
        }
        .to_owned(),
        target_triple: option_env!("TARGET").unwrap_or("unknown").to_owned(),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        commit: option_env!("OCA_COMMIT_SHA")
            .unwrap_or("unknown")
            .to_owned(),
    }))
}

fn builder() -> &'static Mutex<avcore::ErrorReportBuilder> {
    BUILDER.get_or_init(default_builder)
}

/// Seeds the process-wide report builder with this launch's locale and current consent, then
/// returns the reporter this launch reports through -- `None` when `consent` is
/// [`ErrorReportingConsent::Disabled`].
pub fn seed_error_reporting(
    locale_code: &str,
    consent: ErrorReportingConsent,
) -> Option<Arc<dyn ErrorReporter>> {
    let mut b = builder().lock().unwrap_or_else(|p| p.into_inner());
    b.set_locale(locale_code);
    if b.breadcrumbs().is_empty() {
        if let Some(crumb) = Breadcrumb::state_transition("app_started") {
            b.add_breadcrumb(crumb);
        }
    }
    drop(b);
    match consent {
        ErrorReportingConsent::Disabled => None,
        ErrorReportingConsent::AlwaysSend => Some(reporter_handle()),
    }
}

/// Applies a new consent decision: live-swaps the held reporter and, on revocation, deletes
/// every queued-but-undelivered report -- the ER-01 doc's "revocation deletes unsent remote
/// reports" requirement. Returns the new reporter to store on `AppState`.
pub fn apply_consent_change(consent: ErrorReportingConsent) -> Option<Arc<dyn ErrorReporter>> {
    match consent {
        ErrorReportingConsent::Disabled => {
            delete_all_queued();
            None
        }
        ErrorReportingConsent::AlwaysSend => Some(reporter_handle()),
    }
}

// ---------------------------------------------------------------------------
// bounded on-disk queue
// ---------------------------------------------------------------------------

/// Overrides the queue directory (tests point it at a temporary folder).
const QUEUE_DIR_ENV_VAR: &str = "OCA_ERROR_QUEUE_DIR";

fn queue_dir() -> PathBuf {
    match std::env::var_os(QUEUE_DIR_ENV_VAR) {
        Some(dir) => PathBuf::from(dir),
        None => crate::platform_log_dir().join("error_reports"),
    }
}

fn envelope_path(dir: &std::path::Path, event_id: &str) -> PathBuf {
    dir.join(format!("{event_id}.json"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn enqueue_to_disk_in(dir: &std::path::Path, envelope: &QueueEnvelope) -> Option<PathBuf> {
    if std::fs::create_dir_all(dir).is_err() {
        return None;
    }
    write_envelope_atomic(dir, envelope)?;
    prune_queue_dir(dir);
    Some(envelope_path(dir, &envelope.event_id))
}

fn write_envelope_atomic(dir: &std::path::Path, envelope: &QueueEnvelope) -> Option<PathBuf> {
    let bytes = serde_json::to_vec(envelope).ok()?;
    let final_path = envelope_path(dir, &envelope.event_id);
    let tmp_path = dir.join(format!("{}.tmp", envelope.event_id));
    std::fs::write(&tmp_path, &bytes).ok()?;
    std::fs::rename(&tmp_path, &final_path).ok()?;
    Some(final_path)
}

pub fn load_queue() -> Vec<QueueEnvelope> {
    load_queue_in(&queue_dir())
}

fn load_queue_in(dir: &std::path::Path) -> Vec<QueueEnvelope> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let now = now_unix();
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        match serde_json::from_slice::<QueueEnvelope>(&bytes) {
            Ok(envelope) if avcore::validate_envelope(&envelope, now).is_ok() => {
                out.push(envelope);
            }
            _ => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    out.sort_by_key(|e| e.queued_at_unix);
    out
}

/// Number of currently-queued reports, for the Preferences screen's status line.
pub fn queue_len() -> usize {
    load_queue().len()
}

/// Deletes every queued envelope -- consent revocation, or the Preferences screen's explicit
/// "delete pending reports" control.
pub fn delete_all_queued() {
    delete_all_queued_in(&queue_dir());
}

fn delete_all_queued_in(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let _ = std::fs::remove_file(entry.path());
    }
}

fn delete_envelope_in(dir: &std::path::Path, event_id: &str) {
    let _ = std::fs::remove_file(envelope_path(dir, event_id));
}

fn prune_queue_dir(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = now_unix();
    let mut records: Vec<(PathBuf, QueueEnvelope, u64)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let len = bytes.len() as u64;
        match serde_json::from_slice::<QueueEnvelope>(&bytes) {
            Ok(envelope) => {
                if envelope.is_expired(now) {
                    let _ = std::fs::remove_file(&path);
                } else {
                    records.push((path, envelope, len));
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(&path);
            }
        }
    }
    records.sort_by_key(|(_, envelope, _)| envelope.queued_at_unix);
    let mut total_bytes: u64 = records.iter().map(|(_, _, len)| *len).sum();
    let mut count = records.len();
    let mut i = 0;
    while (count > avcore::QUEUE_MAX_RECORDS || total_bytes > avcore::QUEUE_MAX_BYTES)
        && i < records.len()
    {
        let (path, _, len) = &records[i];
        if std::fs::remove_file(path).is_ok() {
            total_bytes = total_bytes.saturating_sub(*len);
            count -= 1;
        }
        i += 1;
    }
}

// ---------------------------------------------------------------------------
// delivery transport
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeliveryOutcome {
    Delivered,
    TransientFailure,
    PermanentFailure,
    NotConfigured,
}

trait EnvelopeSender: Send + Sync {
    fn send(&self, envelope: &QueueEnvelope) -> DeliveryOutcome;
}

struct SentryEnvelopeSender {
    ingest_url: Option<String>,
    public_key: Option<String>,
}

const DSN_ENV_VAR: &str = "OCA_SENTRY_DSN";

impl SentryEnvelopeSender {
    fn from_env() -> Self {
        match std::env::var(DSN_ENV_VAR)
            .ok()
            .and_then(|dsn| parse_dsn(&dsn))
        {
            Some((public_key, ingest_url)) => Self {
                ingest_url: Some(ingest_url),
                public_key: Some(public_key),
            },
            None => Self {
                ingest_url: None,
                public_key: None,
            },
        }
    }
}

impl EnvelopeSender for SentryEnvelopeSender {
    fn send(&self, envelope: &QueueEnvelope) -> DeliveryOutcome {
        let (Some(ingest_url), Some(public_key)) = (&self.ingest_url, &self.public_key) else {
            return DeliveryOutcome::NotConfigured;
        };
        let body = build_sentry_envelope(public_key, envelope);
        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(10)))
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build();
        let result = ureq::Agent::new_with_config(config)
            .post(ingest_url)
            .header("Content-Type", "application/x-sentry-envelope")
            .send(&body[..]);
        match result {
            Ok(response) => {
                let status = response.status().as_u16();
                if (200..300).contains(&status) {
                    DeliveryOutcome::Delivered
                } else if status == 429 || (500..600).contains(&status) {
                    DeliveryOutcome::TransientFailure
                } else {
                    DeliveryOutcome::PermanentFailure
                }
            }
            Err(_) => DeliveryOutcome::TransientFailure,
        }
    }
}

fn parse_dsn(dsn: &str) -> Option<(String, String)> {
    let rest = dsn.strip_prefix("https://")?;
    let (key, rest) = rest.split_once('@')?;
    if key.is_empty() || rest.is_empty() {
        return None;
    }
    let (host_and_prefix, project_id) = rest.rsplit_once('/')?;
    if host_and_prefix.is_empty()
        || project_id.is_empty()
        || !project_id.chars().all(|c| c.is_ascii_digit())
    {
        return None;
    }
    Some((
        key.to_owned(),
        format!("https://{host_and_prefix}/api/{project_id}/envelope/"),
    ))
}

fn error_code_str(report: &avcore::ErrorReport) -> String {
    serde_json::to_value(report.error_code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn operation_str(report: &avcore::ErrorReport) -> String {
    serde_json::to_value(report.operation)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned())
}

fn severity_level(severity: ErrorSeverity) -> &'static str {
    match severity {
        ErrorSeverity::Warning => "warning",
        ErrorSeverity::Error => "error",
        ErrorSeverity::Fatal => "fatal",
    }
}

/// The Sentry event body alone -- what [`crash_review_payload_preview`] shows the user
/// pretty-printed before a one-time send.
pub fn sentry_event_payload(
    report: &avcore::ErrorReport,
    queued_at_unix: u64,
) -> serde_json::Value {
    let sentry_event_id = format!("{0}{0}", report.event_id);
    serde_json::json!({
        "event_id": sentry_event_id,
        "timestamp": queued_at_unix,
        "platform": "other",
        "release": report.release,
        "environment": report.build_channel,
        "level": severity_level(report.severity),
        "message": { "formatted": format!("{} during {}", error_code_str(report), operation_str(report)) },
        "tags": {
            "error_code": error_code_str(report),
            "operation": operation_str(report),
            "recovery_outcome": serde_json::to_value(report.recovery_outcome).ok(),
            "os": report.os,
            "arch": report.arch,
            "target_triple": report.target_triple,
            "portable": report.portable,
            "retried": report.retried,
        },
        "extra": {
            "oca_event_id": report.event_id,
            "session_id": report.session_id,
            "commit": report.commit,
            "breadcrumb_count": report.breadcrumbs.len(),
            "stack_trace": report.sanitized_stack_trace,
        },
        "contexts": {
            "os": { "name": report.os, "version": report.os_version },
            "runtime": { "name": "oca", "version": report.app_version },
        },
    })
}

fn build_sentry_envelope(public_key: &str, envelope: &QueueEnvelope) -> Vec<u8> {
    let report = &envelope.report;
    let sentry_event_id = format!("{0}{0}", report.event_id);
    let header = serde_json::json!({
        "event_id": sentry_event_id,
        "sent_at": envelope.queued_at_unix,
        "dsn_key": public_key,
    });
    let payload = sentry_event_payload(report, envelope.queued_at_unix);
    let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let item_header = serde_json::json!({ "type": "event", "length": payload_bytes.len() });
    let mut out = Vec::new();
    out.extend_from_slice(header.to_string().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(item_header.to_string().as_bytes());
    out.push(b'\n');
    out.extend_from_slice(&payload_bytes);
    out.push(b'\n');
    out
}

// ---------------------------------------------------------------------------
// background delivery worker
// ---------------------------------------------------------------------------

const INLINE_RETRY_BACKOFFS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

fn deliver_with_retry(sender: &dyn EnvelopeSender, envelope: &QueueEnvelope) -> bool {
    deliver_with_backoffs(sender, envelope, &INLINE_RETRY_BACKOFFS)
}

/// [`deliver_with_retry`] with explicit waits between attempts.
fn deliver_with_backoffs(
    sender: &dyn EnvelopeSender,
    envelope: &QueueEnvelope,
    backoffs: &[Duration],
) -> bool {
    match sender.send(envelope) {
        DeliveryOutcome::Delivered | DeliveryOutcome::PermanentFailure => return true,
        DeliveryOutcome::NotConfigured => return false,
        DeliveryOutcome::TransientFailure => {}
    }
    for backoff in backoffs.iter().copied() {
        std::thread::sleep(backoff);
        match sender.send(envelope) {
            DeliveryOutcome::Delivered | DeliveryOutcome::PermanentFailure => return true,
            DeliveryOutcome::NotConfigured => return false,
            DeliveryOutcome::TransientFailure => continue,
        }
    }
    false
}

fn spawn_delivery_worker(
    rx: UnboundedReceiver<avcore::ErrorReport>,
    sender: Arc<dyn EnvelopeSender>,
) {
    let dir = queue_dir();
    std::thread::spawn(move || {
        for envelope in load_queue_in(&dir) {
            if deliver_with_retry(sender.as_ref(), &envelope) {
                delete_envelope_in(&dir, &envelope.event_id);
            }
        }
        let mut rx = rx;
        while let Some(report) = rx.blocking_recv() {
            let envelope = QueueEnvelope::new(report, now_unix());
            if enqueue_to_disk_in(&dir, &envelope).is_none() {
                tracing::warn!("failed to persist queued error report");
                continue;
            }
            if deliver_with_retry(sender.as_ref(), &envelope) {
                delete_envelope_in(&dir, &envelope.event_id);
            }
        }
    });
}

static WORKER_TX: OnceLock<UnboundedSender<avcore::ErrorReport>> = OnceLock::new();

fn ensure_worker_spawned() -> UnboundedSender<avcore::ErrorReport> {
    WORKER_TX
        .get_or_init(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            spawn_delivery_worker(rx, Arc::new(SentryEnvelopeSender::from_env()));
            tx
        })
        .clone()
}

struct SentryReporter {
    tx: UnboundedSender<avcore::ErrorReport>,
}

impl ErrorReporter for SentryReporter {
    fn report(&self, report: avcore::ErrorReport) {
        let _ = self.tx.send(report);
    }
}

fn reporter_handle() -> Arc<dyn ErrorReporter> {
    Arc::new(SentryReporter {
        tx: ensure_worker_spawned(),
    })
}

/// The reporter a crash review's "Send once" hands its one-off report to -- reachable
/// regardless of the persisted consent.
pub fn one_shot_reporter() -> Arc<dyn ErrorReporter> {
    reporter_handle()
}

// ---------------------------------------------------------------------------
// post-crash review
// ---------------------------------------------------------------------------

/// One crash file staged for the user's post-crash review.
#[derive(Debug, Clone)]
pub struct PendingCrashReview {
    pub timestamp: u64,
    pub app_version: String,
    pub location: String,
    pub message: String,
    /// Kept for parity with the egui shell's struct shape; already folded into `report`'s
    /// `sanitized_stack_trace` by [`PendingCrashReview::new`], so no command currently reads it
    /// back out separately.
    #[allow(dead_code)]
    pub backtrace: String,
    pub report: avcore::ErrorReport,
}

impl PendingCrashReview {
    fn new(
        timestamp: u64,
        app_version: String,
        location: String,
        message: String,
        backtrace: String,
    ) -> Self {
        let crash_stack = format!("panic at {location}: {message}\n\n{backtrace}");
        let report = build_crash_report(&crash_stack, &app_version);
        Self {
            timestamp,
            app_version,
            location,
            message,
            backtrace,
            report,
        }
    }
}

/// Scans `log_dir` for `crash_<unix>.txt` files newer than `after_unix` and returns the most
/// recent, parsed. `None` is the common case.
pub fn find_latest_unreviewed_crash(
    log_dir: &std::path::Path,
    after_unix: u64,
) -> Option<PendingCrashReview> {
    let entries = std::fs::read_dir(log_dir).ok()?;
    let mut candidates: Vec<(u64, PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(ts_str) = stem.strip_prefix("crash_") else {
            continue;
        };
        let Ok(timestamp) = ts_str.parse::<u64>() else {
            continue;
        };
        if timestamp > after_unix {
            candidates.push((timestamp, path));
        }
    }
    candidates.sort_by_key(|(ts, _)| *ts);
    let (timestamp, path) = candidates.pop()?;
    let body = std::fs::read_to_string(&path).ok()?;
    let (app_version, location, message, backtrace) = parse_crash_report(&body)?;
    Some(PendingCrashReview::new(
        timestamp,
        app_version,
        location,
        message,
        backtrace,
    ))
}

fn parse_crash_report(body: &str) -> Option<(String, String, String, String)> {
    let app_version = body
        .lines()
        .next()?
        .strip_prefix("oca v")?
        .strip_suffix(" crash report")?
        .to_owned();
    let location = body.lines().find_map(|l| l.strip_prefix("location: "))?;
    let message = body.lines().find_map(|l| l.strip_prefix("message: "))?;
    let backtrace = body
        .split_once("backtrace:\n")
        .map(|(_, rest)| rest.trim_end().to_owned())
        .unwrap_or_default();
    Some((
        app_version,
        location.to_owned(),
        message.to_owned(),
        backtrace,
    ))
}

fn build_crash_report(crash_stack: &str, crashed_app_version: &str) -> avcore::ErrorReport {
    let mut report = builder()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .build_report(
            ErrorCode::Panic,
            ErrorSeverity::Fatal,
            Operation::App,
            RecoveryOutcome::Aborted,
            false,
            Some(crash_stack),
        );
    report.release = format!("oca-{crashed_app_version}");
    report.app_version = crashed_app_version.to_owned();
    report
}

/// The exact post-sanitization JSON "Send once"/"Always send" would transmit.
pub fn crash_review_payload_preview(crash: &PendingCrashReview) -> Option<String> {
    avcore::validate_report(&crash.report).ok()?;
    serde_json::to_string_pretty(&sentry_event_payload(&crash.report, crash.timestamp)).ok()
}

/// "Send once": sends regardless of the persisted consent, one explicit user action.
pub fn send_crash_once(crash: &PendingCrashReview) {
    match avcore::validate_report(&crash.report) {
        Ok(()) => one_shot_reporter().report(crash.report.clone()),
        Err(e) => tracing::warn!(error = %e, "dropping invalid crash report"),
    }
}

#[cfg(test)]
#[path = "error_reporting/error_reporting_test.rs"]
pub(crate) mod tests;
