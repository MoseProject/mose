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

//! Provider-neutral error-reporting contract (ER-01A, the first phase of ROADMAP P5's
//! ER-01). **Deliberately contains no networking and no Sentry (or any other provider) SDK
//! dependency** — everything here is the allowlisted report schema, the stable error codes,
//! the sanitizer that scrubs forbidden content out of free-form fields, the schema validator,
//! the bounded queue envelope, and the `ErrorReporter` abstraction with its consent-disabled
//! `NullReporter`. The transport (a background delivery worker + provider adapter) is UI-layer
//! work that lands in ER-01B, keyed off this module's types, never reaching past them.
//!
//! The existing local diagnostics stay exactly where they are: [`crate::telemetry`]'s JSON-lines
//! file (`ui`'s `telemetry.jsonl`) and the crash file `ui`'s `main.rs` panic hook writes. This
//! module is a *different, separate* path — the existing `TelemetryEvent::Error` carries
//! free-form strings and user content and **must not** become the remote transport (see the
//! ER-01 design doc's own warning). Nothing in this module ever takes a [`crate::telemetry::
//! TelemetryEvent`] or leaks provider types across its public boundary.
//!
//! The remote contract is an *allowlist*, not a blacklist: a report is a fixed set of typed
//! fields, and any string field that still contains forbidden content (paths, URLs, credentials,
//! emails) after sanitization is rejected by [`validate_report`] and dropped. What leaves the
//! machine is strictly the post-sanitization form.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Version of the [`ErrorReport`] schema. Bumped only on a breaking field change; the envelope
/// validator rejects anything else rather than loosely deserializing and forwarding it.
pub const ERROR_REPORT_SCHEMA_VERSION: u32 = 1;

/// Version of the [`QueueEnvelope`] wrapper around a report. Independent of
/// [`ERROR_REPORT_SCHEMA_VERSION`] — a report schema change can roll silently into the same
/// envelope version, but an envelope layout change bumps this.
pub const QUEUE_ENVELOPE_VERSION: u32 = 1;

/// Maximum number of breadcrumbs attached to one report ([`ErrorReport::breadcrumbs`]).
pub const MAX_BREADCRUMBS: usize = 20;

/// Maximum length of a sanitized stack trace inside a report, in bytes. Rust backtraces are
/// long; the interesting part (function names) fits in a small fraction of this.
pub const MAX_STACK_TRACE_BYTES: usize = 16 * 1024;

/// Maximum serialized size of one report, in bytes. Enforced by [`validate_report`] over the
/// actual serialized JSON (bounded before decompression/serialization/upload per the ER-01
/// policy), so a corrupt in-memory builder can never grow an unbounded payload.
pub const MAX_REPORT_BYTES: usize = 32 * 1024;

/// Hard cap on how much text [`sanitize_text`] will process at once. Adversarial free-form
/// input that claims to be a never-ending string is truncated (at a UTF-8 boundary) before
/// any regex work, so sanitization cost is bounded no matter how large the caller's input is.
const MAX_SANITIZE_INPUT_BYTES: usize = 256 * 1024;

/// Offline-queue budget, per the ER-01 spec: at most 20 records or 20 MiB, whichever comes
/// first, expired after [`QUEUE_RETENTION_SECS`]. These are constants the *envelope* is built
/// around; the actual queue directory is ER-01B's job.
pub const QUEUE_MAX_RECORDS: usize = 20;
pub const QUEUE_MAX_BYTES: u64 = 20 * 1024 * 1024;
pub const QUEUE_RETENTION_SECS: u64 = 7 * 24 * 60 * 60;

/// Maximum retry count an envelope is allowed to carry before it's abandoned — bounds delivery
/// attempts for a permanently-failing record.
const MAX_ENVELOPE_RETRIES: u32 = 10;

/// Stable state-transition names [`Breadcrumb::StateTransition`] is allowed to carry. An
/// allowlist, not a free-form string slot: a new transition is a deliberate, reviewed addition,
/// mirroring the ER-01 contract's "bounded state transitions" wording. Grouping/queries depend
/// on these names never drifting.
pub const STATE_TRANSITIONS: &[&str] = &[
    "app_started",
    "export_started",
    "encoder_selected",
    "retry_failed",
];

/// Whether a breadcrumb's free-text `state` field is one of the allowlisted names. `state` is
/// the one user-ish string on a breadcrumb, so validation refuses anything not in
/// [`STATE_TRANSITIONS`].
fn is_known_state_transition(state: &str) -> bool {
    STATE_TRANSITIONS.contains(&state)
}

// ---------------------------------------------------------------------------
// Stable error codes and companions
// ---------------------------------------------------------------------------

