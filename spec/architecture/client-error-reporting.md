# Client Error Reporting

**Status:** proposed

**Roadmap ID:** ER-01

**Priority:** before broad beta distribution and before CF-10 cloud integrations

**Scope:** handled application errors, Rust panics, native crashes, symbolication, consent, and
operational triage

## Problem

oca currently keeps diagnostics on the client:

- `avcore::telemetry` writes bounded JSON Lines events to a rotating local file;
- `ui::app::telemetry` sends those events to a background writer when local telemetry is enabled;
- `ui::main` writes rotating `tracing` logs and a stack-bearing `crash_<timestamp>.txt` file;
- the crash sentinel offers recovery feedback on the next launch.

This is sufficient for a user who can find and manually share a diagnostic file, but it does not
show maintainers which failures are common, which release introduced a regression, or whether a
native FFmpeg/GStreamer crash affects many installations. It also provides no automatic
symbolication or issue grouping.

The existing `TelemetryEvent::Error { context, message }` is intentionally local and contains
free-form strings. It **must not** become the remote transport: messages may include absolute
paths, URLs, media metadata, project names, or user content. Remote reporting needs a separate,
strictly allowlisted contract.

## Goals

1. Collect actionable, deduplicated reports for handled errors, Rust panics, and native crashes.
2. Attach an exact release/build identity so Rust, C, and platform frames can be symbolicated.
3. Protect media, project contents, paths, transcripts, credentials, and personal information by
   default.
4. Require an explicit user decision before automatic remote reporting and allow a one-time send.
5. Never block the editor, export queue, application startup, or shutdown on a network request.
6. Preserve useful reports while offline through a bounded, expiring queue.
7. Hide the provider behind an internal interface so the product can change vendor or deployment
   model without changing error-producing code.

## Non-goals

- Uploading raw application logs, `.ocproj` files, media, frames, audio, thumbnails, transcripts,
  collaboration bundles, or crash-recovery files.
- Replacing the existing local telemetry and local crash report.
- Building a custom ingestion, grouping, symbolication, and alerting service.
- Adding distributed tracing or continuous performance monitoring in ER-01.
- Shipping native crash collection before its handler, packaging, update, and platform behavior
  have been validated on every supported operating system.
- Silently collecting a permanent advertising or cross-application identifier.

## Architecture decision

### Initial backend

Use Sentry Cloud for the first implementation, behind oca-owned interfaces. It already supplies
release tracking, grouping, regressions, alerts, breadcrumbs, and debug-symbol processing. A
self-hosted Sentry installation has meaningful operational cost and should only be reconsidered
for a documented data-residency, scale, or cost requirement.

The Rust SDK is suitable for handled Rust errors, panics, tracing integration, backtraces, and
standard contexts. It is still pre-1.0, so ER-01 must pin a reviewed compatible version, verify its
MSRV against the workspace toolchain, disable unused default integrations, and treat SDK upgrades
as explicit dependency changes rather than unattended drift.

Native crashes in FFmpeg, GStreamer, GPU drivers, or `avbridge` require a native crash handler.
The intended implementation is `sentry-native` with its Crashpad backend, but that is a later
phase: the standalone native SDK is described upstream as experimental/best-effort and introduces
an out-of-process handler plus platform packaging requirements. The Rust-only phase must deliver
value without claiming to catch access violations or other process-ending native faults.

OpenTelemetry remains a future option for vendor-neutral traces and performance measurements. It
does not replace the crash dump, symbolication, issue grouping, and triage workflow required here.

### Component boundary

```text
application/core error
        |
        v
ErrorReporter trait -----> NullReporter (consent disabled)
        |                 LocalReporter (existing diagnostics remain available)
        v
sanitizer + schema validator
        |
        v
bounded encrypted-at-rest queue when platform support is available
        |
        v
background delivery worker -- HTTPS --> Sentry ingestion
                                          |
release build ----------------------------+--> debug symbols + source context
```

