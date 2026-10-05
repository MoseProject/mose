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

//! CF-02 (`spec/architecture/competitive-feature-plan.md`): a versioned JSON sidecar format
//! carrying recorder/game events (kills, deaths, objectives, manual bookmarks) alongside a
//! recording, so highlight detection can combine them with the existing audio-spike score
//! ([`crate::highlight_detection`]) instead of relying on audio alone.
//!
//! This slice covers the sidecar schema, validation, idempotent import as timeline markers, and
//! event-based/combined highlight scoring — CF-02's own implementation-slices 2 and 4. **Not
//! done**: slice 1's automated watched-folder *import* service (distinct from
//! [`crate::watched_folder`]'s already-shipped noise-cleanup watcher, which stability-detects and
//! re-encodes a file, not brings it into a project's media library), slice 3's OBS/Medal/
//! Outplayed format adapters (would mean reverse-engineering an external tool's own export
//! format without a documented spec to verify against — deliberately not guessed at), and slice
//! 5's per-game event allowlists. A sidecar today is hand-authored or produced by an external
//! script against this module's own schema and imported manually — "user-provided sidecars," the
//! doc's own fallback path when a dedicated adapter doesn't exist yet.
//!
//! `source_media_filename` deliberately identifies the recording by filename alone, never an
//! absolute path — matching [`crate::collab_bundle`]'s own portability reasoning: a project
//! (and its sidecars) must reopen correctly on a machine where the recording lives under a
//! different absolute path.

use serde::{Deserialize, Serialize};

use crate::timeline::{Marker, MarkerKind, Timeline};

/// The only schema version this build understands. A sidecar declaring any other value is
/// rejected outright rather than guessed at — see [`EventSidecar::validate`].
pub const CURRENT_SCHEMA_VERSION: u32 = 1;

/// A closed set of recognized event kinds. Deliberately not open-ended/`#[serde(other)]`-
/// tolerant like [`crate::timeline::TextFontFamily`]'s font-fallback case: CF-02's own acceptance
/// criteria require an *unrecognized* event kind to be rejected with an actionable error, not
/// silently normalized — a sidecar naming a kind this build doesn't know about is far more likely
/// a typo or a newer/foreign export format than a case with a safe default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameplayEventKind {
    Kill,
    Death,
    Assist,
    Objective,
    Bookmark,
}

impl GameplayEventKind {
    pub const ALL: [Self; 5] = [
        Self::Kill,
        Self::Death,
        Self::Assist,
        Self::Objective,
        Self::Bookmark,
    ];
}

/// One recorder/game event within a [`EventSidecar`]. Timestamps are relative to the *source
/// media file*, not the timeline — the same convention [`crate::transcript::TranscriptDocument`]
/// uses for the same reason (a sidecar describes the recording, not any particular project's
/// edit of it).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameplayEvent {
    pub kind: GameplayEventKind,
    pub source_timestamp_secs: f64,
    /// `[0.0, 1.0]` — how confident the source (recorder heuristic, game API, manual bookmark)
    /// is that this moment matters. A manual bookmark is always `1.0`.
    pub confidence: f32,
    /// Extra seconds of lead-in/lead-out this specific event asks for, overriding whatever
    /// default a caller would otherwise use (e.g. a "death" event might want more pre-roll than
    /// a "kill" event to show the setup). `None` means "use the caller's default."
    #[serde(default)]
    pub pre_roll_secs: Option<f32>,
    #[serde(default)]
    pub post_roll_secs: Option<f32>,
}

/// A versioned sidecar: one recording's worth of [`GameplayEvent`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventSidecar {
    pub schema_version: u32,
    /// The recording's filename (e.g. `"2026-08-29_ranked.mp4"`), never an absolute path — see
    /// this module's own doc comment.
    pub source_media_filename: String,
    /// Optional free-text game identifier (e.g. `"valorant"`), matched exactly (case-sensitive)
    /// against a configured [`GameEventAllowlist::game_id`] — CF-02 slice 5's own "expose
    /// per-game event allowlists" hook. `#[serde(default)]` so a sidecar produced before this
    /// field existed still parses, with no allowlist ever applying to it.
    #[serde(default)]
    pub game_id: Option<String>,
    pub events: Vec<GameplayEvent>,
}

/// Why an [`EventSidecar`] failed [`EventSidecar::validate`] — each variant's [`Display`](
/// std::fmt::Display) is the actionable message CF-02's own acceptance criteria require (name
/// the field, name the bad value), never a bare parse error.
#[derive(Debug, Clone, PartialEq)]
pub enum SidecarValidationError {
    UnsupportedSchemaVersion { found: u32 },
    InvalidSourceMediaFilename(String),
    InvalidTimestamp { index: usize, value: f64 },
    InvalidConfidence { index: usize, value: f32 },
    InvalidPreRoll { index: usize, value: f32 },
    InvalidPostRoll { index: usize, value: f32 },
}

