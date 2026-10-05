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

//! CF-08: semantic transcript and visual media search
//! ([`spec/architecture/competitive-feature-plan.md`](../../../../spec/architecture/competitive-feature-plan.md)'s
//! own "Outcome: locate moments using queries such as 'boss fight', 'victory screen', or a
//! phrase spoken by the presenter").
//!
//! CF-08's slice 1 (exact transcript search) already shipped as part of CF-01 —
//! [`crate::transcript_search`]. This module is slice 2 ("Add a versioned local index keyed by
//! media content fingerprint and model version") plus the storage/scoring half of slice 4
//! ("Return timestamped results with the source of the match... or a combined score"), plus the
//! *planning* half of slice 3 ("Index bounded representative frames and transcript chunks
//! incrementally") — [`plan_representative_frames`]/[`plan_transcript_chunks`] decide *which*
//! timestamps/spans are worth embedding, and [`IndexingCursor`]/[`next_indexing_batch`] make
//! working through them cancellable, resumable, and bounded per call, the doc's own three
//! acceptance words for this slice.
//!
//! **Deliberately not attempted here: computing any real embedding.** A real semantic-search
//! embedding model (text and/or visual) needs network access to fetch model weights and
//! `libonnxruntime` — this sandbox has neither (the same `ORT_SKIP_DOWNLOAD=1`/no-network gap
//! `CLAUDE.md` documents for `background_removal`/`auto_reframe`). So [`IndexedChunk::embedding`]
//! is an opaque `Vec<f32>` this module never produces itself, and neither does the planning code
//! below — it hands a caller *what* to embed (a frame timestamp or a transcript text span), never
//! computes the embedding itself or reads a file from disk (no `FrameSampler`/transcript-sidecar
//! I/O here — see those modules for the actual decode/load step). What's here — the versioned
//! index shape, fingerprint-based invalidation, a bounded storage budget, cosine-similarity
//! search/combination over whatever embeddings a caller supplies, and now the bounded/resumable
//! planning queue — is real, useful, and fully testable without a real embedding model.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::transcript::TranscriptWord;

/// The on-disk index schema version — bumped only if this module's own layout changes (not the
/// same axis as [`MediaIndexEntry::model_version`], which tracks the *embedding model*).
pub const INDEX_SCHEMA_VERSION: u32 = 1;

/// A cheap, deterministic stand-in for "this file's content is unchanged" — not a true
/// cryptographic content hash. Hashing a multi-gigabyte video file's full bytes on every
/// staleness check would be far too slow for routine use; `(file size, mtime, duration)` is a
/// real, if weaker, signal that's already how this crate's other caches (proxies, waveforms)
/// implicitly reason about staleness, made explicit and combined here for the index's own
/// invalidation check. A caller wanting stronger guarantees can still detect a same-fingerprint
/// content swap the ordinary way every other feature in this app already does: the file failing
/// to probe/decode as expected.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MediaFingerprint {
    pub file_size_bytes: u64,
    pub modified_unix: i64,
    pub duration_secs: f64,
}

impl MediaFingerprint {
    /// Reads `path`'s filesystem metadata to build a fingerprint. `modified_unix` is `0` if the
    /// platform/filesystem can't report a modification time (rare, but real — never a hard
    /// error) — a fingerprint carrying `0` still round-trips and still invalidates correctly
    /// against a later `compute()` reporting a real one.
    pub fn compute(path: &Path, duration_secs: f64) -> std::io::Result<Self> {
        let metadata = std::fs::metadata(path)?;
        let modified_unix = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Ok(Self {
            file_size_bytes: metadata.len(),
            modified_unix,
            duration_secs,
        })
    }
}

/// Which signal produced a [`SearchResult`] — the doc's own "source of the match: transcript,
/// visual, metadata, or a combined score." [`Self::Combined`] is never stored in an
/// [`IndexedChunk`] (a chunk always comes from exactly one real source) — [`combine_search_
/// results`] produces it at query time when transcript and visual hits agree on the same moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchSource {
    Transcript,
    Visual,
    Metadata,
    Combined,
}

