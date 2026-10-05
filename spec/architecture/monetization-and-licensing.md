# Monetization and Licensing Architecture

**Feature:** MON-01
**Decision date:** 2026-08-28
**Status:** licensing baseline aligned; subscription implementation not started

This document is the source of truth for Oca's initial commercial model. It defines two editions
only: a useful free editor and one monthly paid plan. It also defines the licensing boundary,
subscription lifecycle, downgrade guarantees, security requirements, metrics, and implementation
sequence.

This is product and engineering guidance, not a substitute for qualified legal, tax,
consumer-protection, or privacy advice in every country where Oca is sold.

## Decision summary

| Edition | Price | Promise |
|---|---:|---|
| **Oca Free** | R$0 | A complete local editor that publishes finished videos without a watermark |
| **Oca Pro** | R$39.90/month | Gameplay automation, local AI, batch productivity, premium content, and official support |

Launch offer: R$29.90/month for each founding subscriber's first 12 successful billing cycles.
The price then becomes the current standard monthly price. There is no annual, lifetime, team,
enterprise, usage-credit, or paid-per-export plan in the initial release.

Pro may be evaluated for seven days without requiring a card. Free remains available after trial,
cancellation, payment failure, or expiration.

## Product principles

1. **Charge for saved time, not usable output.** Free users can finish real work; Pro users finish
   repetitive gameplay work faster.
2. **No watermark or output hostage.** Free exports carry no Oca watermark and are not artificially
   capped below the engine's supported resolution.
3. **Local-first remains real.** Local processing is not converted into artificial AI credits.
4. **Projects belong to the creator.** Losing Pro never deletes, corrupts, hides, or prevents the
   opening of a project.
5. **GPL rights remain intact.** Payment grants official service and convenience; it does not add
   restrictions forbidden by the software license.
6. **Server-authoritative commerce.** The desktop may cache an entitlement but may not decide that a
   purchase exists from locally editable state.
7. **Graceful failure.** Billing, identity, or network outages never block Free editing.
8. **Security fixes are never paywalled.** Supported Free and Pro builds receive the same security
   corrections.

The public positioning is:

> Oca Free lets you edit. Oca Pro edits hours of gameplay much faster.

## Edition boundary

The entitlement boundary is based on workflow productivity. It must not reduce baseline media
quality, project portability, reliability, security, or accessibility.

### Oca Free

Free includes:

- manual timeline editing, cut/trim/move operations, multiple tracks, and multiple sequences;
- media import, proxy generation, autosave, recovery, and ordinary project persistence;
- baseline effects, transitions, text, shapes, and the built-in font catalog;
- loudness normalization and export-that-matches-the-source-bitrate;
- watermark-free export at every resolution and codec supported by the build;
- hardware acceleration when available and validated;
- one active export at a time, with ordinary progress and cancellation;
- project opening, editing, and export without an account while no Pro service is requested;
- security updates, bug fixes, local logs, and opt-in error reporting;
- complete corresponding source and license notices for the distributed GPL build.

### Oca Pro

Pro includes everything in Free plus:

- automatic silence/dead-air detection and reviewed removal;
- correlated game/microphone highlight detection;
- automatic scene-based chapters and chapter-list export;
- one-click batch creation of portrait Shorts;
- Whisper transcription, subtitle generation, and word-highlight timing;
- local background removal, auto-reframe, motion tracking, and text-to-speech;
- advanced audio workflows such as ducking and batch/series loudness consistency;
- multicam editing workflows;
- persistent multi-job export queue and batch export orchestration;
- portable collaboration bundles with proxies;
- premium presets, templates, subtitle styles, overlays, and other identified content;
- official priority support.

Future functionality does not become Pro merely because it is new. It belongs in Pro only when it
primarily automates repetitive work, provides premium content/support, or incurs recurring hosted
cost. Reliability, compatibility, accessibility, security, and recovery remain Free
responsibilities.

### Feature-gate rules

- A gate protects an **action**, not project data.
- Free users may inspect Pro settings already stored in a project.
- The UI may perform a bounded, non-destructive analysis to preview Pro value, such as counting
  highlight candidates, but it must not mutate the timeline before confirmation and entitlement.
- Upgrade prompts state the specific unavailable action and preserve current work.
- There is no hidden export-time gate after the user has completed an edit.
- Premium assets use stable identifiers and remain distinguishable from GPL application code and
  OFL fonts.

## Trial, billing, and price rules

- Standard price: **R$39.90 per month**.
- Founding price: **R$29.90 per month for the first 12 successful billing cycles**.
- Trial: **seven days, no payment card required**.
- Renewal: monthly only.
- Cancellation: self-service and effective at the end of the paid period.
- Payment failure: provider retry policy followed by a clearly communicated grace state.
- Taxes, refunds, invoices, and withdrawal rights follow applicable law and the selected checkout
  provider, then map into Oca's canonical subscription state.