/// Stable, machine-readable failure code — the primary grouping key for remote reports
/// (`error_code + top application frames + operation`, per the ER-01 doc). Variants map one
/// to one onto serde's `snake_case` names, so the wire form is fixed regardless of any Rust
/// refactor. Codes are deliberately operation-scoped, not "one catch-all variant".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    /// `probe` on a source media file failed (unreadable/unsupported container).
    Probe,
    /// A file failed to complete the import pipeline (probe + loudness/proxy/waveform
    /// enrichment), after having been added to the project.
    Import,
    /// The preview pipeline failed to open/seek/decode a clip.
    Preview,
    /// The background autosave couldn't write its recovery file.
    Autosave,
    /// A `.ocproj` project load failed.
    ProjectLoad,
    /// A project save (manual or autosave-to-main-file) failed.
    ProjectSave,
    /// Resolving a timeline into export segments failed.
    ExportResolve,
    /// The encode itself failed (FFmpeg filter graph, worker, encoder error).
    ExportEncode,
    /// A native post-pass (text/shape/audio overlays) failed after encoding.
    ExportPostPass,
    /// Persisting/reloading the export queue (`ocqueue`) failed.
    ExportQueue,
    /// The update check or update apply failed.
    Update,
    /// A plugin/model failure — any AI feature (Whisper transcription, TTS, auto-reframe,
    /// background removal, motion tracking) whose model/bindings failed.
    Model,
    /// An unhandled Rust panic crashed the previous session. Reported only after the user
    /// reviews it on the next launch (ER-01B's "Send once / Always send / Do not send" offer) —
    /// never from the panic handler itself, since a crashing process cannot reliably complete a
    /// network request.
    Panic,
}

/// How bad an error is. Three levels, coarse on purpose — fine-grained triage is the
/// [`ErrorCode`]'s job, severity only scopes how loudly a report should be handled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorSeverity {
    /// Recoverable; the operation continued (possibly without the failing sub-step).
    Warning,
    /// The operation failed, but the app keeps running.
    Error,
    /// Session-endangering (e.g. project load failure right after a crash).
    Fatal,
}

/// What happened to the operation after the error — the "did the user lose work / is the app
/// still usable" dimension grouping reports with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryOutcome {
    /// Something automatic recovered (e.g. an autosave restore succeeded silently).
    AutoRecovered,
    /// The app kept running with degraded behavior (e.g. a proxy is missing, original used).
    Degraded,
    /// A human had to act (dismiss an error toast, re-run an export, ...).
    RequiresUserAction,
    /// The operation aborted and produced no usable result.
    Aborted,
}

/// Coarse "what were we doing" grouping, distinct from the specific [`ErrorCode`]. Several
/// codes share one operation (e.g. all three export stages are `Operation::Export`); the
/// operation is also what breadcrumbs use for `OperationStarted`-style context (kept for
/// symmetry with the report schema even though ER-01A's breadcrumbs don't emit that variant yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Probe,
    Import,
    Export,
    Preview,
    Autosave,
    ProjectLoad,
    ProjectSave,
    ExportQueue,
    Update,
    Model,
    /// The whole application process rather than one subsystem — used for
    /// [`ErrorCode::Panic`]'s post-crash review report, where the failing subsystem generally
    /// isn't recoverable from a bare panic message/backtrace.
    App,
}

// ---------------------------------------------------------------------------
// Structured breadcrumbs
// ---------------------------------------------------------------------------

/// One structured context record attached to a report (a bounded list per report, see
/// [`MAX_BREADCRUMBS`]). Typed rather than free-form strings so the allowlist survives
/// downstream, matching the ER-01 contract's "bounded list of structured breadcrumbs".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Breadcrumb {
    /// A named state transition such as `app_started` / `export_started` / `encoder_selected`.
    /// The name is checked against [`STATE_TRANSITIONS`] by [`validate_report`] — never a
    /// free-form slot.
    StateTransition { state: String },
    /// A boolean feature flag: the report was produced with the feature `name` in the
    /// `enabled` state. `name` is a stable feature identifier, not user content.
    FeatureFlag { name: String, enabled: bool },
}

impl Breadcrumb {
    /// Builds a state transition without an allocation — returns `None` (the breadcrumb is
    /// dropped) when `state` isn't in [`STATE_TRANSITIONS`], so an out-of-allowlist name can't
    /// even be constructed and accidentally shipped.
    pub fn state_transition(state: &str) -> Option<Self> {
        is_known_state_transition(state).then(|| Breadcrumb::StateTransition {
            state: state.to_owned(),
        })
    }
}

// ---------------------------------------------------------------------------
// Optional coarse dimensions
// ---------------------------------------------------------------------------

/// Coarse resolution bucket for the optional [`MediaContext`] dimension — short-side based so
/// a portrait short (`1080x1920`) and a landscape FHD (`1920x1080`) both land in `Fhd` the way
/// a human would read them. `None` from [`ResolutionBucket::from_dimensions`] on a zero-size
/// frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionBucket {
    Sd,
    Hd,
    Fhd,
    Qhd,
    Uhd,
}

impl ResolutionBucket {
    /// Buckets by the shorter side: `Sd` < 720, `Hd` < 1080, `Fhd` < 1440, `Qhd` < 2160,
    /// else `Uhd`. Returns `None` for a frame with a zero dimension.
    pub fn from_dimensions(width: u32, height: u32) -> Option<Self> {
        let short = width.min(height);
        if short == 0 {
            return None;
        }
        Some(if short < 720 {
            ResolutionBucket::Sd
        } else if short < 1080 {
            ResolutionBucket::Hd
        } else if short < 1440 {
            ResolutionBucket::Fhd
        } else if short < 2160 {
            ResolutionBucket::Qhd
        } else {
            ResolutionBucket::Uhd
        })
    }
}

