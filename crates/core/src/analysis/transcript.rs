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

//! CF-01 ("Transcript-based editing and speech cleanup",
//! `spec/architecture/competitive-feature-plan.md`) slice 1 — persist a media-relative
//! transcript document with stable word IDs, timestamps, and confidence, as an editing surface
//! independent of subtitle rendering. Deliberately its own type, not a rename of
//! [`crate::transcribe::TranscribeSegment`]/`TranscribeWord`: those are Whisper's own raw
//! per-inference output (segments grouping words, no stable id, no schema version); a
//! [`TranscriptDocument`] is the persisted, word-flattened, id-stable editing surface built from
//! them once — CF-01's own plan explicitly calls for keeping "subtitle text and editorial
//! transcript separate," so this type has no rendering-only fields ([`crate::timeline::TextClip`]
//! styling, word-highlight color, ...) at all.
//!
//! Persisted as a sidecar next to the project file (`myproject.ocproj` ->
//! `.myproject_transcripts/asset_<id>.octr`), one file per [`crate::media::MediaAsset`] —
//! [`crate::persistence::to_octr_bytes`]/[`from_octr_bytes`] frame it the same magic-bytes +
//! version + size-bounded-gzip-compressed-MessagePack way every other oca binary format already
//! does (`crate::persistence`'s own doc comment), which is also why explicit per-field
//! validation on load (CF-01's shared security requirement: treat sidecars as untrusted input)
//! is layered on top in [`TranscriptDocument::validate`] rather than trusted implicitly —
//! `#[serde(default)]`-driven forward compatibility only proves the bytes *parsed*, not that
//! their contents make sense (a negative timestamp, `start > end`, `confidence` outside
//! `0.0..=1.0`, or a duplicate word id could all still deserialize cleanly).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::project::Project;
use crate::transcribe::TranscribeSegment;

/// Bumped only if [`TranscriptDocument`]'s own field layout changes in a way
/// `#[serde(default)]` can't absorb (a field removed or its meaning changed, not just a new
/// optional field added) — independent of [`crate::persistence`]'s own binary framing version,
/// which covers the on-disk container, not this document's semantic shape.
pub const TRANSCRIPT_SCHEMA_VERSION: u32 = 1;

/// One transcribed word, flattened out of whisper.cpp's segment grouping — [`Self::id`] is
/// stable across edits to this document (assigned once, at build time, never renumbered), so a
/// caller (a future transcript panel, a proposed-edit list) can reference a specific word across
/// frames/saves without depending on its position in [`TranscriptDocument::words`] staying fixed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptWord {
    pub id: u64,
    pub text: String,
    /// Seconds from the start of the source media (not the timeline) — this document is
    /// media-relative, per CF-01's own wording, so it stays valid across every sequence/timeline
    /// position the asset happens to be cut into.
    pub start_secs: f64,
    pub end_secs: f64,
    /// `0.0..=1.0` — see [`crate::transcribe::TranscribeWord::confidence`] for where this comes
    /// from.
    pub confidence: f32,
    /// `None` — this Whisper model build has no diarization, so every word is speaker-less for
    /// now. The field exists (schema-forward, `#[serde(default)]`) so a future diarization pass
    /// can populate it without another schema bump; CF-01's own plan says "speaker when
    /// available," not "always."
    #[serde(default)]
    pub speaker: Option<String>,
}

/// A persisted, editable transcript for one [`crate::media::MediaAsset`] — see this module's
/// own doc comment for the full design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptDocument {
    pub schema_version: u32,
    pub asset_id: u64,
    /// Whisper language code used for this transcription (e.g. `"pt"`), or `None` if
    /// auto-detected — kept for display/re-transcription context, not read by anything that
    /// affects editing.
    #[serde(default)]
    pub language: Option<String>,
    pub words: Vec<TranscriptWord>,
}

/// Why a loaded [`TranscriptDocument`] was rejected — see [`TranscriptDocument::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TranscriptValidationError {
    /// `schema_version` is newer than [`TRANSCRIPT_SCHEMA_VERSION`] — this build doesn't know
    /// this document's shape and shouldn't guess at it.
    UnsupportedSchemaVersion(u32),
    /// Two words share the same `id` — breaks the "stable, unique reference" contract every
    /// future caller depends on.
    DuplicateWordId(u64),
    /// A word's own `start_secs`/`end_secs`/`confidence` is out of range (negative timing,
    /// `start >= end`, non-finite, or confidence outside `0.0..=1.0`).
    InvalidWord { id: u64, reason: &'static str },
}