- Price changes are communicated in advance with the new price and effective billing date.

The founding discount is account-bound, not tied to a special binary. Pauses, refunds, disputes,
chargebacks, and discount restoration require explicit server rules; the desktop may not infer them.

## Subscription lifecycle

The server owns canonical state. The desktop consumes a smaller entitlement projection.

| State | Pro actions | Required behavior |
|---|---|---|
| free | Disabled | Full Free editor, no account required |
| trialing | Enabled | Show exact trial end; no card required |
| active | Enabled | Show next billing date and account-management link |
| grace | Temporarily enabled | Explain the issue without interrupting active work |
| past_due | Policy dependent | Preserve work and show the payment recovery path |
| canceled | Enabled until paid-through date | Show the effective downgrade date |
| expired | Disabled for new actions | Apply the downgrade contract |
| revoked | Disabled | Confirmed fraud/security events; auditable server decision only |

Unknown, malformed, contradictory, or expired state defaults to Free for new Pro actions. It never
defaults to deleting data or refusing to open a project.

### Offline use

- A recently verified Pro account receives a signed entitlement valid offline for at most 30 days.
- The token contains only a random account subject, edition, issue/expiry times, schema version, and
  unique token ID. It contains no card data or unnecessary PII.
- The app verifies issuer, audience, signature, algorithm allowlist, time bounds, and schema.
- Clock rollback, replay, or local-file modification must not extend server-issued expiry.
- After offline expiry, the app returns to Free until account verification succeeds.

## Downgrade contract

Cancellation, expiration, logout, or payment failure must not hold projects hostage:

1. Every project continues to open.
2. No media, proxy, cache, transcript, timeline item, or setting is deleted.
3. Pro-generated edits and parameters remain visible.
4. Existing projects remain exportable with their existing result.
5. Creating or materially changing a new Pro-only operation is disabled until Pro is active.
6. Ordinary Free edits remain available around Pro-generated content.
7. The app provides a clear sign-in/renew path without blocking unrelated work.
8. Re-subscribing restores Pro actions without rewriting the project.

Project files may record feature/schema versions needed to interpret an edit, but never store
account identity, billing state, entitlement tokens, or secrets.

## Licensing decision

### Application code

The Oca application and workspace crates are licensed under **GNU GPL-3.0-or-later**. This:

- removes the prior conflict between the root GPLv2 text, GPLv2-or-later source notices, and MIT
  Cargo metadata;
- selects a version permitted by the former GPLv2-or-later notices;
- aligns the distributed application with statically linked eSpeak NG, recorded as
  GPL-3.0-or-later in packaging/bundle-manifest.json;
- permits commercial distribution while preserving recipients' use, study, modification, and
  redistribution rights.

The root LICENSE, Cargo metadata, and Oca-owned source headers must agree. Dependencies, models,
fonts, runtimes, and build tools keep their own licenses and notices.

**Update (2026-10):** the eSpeak NG dependency is gone. Piper voices now get their phonemes from
`crates/core/src/analysis/text_to_speech/pt_br.rs`, a rule-based pt-BR phonemizer written for this
project, so nothing linked into the app is GPL any more (the Rust dependency tree was checked: every
crate is permissive, dual-licensed with a permissive option, or file-level MPL-2.0). The GPL-3.0
choice for Oca's own code is therefore a decision, not a constraint. Still to verify before any
closed-source release: the licenses of the *training data* behind the MODNet and Piper Faber
weights, and patent coverage for H.264/AAC distribution.

### What Pro sells

A Pro subscription may charge for:

- access to official signed builds and update channels;
- Oca-operated account and entitlement services;
- separately licensed premium content that is not a derivative of GPL code;
- official support and compatibility validation;
- future hosted storage, review, collaboration, or compute services when explicitly added.

It may not remove a recipient's GPL rights in a binary already conveyed to them. Subscribers may
obtain and redistribute the exact corresponding source and GPL-covered binaries. The business moat
is the trusted official brand, convenient distribution, service operation, content pipeline,
updates, and support, not secrecy or DRM.

### Separately licensed material

- Built-in fonts retain SIL OFL-1.1 terms and notices.
- Third-party models and runtimes retain the licenses in the bundle manifest.
- Premium templates, overlays, and stock media may use a separate content license only when clearly
  separated from GPL code, with auditable provenance and non-conflicting terms.
- User projects, imported media, and exported videos do not become GPL-covered merely because Oca
  processed them, except when output independently incorporates covered source/assets.
