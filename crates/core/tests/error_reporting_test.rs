// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.
//
// ER-01A exit-criteria tests: the sanitizer's forbidden-content invariant over a corpus
// (nothing the sanitizer guarantees to remove survives in its output), validator refusal
// cases, the bounded-queue envelope, the consent-disabled NullReporter, and the stable
// builder. All public-API-only integration tests, like the rest of crates/core/tests/.

use avcore::{
    contains_forbidden_content, sanitize_stack_trace, sanitize_text, validate_envelope,
    validate_report, Breadcrumb, ClipCountBucket, ErrorCode, ErrorReport, ErrorReportBuilder,
    ErrorReporter, ErrorSeverity, ExportStage, MediaContext, NullReporter, Operation,
    QueueEnvelope, RecoveryOutcome, ReleaseMetadata, ReportValidationError, ResolutionBucket,
    RuntimeEnvironment, MAX_BREADCRUMBS, QUEUE_RETENTION_SECS, STATE_TRANSITIONS,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn clean_metadata() -> ReleaseMetadata {
    ReleaseMetadata {
        release: "oca-test".to_owned(),
        build_channel: "dev".to_owned(),
        target_triple: "x86_64-pc-windows-msvc".to_owned(),
        app_version: "9.9.9".to_owned(),
        commit: "0f1d2e3c4b5a6978".to_owned(),
    }
}

fn clean_builder() -> ErrorReportBuilder {
    let mut builder = ErrorReportBuilder::new(clean_metadata());
    builder.set_locale("pt-BR");
    builder
}

fn full_report(builder: &ErrorReportBuilder) -> ErrorReport {
    builder.build_report(
        ErrorCode::ExportEncode,
        ErrorSeverity::Error,
        Operation::Export,
        RecoveryOutcome::Aborted,
        false,
        Some("frame 12 failed at C:\\Users\\Player\\videos\\clip.mp4:3"),
    )
}

// ---------------------------------------------------------------------------
// Sanitizer corpus and invariants
// ---------------------------------------------------------------------------

/// Inputs that must be fully scrubbed: after sanitization, no forbidden content remains.
/// This is the ER-01A forbidden-data-invariant corpus (paths, home aliases, URLs, emails,
/// credentials, over large/unicode/pathological inputs).
#[rustfmt::skip]
const FORBIDDEN_CORPUS: &[&str] = &[
    r"C:\Users\John\AppData\Local\Temp",
    r"c:/users/john/downloads/video.mp4",
    r"E:\footage\2024\clip.mp4",
    r"F:/Movies/avengers_endgame.mkv",
    r"C:\\Users\\John\\videos\\cap.mp4",
    r"D:\\backups\\project.ocproj",
    r"\\NAS\media\video.mp4",
    r"\\192.168.0.10\share\folder\file.avi",
    "/home/lucas/.cargo/bin/cargo",
    "/Users/jane/Documents/project/project.ocproj",
    "/Users/francisco/videos/short.mp4",
    r"~/projects/game/recording.mkv",
    r"%USERPROFILE%\Downloads\export.mp4",
    "/cargo/registry/src/github.com/foo/bar",
    "/rustc/1.80.0/library/std/src/panic.rs:120:5",
    "/home/lucas/games/oh/weary",
    "https://example.com/api/v1/upload?token=abc123",
    "https://user:pass@example.com:8443/path?q=1#frag",
    "spn://server/live/stream", // non-http scheme: still a URL
    "john.doe@example.com",
    "lucasgsm88@gmail.com",
    "first.last+tag@sub.example.co",
    "password=letmein123",
    "PASSWORD: s3cr3t!",
    "api_key=sk-9f8e7d6c",
    "API_KEY: blahblah",
    "secret=supersecret",
    "token=abcdef0123456789",
    "access_token=g.h.i",
    "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9",
    "bearer A91b.cThdE/X_Y+z1",
    "client_secret = cli-007",
    "path is C:\\Users\\John and home is /home/lucas",
];

/// Prose-ish input that must NOT be mangled by the sanitizer (false-positive guard).
#[rustfmt::skip]
const CLEAN_CORPUS: &[&str] = &[
    "no paths here",
    "oh/weary sailor",          // middle-of-word slash, no boundary before it
    "score is 5 / 1",           // spaced out division, no drive/root match
    "100% user satisfaction",   // percent doesn't make %USERPROFILE%
    "a:b is a ratio",           // drive rule requires a slash right after the colon
    "call me at the office",    // @ nowhere
    "version 1.4.2 (stable)",   // dotted version, no URL scheme
    "pt-BR", "en",              // locale strings
    "x86_64-pc-windows-msvc",   // target triple
    "<home>", "<path>", "<url>", "<email>", "<redacted>", // inert markers
];

// ---------------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------------

#[test]
fn sanitizer_removes_every_forbidden_input() {
    for input in FORBIDDEN_CORPUS {
        assert!(
            contains_forbidden_content(input),
            "corpus entry should have been flagged pre-sanitize: {input:?}"
        );
        let scrubbed = sanitize_text(input);
        assert!(
            !contains_forbidden_content(&scrubbed),
            "forbidden content survived sanitization for {input:?} -> {scrubbed:?}"
        );
    }
}

#[test]
fn sanitizer_leaves_clean_prose_untouched() {
    for input in CLEAN_CORPUS {
        assert_eq!(
            sanitize_text(input),
            *input,
            "clean input was mangled by sanitizer: {input:?}"
        );
        assert!(
            !contains_forbidden_content(input),
            "clean input flagged: {input:?}"
        );
    }
}

#[test]
fn sanitizer_is_idempotent() {
    for input in FORBIDDEN_CORPUS {
        let once = sanitize_text(input);
        let twice = sanitize_text(&once);
        assert_eq!(
            twice, once,
            "sanitizer not idempotent for {input:?}: {once:?} -> {twice:?}"
        );
    }
}

#[test]
fn sanitizer_scrubs_each_stack_trace_line_independently() {
    let trace =
        "frame 0: foo::bar\nframe 1: read at C:\\Users\\John\\AppData\\cache.dat\nframe 2: http://evil.example/key";
    let scrubbed = sanitize_stack_trace(trace);
    assert!(!contains_forbidden_content(&scrubbed));
    assert_eq!(scrubbed.lines().count(), 3);
}

#[test]
fn sanitizer_caps_pathological_inputs_before_scanning() {
    let huge = format!("{} C:\\Users\\John\\never\\reached", "a".repeat(300 * 1024));
    let scrubbed = sanitize_text(&huge);
    assert!(scrubbed.len() <= 256 * 1024);
    assert!(!contains_forbidden_content(&scrubbed));
}

#[test]
fn sanitizer_stack_trace_truncates_at_char_boundary() {
    let huge = "é".repeat(20 * 1024); // 40 KiB, multibyte chars straddle the cap
    let scrubbed = sanitize_stack_trace(&huge);
    assert!(scrubbed.len() <= 16 * 1024);
    assert!(scrubbed.is_char_boundary(scrubbed.len()));
    assert!(!contains_forbidden_content(&scrubbed));
}

#[test]
fn sanitizer_replaces_paths_with_identifying_markers() {
    assert_eq!(sanitize_text(r"C:\Users\John\AppData"), "<home>\\AppData");
    assert_eq!(
        sanitize_text(r"E:\footage\clip.mp4"),
        "<path>footage\\clip.mp4"
    );
    assert_eq!(sanitize_text("https://example.com/f"), "<url>");
    assert_eq!(sanitize_text("hi@example.com"), "<email>");
    assert!(sanitize_text("password=letmein123").contains("<redacted>"));
}

// ---------------------------------------------------------------------------
// Small bucket helpers
// ---------------------------------------------------------------------------

#[test]
fn resolution_bucket_is_short_side_based() {
    assert_eq!(
        ResolutionBucket::from_dimensions(640, 480),
        Some(ResolutionBucket::Sd)
    );
    assert_eq!(
        ResolutionBucket::from_dimensions(1280, 720),
        Some(ResolutionBucket::Hd)
    );
    assert_eq!(
        ResolutionBucket::from_dimensions(1920, 1080),
        Some(ResolutionBucket::Fhd)
    );
    assert_eq!(
        ResolutionBucket::from_dimensions(1080, 1920),
        Some(ResolutionBucket::Fhd)
    );
    assert_eq!(
        ResolutionBucket::from_dimensions(2560, 1440),
        Some(ResolutionBucket::Qhd)
    );
    assert_eq!(
        ResolutionBucket::from_dimensions(3840, 2160),
        Some(ResolutionBucket::Uhd)
    );
    assert_eq!(ResolutionBucket::from_dimensions(0, 1080), None);
    assert_eq!(ResolutionBucket::from_dimensions(1920, 0), None);
}

#[test]
fn clip_count_bucket_boundaries() {
    assert_eq!(ClipCountBucket::from_len(0), ClipCountBucket::Few);
    assert_eq!(ClipCountBucket::from_len(10), ClipCountBucket::Few);
    assert_eq!(ClipCountBucket::from_len(11), ClipCountBucket::Moderate);
    assert_eq!(ClipCountBucket::from_len(100), ClipCountBucket::Moderate);
    assert_eq!(ClipCountBucket::from_len(101), ClipCountBucket::Many);
}

// ---------------------------------------------------------------------------
// Breadcrumbs
// ---------------------------------------------------------------------------

#[test]
fn state_transition_allowlist_access_is_enforced_at_build_time() {
    for known in STATE_TRANSITIONS {
        assert!(Breadcrumb::state_transition(known).is_some());
    }
    assert_eq!(Breadcrumb::state_transition("not_a_known_transition"), None);
    assert_eq!(Breadcrumb::state_transition(""), None);
}

// ---------------------------------------------------------------------------
// Builder + validator
// ---------------------------------------------------------------------------

#[test]
fn builder_produces_a_report_that_passes_validation() {
    let mut builder = clean_builder();
    builder.add_breadcrumb(Breadcrumb::state_transition("app_started").unwrap());
    builder.add_breadcrumb(Breadcrumb::state_transition("encoder_selected").unwrap());
    builder.add_breadcrumb(Breadcrumb::FeatureFlag {
        name: "proxy_level_1000".to_owned(),
        enabled: true,
    });
    builder.set_media_context(Some(MediaContext {
        container_family: Some("mp4".to_owned()),
        codec: Some("h264".to_owned()),
        resolution_bucket: ResolutionBucket::from_dimensions(1920, 1080),
        clip_count_bucket: Some(ClipCountBucket::from_len(12)),
        export_stage: Some(ExportStage::Encode),
    }));

    let report = full_report(&builder);
    assert_eq!(validate_report(&report), Ok(()));
    assert_eq!(report.schema_version, 1);
    assert_eq!(report.severity, ErrorSeverity::Error);
    assert_eq!(report.operation, Operation::Export);
    assert_eq!(report.recovery_outcome, RecoveryOutcome::Aborted);
    assert!(!report.retried);
}

#[test]
fn builder_session_id_is_stable_but_event_ids_are_not() {
    let builder = clean_builder();
    let first = builder.build_report(
        ErrorCode::Probe,
        ErrorSeverity::Warning,
        Operation::Probe,
        RecoveryOutcome::Degraded,
        false,
        None,
    );
    let second = builder.build_report(
        ErrorCode::Probe,
        ErrorSeverity::Warning,
        Operation::Probe,
        RecoveryOutcome::Degraded,
        false,
        None,
    );
    assert_eq!(first.session_id, second.session_id);
    assert_ne!(first.event_id, second.event_id);
    assert!(validate_report(&first).is_ok());
    assert!(validate_report(&second).is_ok());
}

#[test]
fn builder_caps_breadcrumbs_at_the_contract_limit() {
    let mut builder = clean_builder();
    for _ in 0..(MAX_BREADCRUMBS + 5) {
        builder.add_breadcrumb(Breadcrumb::state_transition("export_started").unwrap());
    }
    assert_eq!(builder.breadcrumbs().len(), MAX_BREADCRUMBS);
    let report = builder.build_report(
        ErrorCode::ExportResolve,
        ErrorSeverity::Error,
        Operation::Export,
        RecoveryOutcome::RequiresUserAction,
        false,
        None,
    );
    assert_eq!(report.breadcrumbs.len(), MAX_BREADCRUMBS);
    assert_eq!(validate_report(&report), Ok(()));
}

#[test]
fn builder_sanitizes_metadata_defensively() {
    let mut dirty = clean_metadata();
    dirty.release = r"C:\Users\John\app".to_owned();
    dirty.build_channel = "https://internal.example/branch".to_owned();
    let builder = ErrorReportBuilder::new(dirty);
    let report = builder.build_report(
        ErrorCode::Update,
        ErrorSeverity::Warning,
        Operation::Update,
        RecoveryOutcome::RequiresUserAction,
        true,
        None,
    );
    assert!(!contains_forbidden_content(&report.release));
    assert!(!contains_forbidden_content(&report.build_channel));
    assert_eq!(validate_report(&report), Ok(()));
}

#[test]
fn builder_fills_the_real_ffmpeg_version_and_no_longer_reports_gstreamer() {
    let builder = clean_builder();
    let report = builder.build_report(
        ErrorCode::Preview,
        ErrorSeverity::Warning,
        Operation::Preview,
        RecoveryOutcome::Degraded,
        false,
        None,
    );
    let env = &report.environment;
    let ffmpeg = env
        .ffmpeg_version
        .as_deref()
        .expect("linked FFmpeg version expected");
    assert!(
        ffmpeg
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())),
        "malformed ffmpeg version: {ffmpeg}"
    );
    assert_eq!(
        env.gstreamer_version, None,
        "GStreamer is gone; the legacy field stays empty"
    );
    assert_eq!(validate_report(&report), Ok(()));
}