/// One embedded, timestamped span of a media asset — the unit [`SemanticIndex`] stores and
/// searches over. `embedding` is opaque to this module (see this module's own doc comment for
/// why no real one is computed here yet); its only requirement is that two embeddings are only
/// meaningfully comparable when they came from the same [`MediaIndexEntry::model_version`],
/// which [`SemanticIndex::search`] enforces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexedChunk {
    pub start_secs: f64,
    pub end_secs: f64,
    pub source: MatchSource,
    pub embedding: Vec<f32>,
    /// The transcript text this chunk indexes, when `source == Transcript` — kept so a search
    /// result can show a human-readable snippet without re-reading the transcript sidecar.
    /// `None` for `Visual`/`Metadata` chunks, which have no associated text.
    pub text: Option<String>,
}

/// Everything indexed for one media asset, at one fingerprint/model-version pair.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaIndexEntry {
    pub media_id: u64,
    pub fingerprint: MediaFingerprint,
    /// Identifies which embedding model produced every chunk in [`Self::chunks`] — an opaque,
    /// caller-defined string (e.g. a model name plus its own version) since this module has no
    /// embedding model of its own to name yet.
    pub model_version: String,
    pub chunks: Vec<IndexedChunk>,
}

/// The full local semantic-search index — one [`MediaIndexEntry`] per indexed media asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SemanticIndex {
    pub schema_version: u32,
    /// In first-indexed-first order — [`SemanticIndex::enforce_chunk_budget`] evicts from the
    /// front, oldest entry first, matching a simple FIFO eviction policy rather than tracking
    /// real last-used timestamps this slice has no query-time telemetry to drive yet.
    pub entries: Vec<MediaIndexEntry>,
}

impl Default for SemanticIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SemanticIndex {
    pub fn new() -> Self {
        Self {
            schema_version: INDEX_SCHEMA_VERSION,
            entries: Vec::new(),
        }
    }

    /// Whether `media_id` needs (re)indexing against `fingerprint`/`model_version` — true when
    /// there's no entry for it yet, or the stored entry's own fingerprint or model version
    /// doesn't match. The doc's own "index invalidation is deterministic when media or model
    /// versions change" acceptance criterion, made a single pure function.
    pub fn needs_reindex(
        &self,
        media_id: u64,
        fingerprint: MediaFingerprint,
        model_version: &str,
    ) -> bool {
        match self.entries.iter().find(|e| e.media_id == media_id) {
            None => true,
            Some(entry) => entry.fingerprint != fingerprint || entry.model_version != model_version,
        }
    }

    /// Inserts `entry`, replacing any existing entry for the same `media_id` — the result of a
    /// (re)indexing pass. The replaced entry's position is *not* preserved (the new entry is
    /// appended), so a freshly (re)indexed asset is the newest for [`Self::enforce_chunk_budget`]
    /// eviction purposes, matching the intent of a FIFO-by-freshness policy.
    pub fn upsert_entry(&mut self, entry: MediaIndexEntry) {
        self.entries.retain(|e| e.media_id != entry.media_id);
        self.entries.push(entry);
    }

    /// Drops any indexed data for `media_id` — e.g. when the asset is removed from every
    /// project's media library. Returns whether an entry was actually removed.
    pub fn remove_entry(&mut self, media_id: u64) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.media_id != media_id);
        self.entries.len() != before
    }

    /// The total chunk count across every entry — what [`Self::enforce_chunk_budget`] bounds.
    pub fn total_chunks(&self) -> usize {
        self.entries.iter().map(|e| e.chunks.len()).sum()
    }

    /// Evicts whole entries, oldest (front of [`Self::entries`]) first, until [`Self::
    /// total_chunks`] is at or under `max_total_chunks` — the doc's own "indexing is... bounded
    /// in CPU, memory, and disk usage" acceptance criterion. A no-op if already within budget.
    /// Evicts by whole entry (never partially trims one asset's own chunks), so a search never
    /// sees a media asset with an inconsistent subset of its own indexed spans.
    pub fn enforce_chunk_budget(&mut self, max_total_chunks: usize) {
        while self.total_chunks() > max_total_chunks && !self.entries.is_empty() {
            self.entries.remove(0);
        }
    }
}

/// One ranked hit from [`SemanticIndex::search`].
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult {
    pub media_id: u64,
    pub start_secs: f64,
    pub end_secs: f64,
    pub source: MatchSource,
    pub text: Option<String>,
    /// Cosine similarity to the query embedding, in `-1.0..=1.0` (in practice usually
    /// `0.0..=1.0` for real embedding models, whose vectors are rarely near-antiparallel).
    pub score: f32,
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.len() != b.len() || a.is_empty() {
        return None;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return None;
    }
    Some(dot / (norm_a * norm_b))
}