- The Oca name, logo, signing identity, and domains are separate trademark/identity concerns. A
  future trademark policy must prevent modified builds from impersonating official releases while
  permitting truthful nominative use.

### Distribution compliance

Every official binary release must:

1. include the complete unmodified GPLv3 text and third-party notices;
2. identify the exact source tag/commit corresponding to the binary;
3. provide equivalent, durable access to complete corresponding source, including required build
   and packaging scripts;
4. preserve source notices and document Oca modifications;
5. avoid EULA, checkout, or technical terms contradicting GPL permissions;
6. keep premium-content terms separate from the GPL software license;
7. verify manifests and notices before publication.

The release must fail if Cargo package licenses disagree, the root license is missing, an Oca-owned
source file carries a stale version, or binary/source mapping cannot be resolved.

## Account, payment, and entitlement architecture

Provider-specific types stay outside the editor/core domain. A server adapter translates provider
events into canonical subscription state; the app consumes an Oca entitlement contract.

    hosted checkout -> payment provider -> signed webhook -> subscription service
                                                           -> entitlement issuer
    desktop app -> browser sign-in -> entitlement API -> signed entitlement -> OS credential vault

### Payment boundary

- Checkout and card entry are hosted by the payment provider.
- Oca never receives or stores card numbers, security codes, or bank credentials.
- Webhooks require signature verification over the raw body, timestamp/replay checks, strict schema
  validation, idempotency by event ID, and explicit state transitions.
- A webhook is acknowledged only after durable processing or durable queueing.
- Client fields never decide price, discount, paid-through date, or plan.
- Refunds, disputes, cancellations, renewals, and retries are reconciled from authoritative events
  plus periodic server reconciliation.

### Identity and authorization

- Free editing requires no account.
- Pro uses a provider-supported browser flow with PKCE or device flow; the desktop never embeds a
  reusable client secret.
- Every restricted API authenticates the caller and authorizes access to the account owning the
  subscription.
- Public resource identifiers are random UUIDs; client-supplied account IDs are not identity.
- Account linking, e-mail changes, recovery, and deletion use explicit verified workflows.

### Token and secret handling

- Entitlements use asymmetric signatures; the desktop holds only public verification keys.
- Algorithms and key IDs are allowlisted; unknown values fail closed for new Pro actions.
- Refresh credentials and tokens use Windows Credential Manager, macOS Keychain, or Linux Secret
  Service. They never enter project files, prefs, plaintext JSON, command arguments, or URLs.
- Server/provider secrets are injected at runtime and never committed or placed in desktop builds.
- Key rotation includes overlap and emergency revocation without project/media migration.

### Network and error behavior

- Requests use explicit timeouts, bounded bodies, and no automatic redirects unless targets are
  allowlisted.
- Responses are schema/version validated; unknown states are rejected.
- Errors are generic and actionable. Stack traces, provider payloads, tokens, e-mails, and billing
  details are neither displayed nor logged.
- Billing or entitlement downtime never blocks Free operations.

## Privacy and telemetry

Permitted commercial analytics include:

- anonymous Free activation and first successful export;
- upgrade prompt impression by feature category, never project/media content;
- trial start/end and aggregate conversion;
- subscription lifecycle category;
- weekly count of Pro capability use without filenames, text, media, or timeline data;
- aggregate cancellation, refund, and support volume.

Never collect card data, tokens, media, thumbnails, project paths/files, transcript/subtitle text,
voice samples, OAuth credentials, or free-form errors as product analytics. Remote error reporting
remains independently consented and governed by ER-01.

## Success metrics

- Free activation to first successful export;
- weekly active exporters;
- Free-to-trial and Free-to-paid conversion;
- paid users using at least one Pro automation weekly;
- monthly churn, refunds, and chargebacks;
- support hours per subscriber;
- measured or reported editing time saved.

Initial validation target: 3%-5% conversion from active Free users to paying Pro users, treated as a
hypothesis.

| Active Pro subscribers | Gross monthly recurring revenue |
|---:|---:|
| 100 | R$3,990 |
| 500 | R$19,950 |
| 1,000 | R$39,900 |

Gross is not net: track payment fees, taxes, refunds, disputes, support, signing, hosting, and future
cloud compute separately.

## Upgrade experience

Upgrade messaging names time saved instead of threatening output quality:

- “18 highlight candidates found. Review and create them with Oca Pro.”
- “23 minutes of silence can be reviewed for removal with Oca Pro.”
- “This timeline can generate six portrait Shorts with Oca Pro.”

The preview is honest, bounded, and non-destructive. It does not claim an uncomputed result,
silently mutate a project, upload media, or conceal that the action requires Pro.

