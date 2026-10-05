// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! MON-01B: local edition policy
//! ([`spec/architecture/monetization-and-licensing.md`](../../../../spec/architecture/monetization-and-licensing.md)'s
//! own "Define stable feature IDs and one central Free/Pro capability policy").
//!
//! **Deliberately not wired to gate anything yet.** [`MON-01C`](../../../../spec/architecture/monetization-and-licensing.md#mon-01c-identity-and-entitlement-service)
//! (the identity/entitlement service) hasn't shipped — there is no real way for a user to
//! *become* Pro yet, and no purchase flow to point an upgrade prompt at. Wiring [`feature_
//! allowed`] into a real `ui` call site today would silently take already-working functionality
//! away from every current user with no way to unlock it back — a real regression, not a
//! feature. So this module is the local policy *engine* only: stable [`FeatureId`]s, the
//! [`EntitlementState`] state machine exactly as `monetization-and-licensing.md`'s own
//! "Subscription lifecycle" table defines it, and [`feature_allowed`]/[`pro_actions_enabled`] as
//! pure functions — real, tested, and ready for a `ui`-side gate once MON-01C exists to feed it
//! an actual state instead of a hardcoded [`EntitlementState::Free`].
//!
//! The doc's own "Feature-gate rules" and "Downgrade contract" sections constrain what this
//! module is even for: a gate protects an *action* (starting or materially changing a Pro-only
//! operation), never project data — "every project continues to open," "existing projects
//! remain exportable with their existing result," "Free users may inspect Pro settings already
//! stored in a project." So [`feature_allowed`] is deliberately not consulted by, and must never
//! be consulted by, project load/save or export — those stay unconditional. This module has no
//! opinion on load/export at all; it only answers "may this specific Pro action start."

use serde::{Deserialize, Serialize};

/// One stable, named Pro-only capability — the doc's own "Oca Pro" feature list
/// (`monetization-and-licensing.md`'s "Edition boundary" section), given a fixed identifier so a
/// future gate/upgrade-prompt/telemetry call site never has to restate the list itself. Every
/// variant here is Pro-only as of this slice — there's no `Free`-tier variant, since gating a
/// Free feature would contradict the doc's own "Free can install, edit, save, reopen, and export
/// without watermark or account" acceptance criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureId {
    SilenceDetection,
    CorrelatedHighlightDetection,
    AutoChapters,
    ShortsPackBatch,
    WhisperTranscription,
    BackgroundRemoval,
    AutoReframe,
    MotionTracking,
    TextToSpeech,
    AudioDucking,
    BatchLoudnessConsistency,
    MulticamEditing,
    PersistentExportQueue,
    CollaborationBundles,
    PremiumContent,
}

/// A point-in-time projection of a user's subscription, exactly as `monetization-and-
/// licensing.md`'s own "Subscription lifecycle" table names and orders its states — the desktop
/// "consumes a smaller entitlement projection" the doc's own wording calls for, not the full
/// server-side billing record. `Malformed` covers the doc's own "unknown, malformed,
/// contradictory... state defaults to Free for new Pro actions" catch-all for a projection this
/// build can't make sense of (a future schema version, a corrupt cached token, etc.) — a real,
/// named state rather than a silent panic/default.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum EntitlementState {
    /// No account, or an account with no Pro subscription. Every Pro action disabled.
    Free,
    /// The doc's 7-day, no-card trial. Enabled while `trial_end_unix` hasn't passed.
    Trialing { trial_end_unix: i64 },
    /// A confirmed active paid subscription.
    Active,
    /// A payment hiccup the provider is still retrying — "temporarily enabled" per the doc's own
    /// table, while `until_unix` hasn't passed, so ongoing work isn't interrupted mid-edit.
    Grace { until_unix: i64 },
    /// Payment failed and the provider's retry policy is still running; the doc calls this
    /// "policy dependent" rather than naming a fixed behavior — this module defaults it to
    /// disabled (same as the doc's own unknown/malformed/expired catch-all) until a real policy
    /// is decided; work is still never at risk (see this module's own doc comment on the
    /// downgrade contract).
    PastDue,
    /// Self-service cancellation — the doc's own "enabled until paid-through date."
    Canceled { paid_through_unix: i64 },
    /// The paid-through (or trial) period has ended with no renewal.
    Expired,
    /// A confirmed fraud/security revocation — server-decided, never inferred locally.
    Revoked,
    /// A cached offline entitlement token whose signed validity window
    /// (`monetization-and-licensing.md`'s own "Offline use" section, at most 30 days) has
    /// passed — "the app returns to Free until account verification succeeds."
    OfflineExpired,
    /// A projection this build couldn't validate (unknown schema version, bad signature, a
    /// contradictory combination of fields) — see this type's own doc comment.
    Malformed,
}

/// Whether Pro actions are enabled right now, per `monetization-and-licensing.md`'s own
/// "Subscription lifecycle" table. `now_unix` is threaded in explicitly (never read from the
/// system clock internally) so this stays a pure, deterministic function — the same discipline
/// the doc's own "clock rollback... must not extend server-issued expiry" requirement implies:
/// a caller controls exactly what "now" means and can't be fooled by mutating this function's
/// internal clock source, because there isn't one.
pub fn pro_actions_enabled(state: EntitlementState, now_unix: i64) -> bool {
    match state {
        EntitlementState::Free => false,
        EntitlementState::Trialing { trial_end_unix } => now_unix < trial_end_unix,
        EntitlementState::Active => true,
        EntitlementState::Grace { until_unix } => now_unix < until_unix,
        EntitlementState::PastDue => false,
        EntitlementState::Canceled { paid_through_unix } => now_unix < paid_through_unix,
        EntitlementState::Expired => false,
        EntitlementState::Revoked => false,
        EntitlementState::OfflineExpired => false,
        EntitlementState::Malformed => false,
    }
}

/// Whether `feature` may start/materially change right now under `state` — currently identical
/// to [`pro_actions_enabled`] since every [`FeatureId`] is Pro-only, kept as its own named
/// function (rather than callers using `pro_actions_enabled` directly) so a future Free-tier
/// carve-out for one specific feature has one call site to change, not every caller.
pub fn feature_allowed(state: EntitlementState, _feature: FeatureId, now_unix: i64) -> bool {
    pro_actions_enabled(state, now_unix)
}

#[cfg(test)]
#[path = "entitlement/entitlement_test.rs"]
mod tests;