Provider-specific types must not cross the reporter boundary. Core operations emit an oca error
code and structured fields; only the adapter converts a validated report into a provider event.

## Proposed modules and responsibilities

These names are proposed and should be reconciled with the source tree when implementation starts:

- `crates/core/src/platform/error_reporting.rs`
  - stable `ErrorCode`, `ErrorSeverity`, `RecoveryOutcome`, and report schema;
  - path/content sanitizer and allowlist validator;
  - provider-neutral `ErrorReporter` trait;
  - bounded breadcrumb representation and queue envelope;
  - no networking and no dependency on the Sentry SDK.
- `crates/ui/src/app/error_reporting.rs`
  - consent state, post-crash review flow, worker lifecycle, retry/backoff, and queue deletion;
  - Sentry adapter initialization and UI-facing status;
  - non-blocking calls from the central error handler and export worker.
- `crates/ui/src/main.rs`
  - initialize release metadata and reporting before starting the application;
  - extend, but do not replace, the current local panic hook;
  - flush only within a small bounded shutdown budget.
- release workflow
  - publish the exact release identifier and upload matching Rust/native debug symbols;
  - fail the release job when symbol upload or release association is missing;
  - keep provider administration tokens in CI secrets, never in the client binary.
- native crash integration, after the Rust phase
  - initialize and package the Crashpad handler;
  - upload minidumps on the next safe launch instead of doing network work in the crashing process;
  - cover `avbridge` C frames and Windows fast-fail behavior in platform tests.

## Remote report contract

Every queued record uses a versioned envelope. Unknown schema versions are quarantined or dropped,
never loosely deserialized and forwarded.

Required fields:

- `schema_version`;
- random `event_id` and ephemeral `session_id`;
- canonical `release`, build channel, target triple, application version, and commit identifier;
- operating-system name/version, CPU architecture, locale, and whether the build is portable or
  installed;
- stable `error_code`, severity, operation, recovery outcome, and whether the action was retried;
- sanitized Rust stack trace or native minidump reference, according to the phase;
- a bounded list of structured breadcrumbs;
- FFmpeg, GStreamer, GPU vendor/driver, and encoder identifiers only when they are already
  available without probing user data.

Optional diagnostic dimensions must be coarse and bounded, for example media container family,
codec identifier, resolution bucket, clip-count bucket, and export stage. Do not include file
names, project names, paths, arbitrary codec metadata, filter expressions containing source data,
or raw command lines.

Grouping should prefer `error_code + top application frames + operation`. Human-readable messages
are presentation details and must not be the stable grouping key.

## Data minimization and sanitization

Remote reporting uses an allowlist, not a blacklist. A field absent from the contract is rejected.

Allowed examples:

- oca-owned enums and stable numeric buckets;
- build and dependency versions;
- non-user-controlled source module/function names from symbolicated stacks;
- bounded state transitions such as `export_started`, `encoder_selected`, and `retry_failed`;
- boolean feature flags that do not identify a project or user.

Forbidden examples:

- full or partial filesystem paths, file names, home directories, usernames, and volume labels;
- project names, clip labels, markers/comments, caption or transcript text, text-overlay content,
  and generated prompts;
- URLs, query strings, request/response bodies, OAuth data, tokens, cookies, API keys, environment
  variables, and process command lines;
- raw logs or arbitrary error strings;
- media bytes, screenshots, frames, audio samples, thumbnails, waveform data, or model inputs;
- device serial numbers, MAC addresses, email addresses, and stable cross-application identifiers.

The sanitizer must recognize Windows, UNC, macOS, and Linux path forms; URLs; common credential
patterns; and home-directory aliases. It runs before disk persistence and again before delivery.
If sanitization or schema validation fails, the event is discarded locally with a non-sensitive
counter. The original unsafe payload is never written to the remote queue for later inspection.