#[test]
fn validate_rejects_wrong_schema_version() {
    let mut report = full_report(&clean_builder());
    report.schema_version = 2;
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::UnsupportedSchemaVersion(2))
    ));
}

#[test]
fn validate_rejects_an_unsanitized_stack_trace() {
    let mut report = full_report(&clean_builder());
    assert_eq!(validate_report(&report), Ok(()));
    report.sanitized_stack_trace = Some(r"C:\Users\John\raw\leak.rs:12".to_owned());
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::ForbiddenContent(
            "sanitized_stack_trace"
        ))
    ));
}

#[test]
fn validate_rejects_an_oversized_stack_trace() {
    let mut report = full_report(&clean_builder());
    report.sanitized_stack_trace = Some("fn_000".repeat(20 * 1024));
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::StackTraceTooLarge(_))
    ));
}

#[test]
fn validate_rejects_an_unknown_state_transition_pasted_in_after_build() {
    let mut report = full_report(&clean_builder());
    report.breadcrumbs.push(Breadcrumb::StateTransition {
        state: "escaped_allowlist".to_owned(),
    });
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::UnknownStateTransition(_))
    ));
}

#[test]
fn validate_rejects_too_many_breadcrumbs_injected_after_build() {
    let mut report = full_report(&clean_builder());
    report.breadcrumbs =
        vec![Breadcrumb::state_transition("app_started").unwrap(); MAX_BREADCRUMBS + 1];
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::TooManyBreadcrumbs(_))
    ));
}