/// Coarse clip-count bucket for the optional [`MediaContext`] dimension — deliberately three
/// broad bands rather than the real count (the count itself is fine data, but a *bucket* keeps
/// the report diff-friendly and trivially comparable).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipCountBucket {
    /// 0..=10 clips
    Few,
    /// 11..=100 clips
    Moderate,
    /// 101+ clips
    Many,
}

impl ClipCountBucket {
    pub fn from_len(count: usize) -> Self {
        match count {
            0..=10 => ClipCountBucket::Few,
            11..=100 => ClipCountBucket::Moderate,
            _ => ClipCountBucket::Many,
        }
    }
}

/// Which step of an export pipeline a report refers to (the coarse "export stage" dimension).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportStage {
    Probe,
    Resolve,
    Encode,
    PostPass,
}

/// Optional coarse diagnostic dimensions attached to a report. Every field is optional and
/// only filled when already known without probing user data — never from file names, project
/// names, paths, or arbitrary container metadata.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct MediaContext {
    pub container_family: Option<String>,
    pub codec: Option<String>,
    pub resolution_bucket: Option<ResolutionBucket>,
    pub clip_count_bucket: Option<ClipCountBucket>,
    pub export_stage: Option<ExportStage>,
}

/// Build/dependency identifiers already available at runtime without touching user data: the
/// linked FFmpeg (libavformat) version and — only where the app already knows them — GPU
/// vendor/driver and the export encoder actually selected. `gstreamer_version` is a legacy
/// field kept for schema compatibility and always `None` now. Verbatim
/// `nvidia-smi`-style probing is explicitly out of scope; `None` means "not known here", never
/// "probe harder".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct RuntimeEnvironment {
    pub ffmpeg_version: Option<String>,
    pub gstreamer_version: Option<String>,
    pub gpu_vendor: Option<String>,
    pub gpu_driver: Option<String>,
    pub encoder: Option<String>,
}

/// The strict, allowlisted report schema. All free-form string fields pass through
/// [`sanitize_text`] (see [`ErrorReportBuilder`]) and are re-checked by [`validate_report`];
/// serialize-`Deserialize` exists so the queue (ER-01B) can round-trip persisted records, not
/// because arbitrary JSON may ever be loosely deserialized into this.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ErrorReport {
    pub schema_version: u32,
    /// Random per-report identifier (hex) — dedup key.
    pub event_id: String,
    /// Ephemeral per-launch identifier (hex), shared by every report from one process run.
    /// Deliberately *not* a stable installation id — that stays a documented future option.
    pub session_id: String,
    /// Canonical release identifier, e.g. `oca-1.4.2` — must match what packaging/CI publish.
    pub release: String,
    pub build_channel: String,
    pub target_triple: String,
    pub app_version: String,
    pub commit: String,
    /// `std::env::consts::OS` ("windows"/"macos"/...).
    pub os: String,
    pub os_version: Option<String>,
    /// `std::env::consts::ARCH` ("x86_64"/...).
    pub arch: String,
    pub locale: String,
    /// Whether this build is a portable/standalone install or an installed one.
    pub portable: bool,
    pub error_code: ErrorCode,
    pub severity: ErrorSeverity,
    pub operation: Operation,
    pub recovery_outcome: RecoveryOutcome,
    /// Whether the failed action was retried.
    pub retried: bool,
    /// Sanitized stack trace (may still be `None` for low-severity/no-stack reports).
    pub sanitized_stack_trace: Option<String>,
    pub breadcrumbs: Vec<Breadcrumb>,
    pub media_context: Option<MediaContext>,
    pub environment: RuntimeEnvironment,
}

// ---------------------------------------------------------------------------
// Sanitizer
// ---------------------------------------------------------------------------

/// One ordered scrub rule. `re` matches a forbidden pattern; the replacement is either a fixed
/// marker or `KeepFirst` (rebuild the match from capture group 1 + a marker, used when the
/// pattern *starts* with a legitimate connecting character/prefix — a preceding space, the key
/// of a `password=...` pair — that should survive). No rule uses look-around — `regex`'s engine
/// does not support it, so boundaries are consuming captures or anchors (`\A`, `$`, `\b`) only.
struct ScrubRule {
    re: Regex,
    repl: ScrubReplacement,
}