impl SemanticIndex {
    /// Ranks every indexed chunk whose entry's `model_version` matches `model_version` (a
    /// different-model embedding lives in a different, incomparable vector space, so it's
    /// silently excluded rather than scored meaninglessly) by cosine similarity to
    /// `query_embedding`, highest first, truncated to `top_k`. Satisfies the doc's own "search
    /// results seek to the matched moment, not only the containing asset" criterion — every
    /// result carries its own `start_secs`/`end_secs`, not just a `media_id`.
    pub fn search(
        &self,
        query_embedding: &[f32],
        model_version: &str,
        top_k: usize,
    ) -> Vec<SearchResult> {
        let mut results: Vec<SearchResult> = self
            .entries
            .iter()
            .filter(|e| e.model_version == model_version)
            .flat_map(|entry| {
                entry.chunks.iter().filter_map(move |chunk| {
                    cosine_similarity(query_embedding, &chunk.embedding).map(|score| SearchResult {
                        media_id: entry.media_id,
                        start_secs: chunk.start_secs,
                        end_secs: chunk.end_secs,
                        source: chunk.source,
                        text: chunk.text.clone(),
                        score,
                    })
                })
            })
            .collect();
        results.sort_by(|a, b| b.score.total_cmp(&a.score));
        results.truncate(top_k);
        results
    }
}

/// How close two results' time spans must land (on the same `media_id`) to be considered "the
/// same moment" for [`combine_search_results`].
pub const DEFAULT_COMBINE_WINDOW_SECS: f64 = 2.0;

fn spans_overlap_within(a: &SearchResult, b: &SearchResult, window_secs: f64) -> bool {
    a.media_id == b.media_id
        && a.start_secs < b.end_secs + window_secs
        && b.start_secs < a.end_secs + window_secs
}

/// Merges same-moment hits from *different* sources into one [`MatchSource::Combined`] result —
/// the doc's own "...or a combined score" acceptance criterion, the same "evidence from more
/// than one signal deserves a boost" idea [`crate::highlight_detection::combine_highlight_
/// candidates`] already established for CF-02, adapted from that module's binary flags to this
/// one's continuous cosine scores: a combined result's score is the pair's own mean plus a fixed
/// `0.1` bonus (clamped to `1.0`), a simple, documented heuristic — not a claim of any particular
/// statistical meaning. `results` should already be sorted by [`SemanticIndex::search`] (highest
/// score first); this preserves that order for whatever it doesn't merge. Only merges the
/// *first* pair found per result (no transitive three-way merging) — real cross-source overlaps
/// beyond a pair are expected to be rare once real indexing exists, and this stays simple rather
/// than guessing at a many-way merge policy no real data has motivated yet.
pub fn combine_search_results(results: Vec<SearchResult>, window_secs: f64) -> Vec<SearchResult> {
    let mut merged = Vec::with_capacity(results.len());
    let mut consumed = vec![false; results.len()];
    for i in 0..results.len() {
        if consumed[i] {
            continue;
        }
        let mut match_j = None;
        for j in (i + 1)..results.len() {
            if consumed[j] {
                continue;
            }
            if results[i].source != results[j].source
                && spans_overlap_within(&results[i], &results[j], window_secs)
            {
                match_j = Some(j);
                break;
            }
        }
        match match_j {
            Some(j) => {
                consumed[i] = true;
                consumed[j] = true;
                let (a, b) = (&results[i], &results[j]);
                let start_secs = a.start_secs.min(b.start_secs);
                let end_secs = a.end_secs.max(b.end_secs);
                let score = ((a.score + b.score) / 2.0 + 0.1).min(1.0);
                merged.push(SearchResult {
                    media_id: a.media_id,
                    start_secs,
                    end_secs,
                    source: MatchSource::Combined,
                    text: a.text.clone().or_else(|| b.text.clone()),
                    score,
                });
            }
            None => {
                consumed[i] = true;
                merged.push(results[i].clone());
            }
        }
    }
    merged.sort_by(|a, b| b.score.total_cmp(&a.score));
    merged
}