impl std::fmt::Display for TranscriptValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptValidationError::UnsupportedSchemaVersion(v) => {
                write!(
                    f,
                    "transcript schema version {v} is newer than this build supports"
                )
            }
            TranscriptValidationError::DuplicateWordId(id) => {
                write!(f, "duplicate transcript word id {id}")
            }
            TranscriptValidationError::InvalidWord { id, reason } => {
                write!(f, "transcript word {id} is invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for TranscriptValidationError {}

impl TranscriptDocument {
    /// Flattens `segments` (whisper.cpp's own segment-grouped output) into one word list,
    /// assigning each word a stable, sequential id starting at `1` (`0` reserved as a caller-side
    /// "no word" sentinel, same convention `ClipInstance::composite_id`'s `Option<u64>` avoids
    /// needing in the first place — kept here anyway since a plain `u64` id, not an `Option`, is
    /// simpler for a caller that always has a real word). Word order is preserved exactly as
    /// whisper.cpp produced it (already chronological within and across segments).
    pub fn from_transcribe_segments(
        asset_id: u64,
        language: Option<String>,
        segments: &[TranscribeSegment],
    ) -> Self {
        let words = segments
            .iter()
            .flat_map(|segment| &segment.words)
            .enumerate()
            .map(|(index, word)| TranscriptWord {
                id: index as u64 + 1,
                text: word.text.clone(),
                start_secs: word.start_secs,
                end_secs: word.end_secs,
                confidence: word.confidence,
                speaker: None,
            })
            .collect();
        Self {
            schema_version: TRANSCRIPT_SCHEMA_VERSION,
            asset_id,
            language,
            words,
        }
    }

    /// Checks every field-level invariant this module's own doc comment describes — call after
    /// [`load_transcript_document`] (which already calls this) or after deserializing a document
    /// from any other untrusted source before treating it as valid. Returns the *first*
    /// violation found rather than collecting all of them — a caller distinguishing "corrupt,
    /// reject/regenerate" from "fine" doesn't need an exhaustive list, and stopping early avoids
    /// scanning a maliciously huge `words` list further than necessary.
    pub fn validate(&self) -> Result<(), TranscriptValidationError> {
        if self.schema_version > TRANSCRIPT_SCHEMA_VERSION {
            return Err(TranscriptValidationError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        let mut seen_ids = std::collections::HashSet::with_capacity(self.words.len());
        for word in &self.words {
            if !seen_ids.insert(word.id) {
                return Err(TranscriptValidationError::DuplicateWordId(word.id));
            }
            if !word.start_secs.is_finite() || word.start_secs < 0.0 {
                return Err(TranscriptValidationError::InvalidWord {
                    id: word.id,
                    reason: "start_secs must be finite and non-negative",
                });
            }
            if !word.end_secs.is_finite() || word.end_secs <= word.start_secs {
                return Err(TranscriptValidationError::InvalidWord {
                    id: word.id,
                    reason: "end_secs must be finite and after start_secs",
                });
            }
            if !word.confidence.is_finite() || !(0.0..=1.0).contains(&word.confidence) {
                return Err(TranscriptValidationError::InvalidWord {
                    id: word.id,
                    reason: "confidence must be finite and within 0.0..=1.0",
                });
            }
        }
        Ok(())
    }
}

/// Where a project's transcript-document sidecars are cached — a hidden sibling folder next to
/// the project file, same convention [`crate::proxy::cache_dir_for_project`]/
/// [`crate::nested_sequence::cache_dir_for_project`] both already use, kept in its own folder
/// (not shared with either) since transcripts are neither a regenerable-from-source-alone
/// editing proxy nor a rendered-file cache — they're the one artifact of the three actually
/// worth a user backing up (Whisper inference isn't free).
pub fn cache_dir_for_project(project: &Project) -> PathBuf {
    match &project.file_path {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            path.with_file_name(format!(".{stem}_transcripts"))
        }
        None => std::env::temp_dir().join("oca_unsaved_transcripts"),
    }
}

/// The path [`save_transcript_document`]/[`load_transcript_document`] use for `asset_id` inside
/// `dir` — pure, so it's usable to check existence without touching disk.
pub fn transcript_path(dir: &Path, asset_id: u64) -> PathBuf {
    dir.join(format!("asset_{asset_id}.octr"))
}

#[derive(Debug)]
pub enum TranscriptStorageError {
    Io(std::io::Error),
    Persist(crate::persistence::PersistError),
    Validation(TranscriptValidationError),
}

impl std::fmt::Display for TranscriptStorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TranscriptStorageError::Io(e) => write!(f, "failed to access transcript file: {e}"),
            TranscriptStorageError::Persist(e) => write!(f, "{e}"),
            TranscriptStorageError::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for TranscriptStorageError {}

/// Saves `document` under `dir` (see [`transcript_path`]), creating `dir` if needed.
pub fn save_transcript_document(
    dir: &Path,
    document: &TranscriptDocument,
) -> Result<PathBuf, TranscriptStorageError> {
    std::fs::create_dir_all(dir).map_err(TranscriptStorageError::Io)?;
    let path = transcript_path(dir, document.asset_id);
    let bytes =
        crate::persistence::to_octr_bytes(document).map_err(TranscriptStorageError::Persist)?;
    crate::persistence::write_atomic(&path, &bytes).map_err(TranscriptStorageError::Io)?;
    Ok(path)
}

/// Loads and validates the transcript document for `asset_id` under `dir`, if one exists.
/// `Ok(None)` (not an error) when the file doesn't exist — no transcript has been generated for
/// this asset yet, an ordinary state, not a failure.
pub fn load_transcript_document(
    dir: &Path,
    asset_id: u64,
) -> Result<Option<TranscriptDocument>, TranscriptStorageError> {
    let path = transcript_path(dir, asset_id);
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(&path).map_err(TranscriptStorageError::Io)?;
    let document: TranscriptDocument =
        crate::persistence::from_octr_bytes(&bytes).map_err(TranscriptStorageError::Persist)?;
    document
        .validate()
        .map_err(TranscriptStorageError::Validation)?;
    Ok(Some(document))
}

#[cfg(test)]
#[path = "transcript/transcript_test.rs"]
mod tests;