enum ScrubReplacement {
    Fixed(&'static str),
    KeepFirst(&'static str),
    /// Rebuild from capture groups 1 + marker + 3 — for patterns whose match both starts and
    /// ends with a legitimate connecting char (a leading boundary and the trailing separator
    /// after the dropped path segment).
    KeepSides(&'static str),
}

fn rules() -> &'static Vec<ScrubRule> {
    static RULES: OnceLock<Vec<ScrubRule>> = OnceLock::new();
    RULES.get_or_init(|| {
        vec![
            // Tilde home alias: `~/`, `~\`, `~ `, or a lone trailing `~`. `~` is matched with
            // its one following `/`, `\` or whitespace char captured and kept, so `~/foo`
            // becomes `<home>/foo`; a `~` before a digit or word (e.g. `~5`) is never touched.
            ScrubRule {
                re: Regex::new(r"~([/\\\s])").expect("valid regex"),
                repl: ScrubReplacement::KeepFirst(HOME),
            },
            ScrubRule {
                re: Regex::new(r"~$").expect("valid regex"),
                repl: ScrubReplacement::Fixed(HOME),
            },
            // %USERPROFILE% home alias (Windows), any case.
            ScrubRule {
                re: Regex::new(r"(?i)%userprofile%").expect("valid regex"),
                repl: ScrubReplacement::Fixed(HOME),
            },
            // Windows home root: `C:\Users\John\...` (also `c:/users/john/...`). Replaces the
            // root through the username; anything after the username's own path separator
            // stays. Runs before the generic drive-letter rule so `C:\Users\...` never
            // degrades to the generic `<path>` with the username leaking through.
            ScrubRule {
                re: Regex::new(r"(?i)\b[a-z]:[\\/]users[\\/][^\\/]*").expect("valid regex"),
                repl: ScrubReplacement::Fixed(HOME),
            },
            // Unix home roots: `/home/<name>/...` and `/Users/<name>/...`, any case. Keeps a
            // single preceding boundary character (whitespace/paren/quote/string start) so a
            // slash in the middle of a word like "oh/weary" isn't touched — but must run before
            // the generic unix-root rule below so `/home/...` is recognized as home first.
            // `\x22` is the double quote, spelled out since this is a raw string literal.
            ScrubRule {
                re: Regex::new(r"(?i)((?:\A|[\s('`\x22]))/(?:home|users)/[^/\s'<>\x22;:,]*")
                    .expect("valid regex"),
                repl: ScrubReplacement::KeepFirst(HOME),
            },
            // Windows home root as it appears inside JSON/serialized dumps (`C:\\Users\\John\...`
            // — backslashes doubled). Same shape as the rule above, backslash-escaped form; must
            // also run before the generic drive rule.
            ScrubRule {
                re: Regex::new(r"(?i)\b[a-z]:\\\\users\\\\[^\\\\]*").expect("valid regex"),
                repl: ScrubReplacement::Fixed(HOME),
            },
            // Generic Windows drive root `X:\` or `X:\\` (any drive letter, either slash
            // direction) — the tail past the drive is kept.
            ScrubRule {
                re: Regex::new(r"(?i)\b[a-z]:[\\/]{1,2}").expect("valid regex"),
                repl: ScrubReplacement::Fixed(PATH),
            },
            // UNC share `\\server\share` (or `\\server/share`) — server and share are the identifying
            // part; tail past the share stays.
            ScrubRule {
                re: Regex::new(r"\\\\[^\\\s]+[\\/][^\\\s]+").expect("valid regex"),
                repl: ScrubReplacement::Fixed(PATH),
            },
            // Generic unix absolute root `/segment/` (e.g. `/cargo/...`, `/rustc/...`). Only
            // fires at a string start or after a boundary *and* only when a further `/`, `\` or
            // end-of-input follows, so a bare slash in ordinary prose is never touched. `\A`
            // here can only match an original leading `/`, since markers placed by earlier rules
            // do not start with one — keeping the whole pipeline effectively idempotent. Capture
            // group 1 is the connecting prefix (kept), group 2 the path segment (dropped) and
            // group 3 the following separator (kept; empty on the end-of-input branch). `regex`
            // cannot look ahead, so the trailing `/` or `\` is consumed and rebuilt instead of
            // asserted.
            ScrubRule {
                re: Regex::new(r"((?:\A|[\s('`\x22]))(/[^/\s'<>\x22;:,]*)(?:([\\/])|$)")
                    .expect("valid regex"),
                repl: ScrubReplacement::KeepSides(PATH),
            },
            // Email addresses (also the only realistic `@` vector beyond credentials).
            ScrubRule {
                re: Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}")
                    .expect("valid regex"),
                repl: ScrubReplacement::Fixed(EMAIL),
            },
            // URLs, whole thing including scheme, host, path and query. `\x22` is the double quote,
            // spelled out since this is a raw string literal.
            ScrubRule {
                re: Regex::new(r"[A-Za-z][A-Za-z0-9+.-]*://[^\s'<>\x22]+").expect("valid regex"),
                repl: ScrubReplacement::Fixed(URL),
            },
            // `Bearer <token>` — the token after the keyword is the identifying part. Runs
            // *before* the credential pair below so `Authorization: Bearer eyJ...` gets its
            // token redacted first (otherwise the credential rule eats just `Bearer` and the
            // JWT after it leaks through untouched).
            ScrubRule {
                re: Regex::new(r"(?i)(\bbearer\s+)([A-Za-z0-9._~+/-]+)").expect("valid regex"),
                repl: ScrubReplacement::KeepFirst(REDACTED),
            },
            // `password=...`, `api_key: ...`, `authorization: ...`, etc. — key and separator
            // survive, value is redacted. Group 1 = key+separator, group 2 = the value to
            // redact (double quotes allowed in the value so `password="s3cr3t"` redacts the
            // whole quoted string).
            ScrubRule {
                re: Regex::new(
                    r"(?i)((?:(?:password|passwd|secret|token|api[_-]?key|access[_-]?token|refresh[_-]?token|client[_-]?secret|authorization)\b\s*(?:=|:)\s*))([^\s&'<>;]+)",
                )
                .expect("valid regex"),
                repl: ScrubReplacement::KeepFirst(REDACTED),
            },
        ]
    })
}

const HOME: &str = "<home>";
const PATH: &str = "<path>";
const URL: &str = "<url>";
const EMAIL: &str = "<email>";
const REDACTED: &str = "<redacted>";

/// Scrub forbidden content (home dirs, filesystem paths, URLs, emails, credentials) out of
/// `input`, replacing each occurrence with a marker placeholder. The replacement markers are
/// inert — none of the rules match them — so the output no longer triggers
/// [`contains_forbidden_content`]. Truncates pathological inputs (in bytes, at a UTF-8
/// boundary) before doing any work.
pub fn sanitize_text(input: &str) -> String {
    let mut out = input;
    if out.len() > MAX_SANITIZE_INPUT_BYTES {
        out = &out[..floor_char_boundary(out, MAX_SANITIZE_INPUT_BYTES)];
    }
    let mut result = out.to_owned();
    for rule in rules() {
        result = match &rule.repl {
            ScrubReplacement::Fixed(marker) => rule.re.replacen(&result, 0, *marker).into_owned(),
            ScrubReplacement::KeepFirst(marker) => rule
                .re
                .replace_all(&result, |caps: &regex::Captures<'_>| {
                    format!("{}{}", &caps[1], marker)
                })
                .into_owned(),
            ScrubReplacement::KeepSides(marker) => rule
                .re
                .replace_all(&result, |caps: &regex::Captures<'_>| {
                    let trailing = caps.get(3).map_or("", |m| m.as_str());
                    format!("{}{}{}", &caps[1], marker, trailing)
                })
                .into_owned(),
        };
    }
    result
}