impl std::fmt::Display for SidecarValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchemaVersion { found } => write!(
                f,
                "unsupported schema_version {found} (this build only understands \
                 {CURRENT_SCHEMA_VERSION})"
            ),
            Self::InvalidSourceMediaFilename(name) => write!(
                f,
                "source_media_filename must be a bare filename, not empty or a path (got \
                 {name:?})"
            ),
            Self::InvalidTimestamp { index, value } => write!(
                f,
                "event {index}: source_timestamp_secs must be finite and non-negative (got \
                 {value})"
            ),
            Self::InvalidConfidence { index, value } => write!(
                f,
                "event {index}: confidence must be within [0.0, 1.0] (got {value})"
            ),
            Self::InvalidPreRoll { index, value } => write!(
                f,
                "event {index}: pre_roll_secs must be finite and non-negative (got {value})"
            ),
            Self::InvalidPostRoll { index, value } => write!(
                f,
                "event {index}: post_roll_secs must be finite and non-negative (got {value})"
            ),
        }
    }
}

impl std::error::Error for SidecarValidationError {}

impl EventSidecar {
    /// Parses and validates `json` in one step — the only entry point external callers should
    /// use, so an accepted [`EventSidecar`] is always known-valid.
    pub fn parse_and_validate(json: &str) -> Result<Self, SidecarParseOrValidationError> {
        let sidecar: EventSidecar =
            serde_json::from_str(json).map_err(SidecarParseOrValidationError::Parse)?;
        sidecar.validate()?;
        Ok(sidecar)
    }

    /// Checks every field CF-02's acceptance criteria call out: schema version, filename
    /// portability, and each event's timestamp/confidence/roll ranges. `kind` itself never needs
    /// checking here — an unrecognized kind already fails at JSON deserialization (a plain,
    /// if less specifically worded, actionable [`serde_json::Error`]), since
    /// [`GameplayEventKind`] is a closed enum.
    pub fn validate(&self) -> Result<(), SidecarValidationError> {
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SidecarValidationError::UnsupportedSchemaVersion {
                found: self.schema_version,
            });
        }
        let filename_is_bare = !self.source_media_filename.is_empty()
            && !self.source_media_filename.contains('/')
            && !self.source_media_filename.contains('\\')
            && self.source_media_filename != "."
            && self.source_media_filename != "..";
        if !filename_is_bare {
            return Err(SidecarValidationError::InvalidSourceMediaFilename(
                self.source_media_filename.clone(),
            ));
        }
        for (index, event) in self.events.iter().enumerate() {
            if !event.source_timestamp_secs.is_finite() || event.source_timestamp_secs < 0.0 {
                return Err(SidecarValidationError::InvalidTimestamp {
                    index,
                    value: event.source_timestamp_secs,
                });
            }
            if !(0.0..=1.0).contains(&event.confidence) {
                return Err(SidecarValidationError::InvalidConfidence {
                    index,
                    value: event.confidence,
                });
            }
            if let Some(pre_roll) = event.pre_roll_secs {
                if !pre_roll.is_finite() || pre_roll < 0.0 {
                    return Err(SidecarValidationError::InvalidPreRoll {
                        index,
                        value: pre_roll,
                    });
                }
            }
            if let Some(post_roll) = event.post_roll_secs {
                if !post_roll.is_finite() || post_roll < 0.0 {
                    return Err(SidecarValidationError::InvalidPostRoll {
                        index,
                        value: post_roll,
                    });
                }
            }
        }
        Ok(())
    }
}

/// Either half of [`EventSidecar::parse_and_validate`]'s failure — kept distinct from
/// [`SidecarValidationError`] alone since a malformed-JSON failure and a well-formed-but-invalid
/// failure are different classes of problem for a caller to report.
#[derive(Debug)]
pub enum SidecarParseOrValidationError {
    Parse(serde_json::Error),
    Validation(SidecarValidationError),
}

impl std::fmt::Display for SidecarParseOrValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "could not parse event sidecar JSON: {e}"),
            Self::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SidecarParseOrValidationError {}

impl From<SidecarValidationError> for SidecarParseOrValidationError {
    fn from(e: SidecarValidationError) -> Self {
        Self::Validation(e)
    }
}