#[test]
fn validate_rejects_forbidden_content_in_media_and_environment_fields() {
    let mut report = full_report(&clean_builder());
    report.media_context = Some(MediaContext {
        container_family: Some(r"C:\Users\John\evil.mov".to_owned()),
        ..MediaContext::default()
    });
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::ForbiddenContent("container_family"))
    ));

    let mut report = full_report(&clean_builder());
    report.environment = RuntimeEnvironment {
        encoder: Some(r"\\NAS\share\encoder.dll".to_owned()),
        ..RuntimeEnvironment::default()
    };
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::ForbiddenContent(
            "environment.encoder"
        ))
    ));
}

#[test]
fn validate_rejects_a_serialized_report_over_the_size_budget() {
    // A report sized to pass every per-field bound but whose serialized JSON exceeds
    // MAX_REPORT_BYTES: environment strings are bounded but not length-limited by the
    // structural checks (they are limit-checked only for forbidden content), which is the
    // one remaining growth axis a corrupt in-memory builder could abuse.
    let mut report = full_report(&clean_builder());
    report.environment.gpu_driver = Some("x".repeat(64 * 1024));
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::ReportTooLarge(_))
    ));
}

#[test]
fn validate_rejects_toy_metadata_ids_and_whitespace() {
    let mut report = full_report(&clean_builder());
    report.event_id = "EVIL EVENT ID".to_owned();
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::MissingField("event_id"))
    ));

    let mut report = full_report(&clean_builder());
    report.release = "oca release one".to_owned();
    assert!(matches!(
        validate_report(&report),
        Err(ReportValidationError::MissingField("release"))
    ));
}