/// A copy of [`sanitize_text`] specialized for stack traces: each line is scrubbed
/// independently (so one path-bearing line can't entangle the rest), and the result is then
/// truncated to [`MAX_STACK_TRACE_BYTES`]. Drop-in safe for any multi-line free-form field.
pub fn sanitize_stack_trace(input: &str) -> String {
    let mut out = String::new();
    for line in input.lines() {
        out.push_str(&sanitize_text(line));
        out.push('\n');
        if out.len() >= MAX_STACK_TRACE_BYTES {
            out.truncate(floor_char_boundary(&out, MAX_STACK_TRACE_BYTES));
            break;
        }
    }
    out
}

/// Returns `true` when `input` still contains anything the sanitizer is guaranteed to remove.
/// The schema validator calls this on every free-form string field of a built report; because
/// [`sanitize_text`]'s replacement markers are inert, `contains_forbidden_content(sanitize_text(x))`
/// is the property the sanitizer tests pin down.
pub fn contains_forbidden_content(input: &str) -> bool {
    rules().iter().any(|rule| rule.re.is_match(input))
}

/// Largest `i <= byte_limit` such that `s[..i]` is valid UTF-8 (i.e. `i` is a char boundary),
/// and `s.len() > byte_limit`. Bounds-checks trivially since truncation always happens before.
fn floor_char_boundary(s: &str, byte_limit: usize) -> usize {
    if byte_limit >= s.len() {
        return s.len();
    }
    let mut boundary = byte_limit;
    while !s.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

// ---------------------------------------------------------------------------
// Validator
// ---------------------------------------------------------------------------

/// Why a report (or envelope) failed validation. Every condition is a reason to drop the record
/// locally with a non-sensitive counter, per the ER-01 policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReportValidationError {
    UnsupportedSchemaVersion(u32),
    /// A required free-form field was empty.
    MissingField(&'static str),
    /// A free-form field still contains forbidden content after sanitization.
    ForbiddenContent(&'static str),
    TooManyBreadcrumbs(usize),
    UnknownStateTransition(String),
    StackTraceTooLarge(usize),
    /// Serialized report exceeds [`MAX_REPORT_BYTES`].
    ReportTooLarge(usize),
    /// Envelope-level issues (wrong version, mismatched event id, expired, exhausted retries).
    Envelope {
        event_id: String,
        reason: &'static str,
    },
}

impl std::fmt::Display for ReportValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReportValidationError::UnsupportedSchemaVersion(v) => {
                write!(f, "unsupported report schema version {v}")
            }
            ReportValidationError::MissingField(field) => {
                write!(f, "missing required field {field}")
            }
            ReportValidationError::ForbiddenContent(field) => {
                write!(f, "forbidden content in field {field}")
            }
            ReportValidationError::TooManyBreadcrumbs(n) => {
                write!(f, "{n} breadcrumbs exceeds the maximum")
            }
            ReportValidationError::UnknownStateTransition(s) => {
                write!(f, "unknown state transition {s:?}")
            }
            ReportValidationError::StackTraceTooLarge(n) => {
                write!(f, "stack trace of {n} bytes exceeds the maximum")
            }
            ReportValidationError::ReportTooLarge(n) => {
                write!(f, "report of {n} bytes exceeds the maximum")
            }
            ReportValidationError::Envelope { event_id, reason } => {
                write!(f, "envelope {event_id}: {reason}")
            }
        }
    }
}

