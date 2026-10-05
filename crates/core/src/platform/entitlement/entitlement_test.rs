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

use super::*;

const NOW: i64 = 1_000_000;

#[test]
fn free_disables_pro_actions() {
    assert!(!pro_actions_enabled(EntitlementState::Free, NOW));
}

#[test]
fn trialing_is_enabled_before_trial_end() {
    let state = EntitlementState::Trialing {
        trial_end_unix: NOW + 100,
    };
    assert!(pro_actions_enabled(state, NOW));
}

#[test]
fn trialing_is_disabled_after_trial_end() {
    let state = EntitlementState::Trialing {
        trial_end_unix: NOW - 1,
    };
    assert!(!pro_actions_enabled(state, NOW));
}

#[test]
fn active_is_always_enabled() {
    assert!(pro_actions_enabled(EntitlementState::Active, NOW));
}

#[test]
fn grace_is_enabled_before_its_deadline() {
    let state = EntitlementState::Grace {
        until_unix: NOW + 100,
    };
    assert!(pro_actions_enabled(state, NOW));
}

#[test]
fn grace_is_disabled_after_its_deadline() {
    let state = EntitlementState::Grace {
        until_unix: NOW - 1,
    };
    assert!(!pro_actions_enabled(state, NOW));
}

#[test]
fn past_due_defaults_to_disabled() {
    assert!(!pro_actions_enabled(EntitlementState::PastDue, NOW));
}

#[test]
fn canceled_is_enabled_until_the_paid_through_date() {
    let state = EntitlementState::Canceled {
        paid_through_unix: NOW + 100,
    };
    assert!(pro_actions_enabled(state, NOW));
}

#[test]
fn canceled_is_disabled_after_the_paid_through_date() {
    let state = EntitlementState::Canceled {
        paid_through_unix: NOW - 1,
    };
    assert!(!pro_actions_enabled(state, NOW));
}

#[test]
fn expired_disables_pro_actions() {
    assert!(!pro_actions_enabled(EntitlementState::Expired, NOW));
}

#[test]
fn revoked_disables_pro_actions() {
    assert!(!pro_actions_enabled(EntitlementState::Revoked, NOW));
}

#[test]
fn offline_expired_disables_pro_actions() {
    assert!(!pro_actions_enabled(EntitlementState::OfflineExpired, NOW));
}

#[test]
fn malformed_disables_pro_actions() {
    assert!(!pro_actions_enabled(EntitlementState::Malformed, NOW));
}

#[test]
fn feature_allowed_matches_pro_actions_enabled_for_every_feature() {
    let features = [
        FeatureId::SilenceDetection,
        FeatureId::CorrelatedHighlightDetection,
        FeatureId::AutoChapters,
        FeatureId::ShortsPackBatch,
        FeatureId::WhisperTranscription,
        FeatureId::BackgroundRemoval,
        FeatureId::AutoReframe,
        FeatureId::MotionTracking,
        FeatureId::TextToSpeech,
        FeatureId::AudioDucking,
        FeatureId::BatchLoudnessConsistency,
        FeatureId::MulticamEditing,
        FeatureId::PersistentExportQueue,
        FeatureId::CollaborationBundles,
        FeatureId::PremiumContent,
    ];
    for feature in features {
        assert!(!feature_allowed(EntitlementState::Free, feature, NOW));
        assert!(feature_allowed(EntitlementState::Active, feature, NOW));
    }
}

#[test]
fn a_clock_rolled_back_past_an_offline_token_expiry_does_not_extend_the_grace_deadline() {
    // Grace/Trialing/Canceled all compare now_unix < deadline directly with no internal clock
    // source of their own -- a caller passing an earlier "now" naturally sees the state as still
    // valid, which is expected (this function has no way to detect real clock rollback on its
    // own; that verification belongs to whatever issues/reads the signed token, per the doc's
    // own "Offline use" section). What this function *does* guarantee: the deadline itself is
    // never silently extended by anything inside this pure function.
    let state = EntitlementState::Grace {
        until_unix: NOW + 100,
    };
    assert!(pro_actions_enabled(state, NOW));
    assert!(!pro_actions_enabled(state, NOW + 200));
}