// ---------------------------------------------------------------------------
// Wire format (serde) stability
// ---------------------------------------------------------------------------

#[test]
fn report_round_trips_through_json_with_stable_field_names() {
    let builder = clean_builder();
    let report = full_report(&builder);
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"error_code\":\"export_encode\""));
    assert!(json.contains("\"severity\":\"error\""));
    assert!(json.contains("\"operation\":\"export\""));
    assert!(json.contains("\"recovery_outcome\":\"aborted\""));
    assert!(json.contains("\"event_id\":"));

    let back: ErrorReport = serde_json::from_str(&json).unwrap();
    assert_eq!(back, report);
}

// ---------------------------------------------------------------------------
// Queue envelope
// ---------------------------------------------------------------------------

#[test]
fn envelope_wraps_a_report_with_bounded_immutable_bookkeeping() {
    let builder = clean_builder();
    let report = full_report(&builder);
    let now = 1_752_000_000;
    let envelope = QueueEnvelope::new(report.clone(), now);
    assert_eq!(envelope.envelope_version, 1);
    assert_eq!(envelope.event_id, report.event_id);
    assert_eq!(envelope.queued_at_unix, now);
    assert_eq!(envelope.expires_at_unix, now + QUEUE_RETENTION_SECS);
    assert_eq!(envelope.retry_count, 0);
    assert_eq!(envelope.report, report);
    assert_eq!(validate_envelope(&envelope, now), Ok(()));
    assert!(!envelope.is_expired(now));
}