impl std::error::Error for ReportValidationError {}

fn field_nonempty_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && !value.chars().any(char::is_whitespace)
}

fn id_ok(value: &str) -> bool {
    !value.is_empty() && value.len() <= 64 && value.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Schema validation for a built [`ErrorReport`]. Structural checks plus an explicit
/// forbidden-content sweep over every free-form string field. Errors are drop-and-count, never
/// forwarded. Returns `Ok(())` only when the report is safe to persist and — later — to
/// transmit verbatim.
pub fn validate_report(report: &ErrorReport) -> Result<(), ReportValidationError> {
    if report.schema_version != ERROR_REPORT_SCHEMA_VERSION {
        return Err(ReportValidationError::UnsupportedSchemaVersion(
            report.schema_version,
        ));
    }
    if !id_ok(&report.event_id) {
        return Err(ReportValidationError::MissingField("event_id"));
    }
    if !id_ok(&report.session_id) {
        return Err(ReportValidationError::MissingField("session_id"));
    }
    for (field, value) in [
        ("release", &report.release),
        ("build_channel", &report.build_channel),
        ("target_triple", &report.target_triple),
        ("app_version", &report.app_version),
        ("commit", &report.commit),
        ("os", &report.os),
        ("arch", &report.arch),
        ("locale", &report.locale),
    ] {
        if !field_nonempty_ok(value) {
            return Err(ReportValidationError::MissingField(field));
        }
    }
    if let Some(v) = &report.os_version {
        if v.len() > 128 || contains_forbidden_content(v) {
            return Err(ReportValidationError::ForbiddenContent("os_version"));
        }
    }
    if let Some(stack) = &report.sanitized_stack_trace {
        if stack.len() > MAX_STACK_TRACE_BYTES {
            return Err(ReportValidationError::StackTraceTooLarge(stack.len()));
        }
        if contains_forbidden_content(stack) {
            return Err(ReportValidationError::ForbiddenContent(
                "sanitized_stack_trace",
            ));
        }
    }
    if report.breadcrumbs.len() > MAX_BREADCRUMBS {
        return Err(ReportValidationError::TooManyBreadcrumbs(
            report.breadcrumbs.len(),
        ));
    }
    for b in &report.breadcrumbs {
        match b {
            Breadcrumb::StateTransition { state } => {
                if !is_known_state_transition(state) {
                    return Err(ReportValidationError::UnknownStateTransition(state.clone()));
                }
            }
            Breadcrumb::FeatureFlag { name, .. } => {
                if name.is_empty() || name.len() > 64 || contains_forbidden_content(name) {
                    return Err(ReportValidationError::ForbiddenContent("breadcrumb name"));
                }
            }
        }
    }
    for (field, value) in [
        ("release", &report.release),
        ("build_channel", &report.build_channel),
        ("target_triple", &report.target_triple),
        ("app_version", &report.app_version),
        ("commit", &report.commit),
        ("locale", &report.locale),
    ] {
        if contains_forbidden_content(value) {
            return Err(ReportValidationError::ForbiddenContent(field));
        }
    }
    if let Some(mc) = &report.media_context {
        for (field, value) in [
            (
                "container_family",
                mc.container_family.as_deref().unwrap_or(""),
            ),
            ("codec", mc.codec.as_deref().unwrap_or("")),
        ] {
            if !value.is_empty() && contains_forbidden_content(value) {
                return Err(ReportValidationError::ForbiddenContent(field));
            }
        }
    }
    for (field, value) in [
        (
            "environment.ffmpeg_version",
            report.environment.ffmpeg_version.as_deref(),
        ),
        (
            "environment.gstreamer_version",
            report.environment.gstreamer_version.as_deref(),
        ),
        (
            "environment.gpu_vendor",
            report.environment.gpu_vendor.as_deref(),
        ),
        (
            "environment.gpu_driver",
            report.environment.gpu_driver.as_deref(),
        ),
        ("environment.encoder", report.environment.encoder.as_deref()),
    ] {
        // Forbidden-content only: these are closed-set identifiers (versions/vendors), not
        // free-form text, and the serialized-size gate below bounds them regardless — an
        // absurdly long encoder string fails the size gate, not here.
        if let Some(value) = value {
            if contains_forbidden_content(value) {
                return Err(ReportValidationError::ForbiddenContent(field));
            }
        }
    }
    match serde_json::to_vec(report) {
        Ok(bytes) if bytes.len() <= MAX_REPORT_BYTES => Ok(()),
        Ok(bytes) => Err(ReportValidationError::ReportTooLarge(bytes.len())),
        Err(_) => Err(ReportValidationError::ReportTooLarge(usize::MAX)),
    }
}

/// Schema validation for a persisted [`QueueEnvelope`] — the recipient-side (well, the
/// ER-01B load-side) gate that makes corrupt/oversized local state drop rather than reach
/// delivery.
pub fn validate_envelope(
    envelope: &QueueEnvelope,
    now_unix: u64,
) -> Result<(), ReportValidationError> {
    if envelope.envelope_version != QUEUE_ENVELOPE_VERSION {
        return Err(ReportValidationError::Envelope {
            event_id: envelope.event_id.clone(),
            reason: "unsupported envelope version",
        });
    }
    if envelope.event_id != envelope.report.event_id {
        return Err(ReportValidationError::Envelope {
            event_id: envelope.event_id.clone(),
            reason: "event id mismatch",
        });
    }
    if envelope.expires_at_unix < envelope.queued_at_unix {
        return Err(ReportValidationError::Envelope {
            event_id: envelope.event_id.clone(),
            reason: "inverted expiry",
        });
    }
    if envelope.is_expired(now_unix) {
        return Err(ReportValidationError::Envelope {
            event_id: envelope.event_id.clone(),
            reason: "expired",
        });
    }
    if envelope.retry_count > MAX_ENVELOPE_RETRIES {
        return Err(ReportValidationError::Envelope {
            event_id: envelope.event_id.clone(),
            reason: "retry budget exhausted",
        });
    }
    validate_report(&envelope.report)
}

// ---------------------------------------------------------------------------
// Queue envelope
// ---------------------------------------------------------------------------

/// Versioned wrapper persisted for the offline queue (ER-01B): a validated report plus the
/// bookkeeping a bounded, expiring delivery worker needs. Never written or read by ER-01A
/// itself — this type is the *shape* the ER-01B queue is built around, which is what "queue
/// envelope" means in this phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct QueueEnvelope {
    pub envelope_version: u32,
    pub event_id: String,
    /// Unix-epoch seconds the record was queued.
    pub queued_at_unix: u64,
    /// `queued_at_unix + QUEUE_RETENTION_SECS`; past this the record is dropped.
    pub expires_at_unix: u64,
    /// How many delivery attempts have already been made (transient failures only).
    pub retry_count: u32,
    pub report: ErrorReport,
}

