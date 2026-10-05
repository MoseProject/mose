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

//! CF-01 (`spec/architecture/competitive-feature-plan.md`) slices 4-5 — build a *proposed-edit
//! list* from a [`crate::transcript::TranscriptDocument`] (slice 1) and apply the accepted
//! proposals as one undoable edit. Pure analysis lives here; the apply orchestration (staging a
//! review on `App`, ripple-deleting the accepted ranges with a single undo snapshot) lives in
//! the UI side, mirroring D1's silence-review flow.
//!
//! All four proposal kinds are derived from the same media-relative word stream, so they need
//! nothing beyond [`crate::transcript::TranscriptWord`]'s existing text + timestamps + stable
//! ids — no audio decode, no waveform, no Whisper re-run:
//!
//! - **Dead air** (`TranscriptEditKind::DeadAir`) — a long pause *between* two consecutive
//!   words (their `gap = next.start - this.end` exceeds [`DEAD_AIR_THRESHOLD_SECS`]). The
//!   transcript knows speech boundaries precisely, so this is transcript-derived dead air,
//!   complementary to (not a replacement for) D1's waveform-based silence detection.
//! - **Filler word** (`FillerWord`) — a run of consecutive words from the language's filler set
//!   (`uh`/`um`/`tipo`/`né`/...), conservative by design: only clear vocalized fillers, never
//!   ambiguous common words.
//! - **Retake** (`Retake`) — an immediate false-start self-repetition of one word (or two)
//!   back-to-back ("the the", "I I I", "tipo tipo") — the repeated tail is proposed.
//! - **Repeated phrase** (`RepeatedPhrase`) — a 2-4 word phrase repeated within a short window
//!   ([`REPEAT_WINDOW_SECS`]); the *second* occurrence is proposed as the deletable duplicate.
//!
//! Proposals from the different kinds can overlap (a filler word inside a repeated phrase, dead
//! air flanking a retake). Applying merges overlapping/adjacent accepted ranges into disjoint
//! source intervals first (see [`merge_source_ranges`]) so a sequence of ripple-deletes can't
//! double-cut or produce inconsistent edges.

use crate::timeline::ClipInstance;
use crate::transcript::TranscriptWord;

/// How long a pause between two consecutive transcribed words must be before it counts as
/// cuttable dead air — 0.8s, long enough to skip ordinary between-phrase breathing beats of
/// normal speech cadence, short enough to actually catch a held pause or dead stretch in
/// gameplay commentary.
pub const DEAD_AIR_THRESHOLD_SECS: f64 = 0.8;

/// The maximum time span between the first and second occurrence of a repeated phrase for the
/// second occurrence to be proposed as a deletable duplicate — 5s. A phrase repeated only much
/// later is more likely an intentional callback than a flubbed take.
pub const REPEAT_WINDOW_SECS: f64 = 5.0;

/// Longest n-gram considered for [`TranscriptEditKind::RepeatedPhrase`].
pub const MAX_REPEAT_NGRAM: usize = 4;

/// What kind of edit a [`TranscriptProposal`] suggests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptEditKind {
    /// A long pause between two consecutive words (transcript-derived dead air).
    DeadAir,
    /// A run of vocalized filler words (`uh`/`tipo`/...).
    FillerWord,
    /// An immediate false-start repetition of the same word(s) back-to-back.
    Retake,
    /// A phrase repeated within a short window; the second occurrence is the duplicate.
    RepeatedPhrase,
}

/// One discrete, reviewable edit suggestion over a [`TranscriptDocument`]'s word stream. The
/// range is **media/source-relative** seconds (the same space `TranscriptWord::start_secs`/
/// `end_secs` live in); mapping it onto a specific timeline clip (trim + speed) is a separate
/// step ([`map_source_range_to_timeline`]) because the same asset can be cut into many clips.
#[derive(Debug, Clone, PartialEq)]
pub struct TranscriptProposal {
    pub kind: TranscriptEditKind,
    /// Stable id (per slice 1) of the first word this proposal touches — non-`Option` by
    /// construction since every proposal starts and ends at a word or word boundary.
    pub first_word_id: u64,
    pub last_word_id: u64,
    /// Media-relative start of the range to delete.
    pub start_secs: f64,
    /// Media-relative end of the range to delete.
    pub end_secs: f64,
    /// Human-readable description for the review row (e.g. the joined filler words or the
    /// repeated phrase text).
    pub detail: String,
}

