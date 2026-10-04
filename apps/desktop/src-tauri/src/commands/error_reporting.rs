//! ER-01B post-crash review + consent commands -- the command surface over
//! `crate::error_reporting`'s ported orchestration. See that module's own doc comment for what
//! is and isn't ported relative to the egui shell.

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};

use crate::error_reporting::{self, ErrorReportingConsent};
use crate::state::AppState;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashReviewView {
    pub timestamp: u64,
    pub app_version: String,
    pub location: String,
    pub message: String,
    /// The exact post-sanitization JSON "Send once"/"Always send" would transmit, pretty-printed
    /// -- the ER-01 doc's required pre-send payload preview.
    pub payload_preview: String,
}

/// The previous launch's crash staged for review, if any -- what the startup review modal reads
/// once. `None` once reviewed (any of the three actions below clears it).
#[tauri::command]
pub fn get_pending_crash_review(state: State<'_, AppState>) -> Option<CrashReviewView> {
    let guard = state
        .pending_crash_review
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let crash = guard.as_ref()?;
    Some(CrashReviewView {
        timestamp: crash.timestamp,
        app_version: crash.app_version.clone(),
        location: crash.location.clone(),
        message: crash.message.clone(),
        payload_preview: error_reporting::crash_review_payload_preview(crash)
            .unwrap_or_else(|| "-".to_string()),
    })
}

/// "Send once": sends regardless of the persisted consent, then marks the crash reviewed.
#[tauri::command]
pub fn send_crash_once<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<AppState>();
    let crash = state
        .pending_crash_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let Some(crash) = crash {
        error_reporting::send_crash_once(&crash);
    }
    finish_crash_review(&state)
}

/// "Always send": opts into steady-state consent, sends the pending crash through that reporter,
/// then marks it reviewed.
#[tauri::command]
pub fn always_send_crash<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    let state = app.state::<AppState>();
    set_consent(&state, ErrorReportingConsent::AlwaysSend)?;
    let crash = state
        .pending_crash_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    let reporter = state
        .error_reporter
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone();
    if let (Some(crash), Some(reporter)) = (crash, reporter) {
        match avcore::validate_report(&crash.report) {
            Ok(()) => reporter.report(crash.report),
            Err(e) => tracing::warn!(error = %e, "dropping invalid crash report"),
        }
    }
    finish_crash_review(&state)
}

/// "Do not send": dismisses the review without transmitting anything.
#[tauri::command]
pub fn dismiss_crash_review<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
    finish_crash_review(&app.state::<AppState>())
}

fn finish_crash_review(state: &AppState) -> Result<(), String> {
    let crash = state
        .pending_crash_review
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take();
    if let Some(crash) = crash {
        let mut prefs = state.prefs();
        prefs.view.last_reviewed_crash_unix = crash.timestamp;
        prefs.save()?;
    }
    Ok(())
}

/// Applies a new ER-01B consent decision (Preferences screen toggle) -- persists it and
/// live-swaps the held reporter; revocation deletes every queued-but-undelivered report.
#[tauri::command]
pub fn set_error_reporting_consent<R: Runtime>(
    app: AppHandle<R>,
    consent: ErrorReportingConsent,
) -> Result<(), String> {
    set_consent(&app.state::<AppState>(), consent)
}

fn set_consent(state: &AppState, consent: ErrorReportingConsent) -> Result<(), String> {
    {
        let mut prefs = state.prefs();
        prefs.view.error_reporting_consent = consent;
        prefs.save()?;
    }
    *state
        .error_reporter
        .lock()
        .unwrap_or_else(|p| p.into_inner()) = error_reporting::apply_consent_change(consent);
    Ok(())
}

/// Number of currently-queued, undelivered error reports -- the Preferences screen's status
/// line.
#[tauri::command]
pub fn get_queued_report_count() -> usize {
    error_reporting::queue_len()
}

/// Deletes every queued report -- the Preferences screen's explicit control.
#[tauri::command]
pub fn delete_queued_reports() {
    error_reporting::delete_all_queued();
}

#[cfg(test)]
#[path = "error_reporting_test.rs"]
mod tests;