/// Default representative-frame sampling density for [`plan_representative_frames`] — sparse
/// enough that a long recording stays bounded (see that function's own `min`/`max` clamps), dense
/// enough to catch distinct visual moments in a short one. Mirrors the order of magnitude
/// `auto_reframe`/`motion_tracking` already use for their own `FrameSampler::even_sample_times`
/// call sites, not a new tuning exercise.
pub const DEFAULT_FRAME_SAMPLES_PER_SEC: f64 = 0.1;
/// Default floor/ceiling for [`plan_representative_frames`]'s sample count — at least a handful
/// of frames even for a very short clip, never more than this regardless of duration (the doc's
/// own "bounded in CPU, memory, and disk usage" criterion applied to the frame side).
pub const DEFAULT_MIN_FRAME_SAMPLES: usize = 3;
pub const DEFAULT_MAX_FRAME_SAMPLES: usize = 200;
/// Default transcript-chunk bounds for [`plan_transcript_chunks`] — a chunk spans at most this
/// many seconds of speech...
pub const DEFAULT_MAX_CHUNK_SPAN_SECS: f64 = 15.0;
/// ...or this many words, whichever comes first — bounds a chunk's own text size independent of
/// how densely someone talks, so a rapid-fire speaker doesn't produce one oversized chunk.
pub const DEFAULT_MAX_WORDS_PER_CHUNK: usize = 40;

/// One unit of representative content slice 3 queues for embedding — a video frame timestamp to
/// sample, or a transcript text span to embed as one chunk. Carries everything an
/// [`IndexedChunk`] needs except the embedding itself, which only a real model (not this module)
/// can produce.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PlannedChunk {
    Frame {
        at_secs: f64,
    },
    Transcript {
        start_secs: f64,
        end_secs: f64,
        text: String,
    },
}

impl PlannedChunk {
    /// The [`MatchSource`] an [`IndexedChunk`] built from this plan should carry.
    pub fn source(&self) -> MatchSource {
        match self {
            PlannedChunk::Frame { .. } => MatchSource::Visual,
            PlannedChunk::Transcript { .. } => MatchSource::Transcript,
        }
    }
}

/// Plans which timestamps within `[0.0, duration_secs)` are worth sampling as representative
/// frames for visual embedding — thin wrapper over [`crate::frame_sampler::FrameSampler::
/// even_sample_times`] (reused, not reimplemented) so this module's own bounds apply uniformly
/// regardless of caller. Empty for a non-positive duration, same as the function it wraps.
pub fn plan_representative_frames(
    duration_secs: f64,
    samples_per_sec: f64,
    min_samples: usize,
    max_samples: usize,
) -> Vec<PlannedChunk> {
    crate::frame_sampler::FrameSampler::even_sample_times(
        0.0,
        duration_secs,
        samples_per_sec,
        min_samples,
        max_samples,
    )
    .into_iter()
    .map(|at_secs| PlannedChunk::Frame { at_secs })
    .collect()
}

/// Groups `words` (already time-ordered, as a loaded [`crate::transcript::TranscriptDocument`]
/// always is) into consecutive chunks, each starting a new chunk once appending the next word
/// would either push the chunk's own span past `max_chunk_span_secs` (measured from the chunk's
/// first word's `start_secs` to the candidate word's `end_secs`) or its word count past
/// `max_words_per_chunk` — whichever bound is hit first. A single word whose own span already
/// exceeds `max_chunk_span_secs` still becomes its own one-word chunk rather than being dropped
/// (never lose content to a pathological bound). Empty input produces no chunks.
pub fn plan_transcript_chunks(
    words: &[TranscriptWord],
    max_chunk_span_secs: f64,
    max_words_per_chunk: usize,
) -> Vec<PlannedChunk> {
    let mut chunks = Vec::new();
    let mut current: Vec<&TranscriptWord> = Vec::new();
    for word in words {
        if let Some(first) = current.first() {
            let would_span = word.end_secs - first.start_secs;
            let would_exceed_span = would_span > max_chunk_span_secs;
            let would_exceed_count = current.len() >= max_words_per_chunk;
            if would_exceed_span || would_exceed_count {
                chunks.push(flush_transcript_chunk(&current));
                current.clear();
            }
        }
        current.push(word);
    }
    if !current.is_empty() {
        chunks.push(flush_transcript_chunk(&current));
    }
    chunks
}