impl TranscriptProposal {
    pub fn source_duration_secs(&self) -> f64 {
        self.end_secs - self.start_secs
    }
}

/// The filler-word vocabulary for a given Whisper language code (`"en"`, `"pt"`, ...),
/// conservative by design — only unambiguous vocalized fillers, not common words that could be
/// real content. Unknown/`None` language falls back to a combined set so detection still works
/// rather than silently returning nothing.
pub fn filler_word_set(language: Option<&str>) -> &'static [&'static str] {
    match language {
        Some("pt") => &["uh", "um", "ah", "hmm", "tipo", "né", "sabe", "uhh", "umm"],
        Some("en") => &["uh", "um", "ah", "er", "hmm", "uhh", "umm"],
        _ => &[
            "uh", "um", "ah", "er", "hmm", "uhh", "umm", "tipo", "né", "sabe",
        ],
    }
}

fn norm(word: &str) -> String {
    word.to_lowercase()
        .chars()
        .filter(|c| !c.is_ascii_punctuation())
        .collect()
}

/// Detects every kind of proposed edit over `words` (media-relative, in source order) using the
/// language-aware filler set for `language`. Proposals may overlap; callers merge via
/// [`merge_source_ranges`] before applying.
pub fn detect_proposals(
    words: &[TranscriptWord],
    language: Option<&str>,
) -> Vec<TranscriptProposal> {
    let mut out = Vec::new();

    // --- Dead air: gaps between consecutive words taller than the threshold ---
    for pair in words.windows(2) {
        let gap = pair[1].start_secs - pair[0].end_secs;
        if gap >= DEAD_AIR_THRESHOLD_SECS {
            out.push(TranscriptProposal {
                kind: TranscriptEditKind::DeadAir,
                first_word_id: pair[0].id,
                last_word_id: pair[1].id,
                start_secs: pair[0].end_secs,
                end_secs: pair[1].start_secs,
                detail: String::new(),
            });
        }
    }

    let fillers: std::collections::HashSet<&str> =
        filler_word_set(language).iter().copied().collect();

    // --- Filler runs: merge consecutive filler words into one proposal each ---
    let mut i = 0;
    while i < words.len() {
        if fillers.contains(norm(&words[i].text).as_str()) {
            let run_start = i;
            let mut j = i;
            while j + 1 < words.len() && fillers.contains(norm(&words[j + 1].text).as_str()) {
                j += 1;
            }
            out.push(TranscriptProposal {
                kind: TranscriptEditKind::FillerWord,
                first_word_id: words[run_start].id,
                last_word_id: words[j].id,
                start_secs: words[run_start].start_secs,
                end_secs: words[j].end_secs,
                detail: words[run_start..=j]
                    .iter()
                    .map(|w| w.text.clone())
                    .collect::<Vec<_>>()
                    .join(" "),
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }

    // --- Retakes: immediate false-start repetition of 1-2 identical words ---
    i = 0;
    while i + 1 < words.len() {
        if norm(&words[i].text) == norm(&words[i + 1].text) {
            // Extend across any further immediate identical words ("I I I I").
            let mut j = i + 1;
            while j < words.len() && norm(&words[j].text) == norm(&words[i].text) {
                j += 1;
            }
            let tail = &words[i + 1..j];
            if !tail.is_empty() {
                out.push(TranscriptProposal {
                    kind: TranscriptEditKind::Retake,
                    first_word_id: tail[0].id,
                    last_word_id: tail[tail.len() - 1].id,
                    start_secs: tail[0].start_secs,
                    end_secs: tail[tail.len() - 1].end_secs,
                    detail: tail
                        .iter()
                        .map(|w| w.text.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
            }
            i = j;
        } else {
            i += 1;
        }
    }

    // --- Repeated phrases: a 2-4 word phrase repeating within a short window ---
    for n in 2..=MAX_REPEAT_NGRAM {
        let mut seen: Vec<(String, usize, f64)> = Vec::new(); // (norm text, start idx, start_secs)
        let mut k = 0;
        while k + n <= words.len() {
            let ngram: Vec<String> = words[k..k + n].iter().map(|w| norm(&w.text)).collect();
            // Skip n-grams containing a filler word — those are already covered by FillerWord.
            if ngram.iter().any(|w| fillers.contains(w.as_str())) {
                k += 1;
                continue;
            }
            let key = ngram.join(" ");
            let start_secs = words[k].start_secs;
            // Only a *previous* occurrence of this exact phrase, within the window, is a
            // duplicate worth proposing — an unrelated n-gram nearby is not a repeat.
            let is_duplicate = seen.iter().any(|(seen_key, seen_idx, seen_start)| {
                seen_key == &key && *seen_idx < k && start_secs - seen_start <= REPEAT_WINDOW_SECS
            });
            if is_duplicate {
                // Second occurrence within the window -> propose deleting it (the duplicate).
                out.push(TranscriptProposal {
                    kind: TranscriptEditKind::RepeatedPhrase,
                    first_word_id: words[k].id,
                    last_word_id: words[k + n - 1].id,
                    start_secs: words[k].start_secs,
                    end_secs: words[k + n - 1].end_secs,
                    detail: words[k..k + n]
                        .iter()
                        .map(|w| w.text.clone())
                        .collect::<Vec<_>>()
                        .join(" "),
                });
                // Don't also match a third occurrence against the same phrase base here (would
                // propose deleting all of them); advance past this n-gram.
                k += n;
                continue;
            }
            seen.push((key, k, start_secs));
            k += 1;
        }
    }

    out
}

/// An inclusive source-media time range, the result of collapsing overlapping proposal ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SourceRange {
    pub start_secs: f64,
    pub end_secs: f64,
}

/// Merges `proposals`' source ranges into disjoint, maximal inclusive ranges (overlapping or
/// touching entries collapse into one). Output is earliest-first. Ranges are non-empty by
/// construction (`end > start`). This is the exact input the apply step wants so a sequence of
/// `ripple_delete_range` calls can never double-cut the same media.
pub fn merge_source_ranges(proposals: &[TranscriptProposal]) -> Vec<SourceRange> {
    let mut ranges: Vec<(f64, f64)> = proposals
        .iter()
        .map(|p| (p.start_secs, p.end_secs))
        .filter(|(s, e)| e > s)
        .collect();
    ranges.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (s, e) in ranges {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }
    merged
        .into_iter()
        .map(|(start_secs, end_secs)| SourceRange {
            start_secs,
            end_secs,
        })
        .collect()
}

/// Maps a media-relative `(start, end)` range onto `clip`'s own timeline position, clipped to
/// the portion of it `clip` actually displays (`source_in_secs..source_out_secs`) and scaled by
/// `clip`'s `speed_factor`. Returns `None` when the range doesn't intersect the clip's visible
/// source at all, or when the clip's `speed_factor` isn't positive (can't map it).
pub fn map_source_range_to_timeline(
    clip: &ClipInstance,
    start_secs: f64,
    end_secs: f64,
) -> Option<(f64, f64)> {
    if clip.speed_factor <= 0.0 {
        return None;
    }
    let clipped_start = start_secs.max(clip.source_in_secs);
    let clipped_end = end_secs.min(clip.source_out_secs);
    if clipped_end <= clipped_start {
        return None;
    }
    Some((
        clip.source_to_timeline_secs(clipped_start),
        clip.source_to_timeline_secs(clipped_end),
    ))
}

#[cfg(test)]
#[path = "transcript_proposals/transcript_proposals_test.rs"]
mod tests;