impl QueueEnvelope {
    /// Wraps a report with `now_unix` as the queue time and expiry [`QUEUE_RETENTION_SECS`]
    /// later. `now_unix` is explicit (not read from the wall clock) so tests can pin behaviour;
    /// callers in ER-01B pass the current unix seconds.
    pub fn new(report: ErrorReport, now_unix: u64) -> Self {
        Self {
            envelope_version: QUEUE_ENVELOPE_VERSION,
            event_id: report.event_id.clone(),
            queued_at_unix: now_unix,
            expires_at_unix: now_unix.saturating_add(QUEUE_RETENTION_SECS),
            retry_count: 0,
            report,
        }
    }

    /// Whether this record has outlived its retention window.
    pub fn is_expired(&self, now_unix: u64) -> bool {
        now_unix >= self.expires_at_unix
    }
}

// ---------------------------------------------------------------------------
// Reporter abstraction
// ---------------------------------------------------------------------------

/// The provider-neutral sink handled errors and panics are dispatched to. `report` must be
/// synchronous, non-blocking, and never panic — an error path must not block on disk or network
/// I/O, anywhere. The consent decision is made *before* a reporter exists: the consent-disabled
/// state simply holds no reporter (or a [`NullReporter`]); it is never a delivery decision made
/// inside `report`.
pub trait ErrorReporter: Send + Sync {
    fn report(&self, report: ErrorReport);
}

/// The consent-disabled reporter: accepts and drops every report as if reporting were off.
/// Used whenever the user has not opted in. Guarantees the "consent-off writes no remote queue
/// records" invariant without conditional branches at call sites.
pub struct NullReporter;

impl ErrorReporter for NullReporter {
    fn report(&self, _report: ErrorReport) {}
}

// ---------------------------------------------------------------------------
// Metadata + builder
// ---------------------------------------------------------------------------

/// Static release/build identity attached to every report from this build. `release` must be
/// the canonical identifier packaging/CI actually publishes (ER-01C's job once shipping);
/// the remaining fields give symbolication the exact context even when `release` is a coarse
/// chunk. All strings are re-sanitized defensively by [`ErrorReportBuilder::new`].
#[derive(Debug, Clone)]
pub struct ReleaseMetadata {
    pub release: String,
    pub build_channel: String,
    pub target_triple: String,
    pub app_version: String,
    pub commit: String,
}

/// Counter feeding [`next_id_hex`] so two reports built in the same nanosecond still differ.
static EVENT_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One 64-bit chunk of entropy — wall-clock nanoseconds XORed with the process id and a
/// monotonic counter, then split-mixed. Non-cryptographic, deliberately: event/session ids only
/// need uniqueness and correlation, not unguessability, and no `rand`/`uuid` dependency is
/// warranted for that.
fn next_id_hex() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let counter = EVENT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mixed = splitmix64(nanos ^ (pid.wrapping_mul(0x9E3779B9)) ^ (counter << 32));
    format!("{mixed:016x}")
}

fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E3779B97F4A7C15);
    x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB);
    x ^ (x >> 31)
}

