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

//! CF-01 (`spec/architecture/competitive-feature-plan.md`) slice 3 — exact text search across
//! the **whole project media library**, not just the one clip's transcript the panel is showing.
//!
//! The Transcript panel (slice 2) already filters within the loaded `TranscriptDocument` by
//! substring; this module lifts that same search up to every asset in a project that has a
//! persisted transcript sidecar (slice 1). Nothing here is stateful or cached: it re-reads each
//! asset's `.octr` sidecar on demand from the passed-in cache directory, matching how
//! `App::ensure_transcript_loaded_for_preview` already reads one document at a time — there is
//! no per-asset "has transcript" index anywhere in the project model (`MediaAsset` carries no
//! such flag), so the truth lives on disk and this scans the subset that exists. Sidecar files
//! are small (already-gzip-compressed MessagePack), so this stays cheap enough to call per
//! search keystroke on a typical library; it deliberately does not build a persistent index —
//! [`search_transcripts_in_project`] is the only search entry point, and the caller decides how
//! often to invoke it.

use std::path::Path;

use crate::media::MediaAsset;
use crate::transcript::{load_transcript_document, TranscriptWord};

/// One word matched by a project-wide search, plus enough of its own context to present it as a
/// row in the panel and to act on it (`asset_id`/`asset_name` to know which asset and to switch
/// the preview, `word` carries the stable id + media-relative timestamps the transcript path
/// already uses).
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectTranscriptHit {
    pub asset_id: u64,
    pub asset_name: String,
    pub word: TranscriptWord,
}

/// Runs `query` (case-insensitive substring match on word text — the same match the per-clip
/// Transcript panel uses) against every `assets` entry that has a loadable transcript sidecar
/// under `dir`, returning the matching words in asset order (the order `assets` itself is in),
/// then in source-time order within each asset.
///
/// An asset with no sidecar yet, or whose sidecar fails to load or validate, is silently skipped
/// (not an error) — exactly the "`Ok(None)` means no transcript yet" convention
/// [`load_transcript_document`] itself uses; the UI's own per-asset load path logs load failures
/// already, so this function stays total over untrusted/corrupt sidecars instead of aborting a
/// whole-library search on one bad file.
pub fn search_transcripts_in_project(
    dir: &Path,
    assets: &[MediaAsset],
    query: &str,
) -> Vec<ProjectTranscriptHit> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for asset in assets {
        let Ok(Some(document)) = load_transcript_document(dir, asset.id) else {
            continue;
        };
        for word in &document.words {
            if word.text.to_lowercase().contains(&query) {
                hits.push(ProjectTranscriptHit {
                    asset_id: asset.id,
                    asset_name: asset.file_name.clone(),
                    word: word.clone(),
                });
            }
        }
    }
    hits
}

#[cfg(test)]
#[path = "transcript_search/transcript_search_test.rs"]
mod tests;