The client may contain only the provider's intended public ingestion configuration.
Administrative/authentication tokens used for release management or symbol upload remain in CI
secret storage and must never ship in the binary, logs, crash files, or repository.

## Consent and user controls

Local diagnostics and remote error reporting are separate preferences with separate explanations.
Performance analytics, if introduced later, is a third purpose and needs its own choice.

Required behavior:

- Automatic remote reporting defaults to disabled until the user explicitly opts in.
- After a detected crash, the next launch offers **Send once**, **Always send error reports**, and
  **Do not send**.
- Before a one-time send, the user can inspect the exact structured payload. The view must use the
  post-sanitization form that will actually be transmitted.
- Preferences show the reporting state, current queue size, privacy summary, and controls to delete
  queued diagnostics and revoke consent.
- Revocation stops new collection immediately and deletes unsent remote reports. It does not erase
  the existing local log unless the user explicitly requests that separate action.
- The error-reporting UI must remain usable when the provider is unreachable.

Start with ephemeral session identifiers. An anonymous installation identifier may be added only if
operational evidence shows that crash-free-install metrics are necessary, with a documented
retention period and a reset control.

## Queue and delivery behavior

- Enqueue on a dedicated non-blocking channel; error paths must not wait for disk or network I/O.
- Persist at most 20 reports or 20 MiB, whichever comes first, and expire reports after seven days.
- Use atomic replacement and a versioned strict schema. Corrupt entries are quarantined or dropped
  without repeatedly crashing startup.
- Prefer operating-system-protected storage. If encryption at rest is unavailable, persist only the
  already-minimized schema and document the platform limitation before release.
- Send only to a fixed HTTPS ingestion allowlist with certificate validation, explicit connection
  and total timeouts, and no automatic cross-origin redirects.
- Retry transient failures with exponential backoff and jitter. Bound attempts and request rate;
  do not retry authentication, schema, or payload-size failures indefinitely.
- Apply both record-count and byte-size limits before serialization, decompression, symbol handling,
  and upload to avoid resource exhaustion from corrupt local state.
- Never delay application exit beyond a small documented flush timeout. Remaining reports stay in
  the queue for the next launch.

## Implementation plan

### ER-01A: Contract, sanitization, and local adapter

1. Add stable error codes for import, probe, preview, autosave, project load/save, export stages,
   update, and plugin/model failures.
2. Add the provider-neutral schema, sanitizer, validator, `NullReporter`, and queue envelope.
3. Adapt the central error handler to emit structured reports without changing user-facing error
   behavior or local diagnostics.
4. Keep raw provider SDK types and raw `TelemetryEvent::Error` out of this path.

**Exit criteria:** no network dependency; unit/property tests prove forbidden data does not survive
sanitization; consent-disabled execution writes no remote queue records.

### ER-01B: Consent and Rust reporting

1. Add consent and one-time post-crash review UI.
2. Add the background queue/delivery worker and Sentry Rust adapter.
3. Capture handled errors and Rust panics while preserving the current local crash file and default
   panic output.
4. Add a remote kill switch limited to delivery, not to local error handling or validation.

**Exit criteria:** a controlled handled error and subprocess panic appear in the correct Sentry
release with sanitized fields; disabling consent produces zero outbound requests.

### ER-01C: Releases and symbolication

1. Define one canonical release identifier used by the binary, Sentry events, packaging, and CI.
2. Upload matching PDB/dSYM/ELF debug information and relevant source context with `sentry-cli`.
3. Preserve private symbols outside public packages while keeping them available for the configured
   retention window.
4. Add release smoke tests that reject mismatched or missing symbols.

**Exit criteria:** a synthetic panic resolves to oca source file/function information for every
supported release target.

### ER-01D: Native crash capture

1. Spike `sentry-native`/Crashpad against supported Rust targets and the packaged application.
2. Package, sign, update, and uninstall the Crashpad handler with the app.
3. Capture a deliberate crash in `avbridge` and confirm symbolication of both Rust and C frames.
4. Queue/upload the minidump on the next safe launch and apply the same consent decision.