/// Renders [`avbridge::version()`]'s packed `(major<<16)|(minor<<8)|micro` as `major.minor.micro`.
fn format_ffmpeg_version(packed: u32) -> String {
    format!(
        "{}.{}.{}",
        packed >> 16,
        (packed >> 8) & 0xff,
        packed & 0xff
    )
}

fn default_environment() -> RuntimeEnvironment {
    RuntimeEnvironment {
        ffmpeg_version: Some(format_ffmpeg_version(avbridge::version())),
        // GStreamer is no longer part of the app; the field stays in the (strict) report schema
        // so queued and stored reports keep parsing, and is simply never filled in.
        gstreamer_version: None,
        gpu_vendor: None,
        gpu_driver: None,
        encoder: None,
    }
}

/// Assembles [`ErrorReport`]s from a fixed process-wide identity plus per-report error fields.
/// One instance per process (see `crates/ui/src/app/error_reporting.rs`) so `session_id` is
/// stable across a whole launch and breadcrumbs accumulate in one place. Never touches a
/// provider SDK and never performs I/O; it only builds and sanitizes the in-memory contract.
pub struct ErrorReportBuilder {
    session_id: String,
    release: String,
    build_channel: String,
    target_triple: String,
    app_version: String,
    commit: String,
    os: &'static str,
    arch: &'static str,
    os_version: Option<String>,
    locale: String,
    portable: bool,
    breadcrumbs: Vec<Breadcrumb>,
    media_context: Option<MediaContext>,
    environment: RuntimeEnvironment,
}

impl ErrorReportBuilder {
    /// Starts a builder for this process/launch. Generates (and keeps) the ephemeral
    /// `session_id`, snapshots OS/arch from `std::env::consts`, and — as a defense-in-depth
    /// layer on top of wherever `metadata` came from — runs every metadata string through
    /// [`sanitize_text`] so no caller-supplied build metadata can smuggle user content into the
    /// reserved identity fields.
    pub fn new(metadata: ReleaseMetadata) -> Self {
        Self {
            session_id: next_id_hex(),
            release: sanitize_text(&metadata.release),
            build_channel: sanitize_text(&metadata.build_channel),
            target_triple: sanitize_text(&metadata.target_triple),
            app_version: sanitize_text(&metadata.app_version),
            commit: sanitize_text(&metadata.commit),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            os_version: None,
            locale: "en".to_owned(),
            portable: false,
            breadcrumbs: Vec::new(),
            media_context: None,
            environment: default_environment(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn set_locale(&mut self, locale: impl Into<String>) {
        self.locale = locale.into();
    }

    pub fn set_portable(&mut self, portable: bool) {
        self.portable = portable;
    }

    pub fn set_os_version(&mut self, version: impl Into<String>) {
        self.os_version = Some(version.into());
    }

    pub fn set_media_context(&mut self, context: Option<MediaContext>) {
        self.media_context = context;
    }

    pub fn set_runtime_environment(&mut self, environment: RuntimeEnvironment) {
        self.environment = environment;
    }

    /// Appends a breadcrumb, keeping at most [`MAX_BREADCRUMBS`] (oldest dropped first) — the
    /// report contract bounds breadcrumbs regardless of how chatty the caller is.
    pub fn add_breadcrumb(&mut self, crumb: Breadcrumb) {
        self.breadcrumbs.push(crumb);
        if self.breadcrumbs.len() > MAX_BREADCRUMBS {
            self.breadcrumbs.remove(0);
        }
    }

    pub fn breadcrumbs(&self) -> &[Breadcrumb] {
        &self.breadcrumbs
    }

    /// Builds one structured report. `stack_trace` is a raw backtrace display string (callers
    /// never pre-sanitize — that would risk forgetting), which [`sanitize_stack_trace`] scrubs
    /// and caps here. `event_id` is fresh per call. Memory-only; the caller decides whether to
    /// validate, persist, or hand the result to an [`ErrorReporter`].
    pub fn build_report(
        &self,
        error_code: ErrorCode,
        severity: ErrorSeverity,
        operation: Operation,
        recovery_outcome: RecoveryOutcome,
        retried: bool,
        stack_trace: Option<&str>,
    ) -> ErrorReport {
        ErrorReport {
            schema_version: ERROR_REPORT_SCHEMA_VERSION,
            event_id: next_id_hex(),
            session_id: self.session_id.clone(),
            release: self.release.clone(),
            build_channel: self.build_channel.clone(),
            target_triple: self.target_triple.clone(),
            app_version: self.app_version.clone(),
            commit: self.commit.clone(),
            os: self.os.to_owned(),
            os_version: self.os_version.clone(),
            arch: self.arch.to_owned(),
            locale: self.locale.clone(),
            portable: self.portable,
            error_code,
            severity,
            operation,
            recovery_outcome,
            retried,
            sanitized_stack_trace: stack_trace.map(sanitize_stack_trace),
            breadcrumbs: self.breadcrumbs.clone(),
            media_context: self.media_context.clone(),
            environment: self.environment.clone(),
        }
    }
}

#[cfg(test)]
#[path = "error_reporting/error_reporting_test.rs"]
mod tests;