## Implementation slices

### MON-01A: license and distribution baseline

- [x] Replace the root GPLv2 text with complete GPLv3 text.
- [x] Set workspace package metadata to GPL-3.0-or-later.
- [x] Align Oca-owned source headers to GPLv3-or-later.
- [x] Document commercial/GPL boundaries and corresponding-source obligations.
- [ ] Validate stale headers, Cargo metadata, notices, and binary/source mapping in releases.
- [ ] Obtain qualified legal review before accepting payment.

### MON-01B: local edition policy

- [ ] Define stable feature IDs and one central Free/Pro capability policy.
- [ ] Gate actions at service boundaries, not only by hiding UI controls.
- [ ] Preserve project load/export independently of current entitlement.
- [ ] Test Free, trial, active, grace, canceled, expired, malformed, and offline-expired states.

### MON-01C: identity and entitlement service

- [ ] Select account provider and desktop-compatible authorization flow.
- [ ] Define versioned schemas and server-side ownership checks.
- [ ] Issue asymmetric, short-lived online tokens and at-most-30-day offline entitlements.
- [ ] Integrate OS credential vaults with logout/deletion semantics.
- [ ] Test rotation, clock skew, replay, revocation, offline expiry, and outages.

### MON-01D: monthly billing

- [ ] Select a provider supporting hosted checkout, BRL recurring billing, tax/invoice needs,
      refunds, and self-service cancellation.
- [ ] Implement signature-verified, replay-resistant, idempotent webhooks.
- [ ] Model trial, renewal, failure, grace, cancellation, refund, dispute, and revocation.
- [ ] Periodically reconcile provider state instead of relying only on webhooks.
- [ ] Enforce R$29.90 for the first 12 cycles and R$39.90 standard pricing server-side.

### MON-01E: desktop experience

- [ ] Add account, trial, billing status, renewal, cancellation, and offline-expiry surfaces.
- [ ] Add contextual upgrade prompts without blocking Free work.
- [ ] Implement the downgrade contract and localized accessible messages.
- [ ] Open sign-in/checkout only on allowlisted HTTPS origins in the system browser.

### MON-01F: launch validation

- [ ] Complete legal, privacy, tax, consumer-rights, security, and accessibility reviews.
- [ ] Test purchase, renewal, failure, cancellation, refund, dispute, reinstall, new machine,
      offline, account deletion, and provider outage in staging.
- [ ] Confirm GPL source access from every official binary download surface.
- [ ] Launch trial and founding price behind a reversible server flag.
- [ ] Monitor conversion, churn, refunds, support, and entitlement failures without project data.

## Acceptance criteria

- Free can install, edit, save, reopen, and export without watermark or account.
- Valid Pro works online and throughout the offline entitlement window.
- Editing local state cannot mint or extend a subscription.
- Duplicate/reordered webhooks cannot double-credit, regress, or corrupt state.
- Downgrade never deletes data or prevents project open/export.
- Existing Pro edits survive downgrade and become editable again after renewal.
- Payment data, auth tokens, transcript, paths, and media never enter logs, analytics, crash reports,
  prefs, or project files.
- Every binary exposes exact complete corresponding source and required notices.
- Price, trial end, renewal, downgrade, and cancellation are clear in both locales.
- Billing/identity outages leave Free editing functional.

## Explicit non-goals

- annual, lifetime, family, team, enterprise, education, or regional plans;
- local AI usage credits;
- cloud processing, backup, review, or collaboration;
- proprietary relicensing of the combined desktop binary;
- DRM intended to prevent lawful GPL modification or redistribution;
- mandatory Free accounts;
- watermarking, forced intros/outros, or resolution paywalls;
- selling or uploading media, transcripts, voice samples, or project telemetry.

## Open decisions

- payment/merchant-of-record provider and tax/invoice responsibility;
- identity provider and recovery policy;
- exact payment-failure grace period, distinct from the 30-day offline limit;
- premium content license and redistribution boundary;
- trademark policy and official-build naming;
- support channel and refund policy;
- source archive retention and release evidence format.

## Primary references

- [GNU GPLv3 official text](https://www.gnu.org/licenses/gpl-3.0.txt)
- [GNU GPL FAQ](https://www.gnu.org/licenses/gpl-faq.html)
- [GNU instructions for GPLv3-or-later notices](https://www.gnu.org/licenses/gpl-howto.html)
- [eSpeak NG GPL-3.0-or-later declaration](https://github.com/espeak-ng/espeak-ng)
- [Bundle manifest](../../packaging/bundle-manifest.json)
- [ER-01 client error reporting](client-error-reporting.md)

---

[← back to spec/INDEX.md](../INDEX.md)