#[test]
fn envelope_round_trips_through_json() {
    let builder = clean_builder();
    let envelope = QueueEnvelope::new(full_report(&builder), 1_752_000_000);
    let json = serde_json::to_string(&envelope).unwrap();
    let back: QueueEnvelope = serde_json::from_str(&json).unwrap();
    assert_eq!(back, envelope);
}

#[test]
fn validate_envelope_drops_expired_records() {
    let builder = clean_builder();
    let envelope = QueueEnvelope::new(full_report(&builder), 1_752_000_000);
    let past_expiry = 1_752_000_000 + QUEUE_RETENTION_SECS + 1;
    assert!(matches!(
        validate_envelope(&envelope, past_expiry),
        Err(ReportValidationError::Envelope {
            reason: "expired",
            ..
        })
    ));
}

#[test]
fn validate_envelope_rejects_tampered_versions_ids_and_retries() {
    let builder = clean_builder();

    let mut envelope = QueueEnvelope::new(full_report(&builder), 100);
    envelope.envelope_version = 99;
    assert!(matches!(
        validate_envelope(&envelope, 100),
        Err(ReportValidationError::Envelope {
            reason: "unsupported envelope version",
            ..
        })
    ));

    let mut envelope = QueueEnvelope::new(full_report(&builder), 100);
    envelope.event_id = "stolen_event".to_owned();
    assert!(matches!(
        validate_envelope(&envelope, 100),
        Err(ReportValidationError::Envelope {
            reason: "event id mismatch",
            ..
        })
    ));

    let mut envelope = QueueEnvelope::new(full_report(&builder), 100);
    envelope.retry_count = 11;
    assert!(matches!(
        validate_envelope(&envelope, 100),
        Err(ReportValidationError::Envelope {
            reason: "retry budget exhausted",
            ..
        })
    ));
}

#[test]
fn validate_envelope_still_runs_report_validation() {
    let builder = clean_builder();
    let mut envelope = QueueEnvelope::new(full_report(&builder), 100);
    envelope.report.schema_version = 2;
    assert!(matches!(
        validate_envelope(&envelope, 100),
        Err(ReportValidationError::UnsupportedSchemaVersion(2))
    ));
}

// ---------------------------------------------------------------------------
// Reporter abstraction
// ---------------------------------------------------------------------------

#[test]
fn null_reporter_quietly_drops_validated_reports() {
    let builder = clean_builder();
    let report = full_report(&builder);
    assert_eq!(validate_report(&report), Ok(()));

    let reporter: &dyn ErrorReporter = &NullReporter;
    reporter.report(report); // must not panic, must not block
}

#[test]
fn null_reporter_composes_with_expectation_of_consent_off() {
    // The consent-disabled path is "no reporter at all" — the NullReporter just makes the
    // decision explicit at a single call site instead of branching everywhere.
    let report = full_report(&clean_builder());
    NullReporter.report(report);
}