fn flush_transcript_chunk(words: &[&TranscriptWord]) -> PlannedChunk {
    let start_secs = words.first().map(|w| w.start_secs).unwrap_or(0.0);
    let end_secs = words.last().map(|w| w.end_secs).unwrap_or(start_secs);
    let text = words
        .iter()
        .map(|w| w.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    PlannedChunk::Transcript {
        start_secs,
        end_secs,
        text,
    }
}

/// One media asset's own planned work, and how far a resumed pass has gotten through it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PendingMedia {
    pub media_id: u64,
    pub chunks: Vec<PlannedChunk>,
    /// Index into `chunks` of the next one [`next_indexing_batch`] will hand out. Advances by
    /// exactly the number of items a batch actually takes from this entry — resuming after a
    /// cancel/restart just means reconstructing a cursor with this value already set (a caller
    /// persists it the same way it persists a [`SemanticIndex`]).
    pub next_index: usize,
}

impl PendingMedia {
    fn is_finished(&self) -> bool {
        self.next_index >= self.chunks.len()
    }
}

/// Resumable position within one bounded indexing pass across possibly many pending media
/// assets — the doc's own "cancellable, resumable, bounded" acceptance criterion for slice 3,
/// made concrete as plain, serializable data. A caller persists this between calls (or app
/// launches); dropping it after a cancel loses only the *plan* for whatever wasn't embedded
/// yet — nothing here writes to [`SemanticIndex`] itself, so a cancelled pass never leaves the
/// index in an inconsistent state (a caller only calls [`SemanticIndex::upsert_entry`] once it
/// has real embeddings for a media asset's chunks in hand).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct IndexingCursor {
    pub pending: Vec<PendingMedia>,
}

impl IndexingCursor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `chunks` for `media_id`, replacing any existing (possibly partially-worked-through)
    /// entry for the same id — a fresh (re)plan (e.g. after `SemanticIndex::needs_reindex` says
    /// the media changed) always restarts that asset's own progress from the top, since its old
    /// planned chunks no longer correspond to the current content.
    pub fn enqueue(&mut self, media_id: u64, chunks: Vec<PlannedChunk>) {
        self.pending.retain(|p| p.media_id != media_id);
        if !chunks.is_empty() {
            self.pending.push(PendingMedia {
                media_id,
                chunks,
                next_index: 0,
            });
        }
    }

    /// Whether every queued asset has had every planned chunk handed out already — the doc's own
    /// "resumable" pass reaching its natural end, as opposed to a caller-initiated cancel.
    pub fn is_finished(&self) -> bool {
        self.pending.iter().all(PendingMedia::is_finished)
    }
}

/// One item handed out by [`next_indexing_batch`] — which media asset a [`PlannedChunk`] belongs
/// to, since a batch can span more than one pending asset.
#[derive(Debug, Clone, PartialEq)]
pub struct IndexingBatchItem {
    pub media_id: u64,
    pub chunk: PlannedChunk,
}

/// Pops up to `max_items` not-yet-handed-out chunks from `cursor`, advancing each touched entry's
/// own `next_index` in place and dropping any entry that becomes fully finished as a result —
/// bounded per call (`max_items` caps the CPU/decode/embedding work one tick does regardless of
/// how much is queued overall), resumable (progress lives entirely in `cursor`, which a caller
/// can serialize between calls), and cancellable (a caller simply stops calling this — no partial
/// state anywhere else needs unwinding, per [`IndexingCursor`]'s own doc comment). Earlier-queued
/// assets are drained before later ones (front of `cursor.pending` first), so one huge asset can't
/// starve every other pending asset indefinitely — later assets still get scheduled once the
/// batch boundary falls after it. Returns fewer than `max_items` (possibly zero) once every
/// pending entry runs out.
pub fn next_indexing_batch(
    cursor: &mut IndexingCursor,
    max_items: usize,
) -> Vec<IndexingBatchItem> {
    let mut batch = Vec::with_capacity(max_items.min(cursor.pending.len().max(1)));
    for pending in cursor.pending.iter_mut() {
        while batch.len() < max_items && !pending.is_finished() {
            batch.push(IndexingBatchItem {
                media_id: pending.media_id,
                chunk: pending.chunks[pending.next_index].clone(),
            });
            pending.next_index += 1;
        }
        if batch.len() >= max_items {
            break;
        }
    }
    cursor.pending.retain(|p| !p.is_finished());
    batch
}

#[cfg(test)]
#[path = "semantic_index/semantic_index_test.rs"]
mod tests;