/// Two markers are treated as "the same already-imported event" when they share a kind and
/// land within this many seconds of each other — matches on close-enough position rather than
/// exact float equality, since a re-imported sidecar's timestamps round-trip through the same
/// arithmetic and should match exactly in practice, but this stays robust to a caller re-deriving
/// `position_secs` slightly differently (e.g. a clip whose `source_in_secs` shifted by a trim).
const IMPORT_DEDUP_EPSILON_SECS: f64 = 0.05;

fn marker_label_for(kind: GameplayEventKind) -> &'static str {
    match kind {
        GameplayEventKind::Kill => "Kill",
        GameplayEventKind::Death => "Death",
        GameplayEventKind::Assist => "Assist",
        GameplayEventKind::Objective => "Objective",
        GameplayEventKind::Bookmark => "Bookmark",
    }
}

/// A user-configured per-game filter (CF-02 slice 5, "expose per-game event allowlists and
/// pre/post-roll settings") restricting which [`GameplayEventKind`]s get imported from a sidecar
/// whose [`EventSidecar::game_id`] matches [`Self::game_id`] exactly (case-sensitive, no fuzzy
/// matching — a typo'd `game_id` should surface as "no profile applied," never silently misapply
/// another game's roll defaults), plus default pre/post-roll seconds for events that don't
/// specify their own. Persisted app-wide (`ui::PrefsState`), not per-project, since the whole
/// point is reusing the same profile across every sidecar for the same game.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameEventAllowlist {
    pub game_id: String,
    /// Which event kinds pass [`apply_game_event_allowlist`] — an empty list is a deliberate
    /// "import nothing from this game" configuration, not "no restriction" (that's what leaving
    /// a game unconfigured, i.e. no matching [`GameEventAllowlist`] at all, already means).
    pub allowed_kinds: Vec<GameplayEventKind>,
    pub default_pre_roll_secs: Option<f32>,
    pub default_post_roll_secs: Option<f32>,
}

/// Filters `events` down to [`GameEventAllowlist::allowed_kinds`] and fills each passing event's
/// `pre_roll_secs`/`post_roll_secs` from the allowlist's own defaults when the event didn't
/// specify one — an event's own `Some(_)` override always wins. `allowlist: None` (no configured
/// profile matched this sidecar's `game_id`) returns `events` unchanged, exactly CF-02's own
/// pre-slice-5 behavior, so an unconfigured game is never silently restricted.
pub fn apply_game_event_allowlist(
    events: &[GameplayEvent],
    allowlist: Option<&GameEventAllowlist>,
) -> Vec<GameplayEvent> {
    let Some(allowlist) = allowlist else {
        return events.to_vec();
    };
    events
        .iter()
        .filter(|event| allowlist.allowed_kinds.contains(&event.kind))
        .cloned()
        .map(|mut event| {
            if event.pre_roll_secs.is_none() {
                event.pre_roll_secs = allowlist.default_pre_roll_secs;
            }
            if event.post_roll_secs.is_none() {
                event.post_roll_secs = allowlist.default_post_roll_secs;
            }
            event
        })
        .collect()
}

/// Imports `events` (already validated — see [`EventSidecar::parse_and_validate`]) as
/// [`MarkerKind::Highlight`] markers on `timeline`, mapping each event's source-relative
/// `source_timestamp_secs` through `to_timeline_secs` (the caller's own source-to-timeline
/// mapping for whichever clip/offset the recording was placed at — this module has no opinion on
/// timeline placement). Idempotent: an event that already has a same-kind marker within
/// [`IMPORT_DEDUP_EPSILON_SECS`] is skipped rather than duplicated, so importing the same sidecar
/// twice (or once per clip instance of the same recording) is a no-op the second time. Returns
/// the number of markers actually added.
pub fn import_events_as_markers(
    timeline: &mut Timeline,
    events: &[GameplayEvent],
    to_timeline_secs: impl Fn(f64) -> f64,
) -> usize {
    let mut added = 0;
    for event in events {
        let position_secs = to_timeline_secs(event.source_timestamp_secs);
        let label = marker_label_for(event.kind);
        let already_imported = timeline.markers.iter().any(|marker: &Marker| {
            marker.kind == MarkerKind::Highlight
                && marker.label == label
                && (marker.position_secs - position_secs).abs() <= IMPORT_DEDUP_EPSILON_SECS
        });
        if already_imported {
            continue;
        }
        let id = timeline.add_marker(position_secs, MarkerKind::Highlight);
        if let Some(marker) = timeline.marker_mut(id) {
            marker.label = label.to_string();
        }
        added += 1;
    }
    added
}

#[cfg(test)]
#[path = "gameplay_events/gameplay_events_test.rs"]
mod tests;