**Exit criteria:** each supported platform passes a packaged native-crash test; handler absence or
failure degrades to the existing local crash behavior without preventing startup.

### ER-01E: Operations and rollout

1. Define owners and alerts for a new issue, a release regression, and crash-free session changes.
2. Create triage labels by error code, release, operation, and platform.
3. Set and document event/minidump retention and deletion procedures.
4. Roll out to maintainers, then opted-in beta users, then the stable channel after privacy and
   symbolication audits pass.

## Testing strategy

- Sanitizer table tests for Windows/UNC/macOS/Linux paths, URLs, credentials, Unicode, multiline
  errors, nested fields, oversized inputs, and adversarial near-matches.
- Property/fuzz tests asserting that only schema fields survive and output stays within byte limits.
- Consent tests proving disabled, send-once, always-send, revoke, queue-delete, and payload-preview
  behavior.
- A local mock HTTPS collector for timeout, TLS failure, redirect, retry, rate-limit, malformed
  response, and offline recovery tests. Tests must not contact the production provider.
- Subprocess panic tests because panic hooks and process termination cannot be proven with an
  in-process unit test alone.
- Queue tests for atomic writes, expiration, eviction order, corruption, schema migration, and disk
  full/read-only conditions.
- Release smoke tests for event/release association and symbol resolution on each packaged target.
- Native access-violation/abort tests only after ER-01D, run in isolated subprocesses.
- UI responsiveness measurements with a slow/unreachable collector during playback and export.

## Operational policy

- Triage uses stable error codes; alerts based only on free-form message counts are prohibited.
- Access to provider projects follows least privilege. Maintainers who only triage issues do not
  receive organization administration or release-token permissions.
- Production and development events use separate projects or environments so local failures do not
  pollute release health.
- Retention starts at the shortest operationally useful period and is reviewed before expansion.
- Any discovered sensitive-data leak triggers ingestion disablement, deletion using provider tools,
  sanitizer correction, and a regression test before re-enablement.

## Definition of done

ER-01 is complete only when:

- [ ] handled errors and Rust panics are remotely grouped under exact releases;
- [ ] native crashes are either proven on all supported packaged targets or explicitly remain a
      separately tracked, user-visible limitation;
- [ ] matching symbols are uploaded and verified for every release target;
- [ ] consent-off and revoked states generate no remote traffic or retained remote queue;
- [ ] payload preview shows exactly what is transmitted;
- [ ] forbidden data and oversized/corrupt inputs are covered by automated tests;
- [ ] offline delivery is bounded, expiring, and non-blocking;
- [ ] alert ownership, retention, access control, and incident deletion procedures are documented;
- [ ] `ROADMAP.md` and `matrix/robustness.md` are reconciled with what actually shipped.

## Open decisions

- Sentry organization/project ownership, region, retention, and budget.
- Exact Sentry Rust version after MSRV, feature, and license review.
- Canonical release identifier shared by packaging and CI.
- Whether platform-protected queue encryption is available consistently enough to be mandatory in
  the first release.
- Crashpad binary size, license notices, signing, updater behavior, and per-platform support matrix.
- Whether evidence later justifies a resettable anonymous installation identifier.

## Primary references

- [Sentry Rust SDK](https://github.com/getsentry/sentry-rust)
- [Sentry Native SDK and Crashpad backend](https://github.com/getsentry/sentry-native)
- [Sentry CLI for release and debug-symbol management](https://github.com/getsentry/sentry-cli)
- [Sentry self-hosted deployment guidance](https://github.com/getsentry/develop/blob/master/src/docs/self-hosted/index.mdx)
- [OpenTelemetry Rust status](https://opentelemetry.io/docs/languages/rust/)

[<- back to spec/INDEX.md](../INDEX.md)
