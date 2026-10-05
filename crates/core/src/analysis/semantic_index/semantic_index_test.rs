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

fn fingerprint(size: u64) -> MediaFingerprint {
    MediaFingerprint {
        file_size_bytes: size,
        modified_unix: 1000,
        duration_secs: 60.0,
    }
}

fn chunk(start: f64, end: f64, source: MatchSource, embedding: Vec<f32>) -> IndexedChunk {
    IndexedChunk {
        start_secs: start,
        end_secs: end,
        source,
        embedding,
        text: None,
    }
}

fn entry(
    media_id: u64,
    fp: MediaFingerprint,
    model_version: &str,
    chunks: Vec<IndexedChunk>,
) -> MediaIndexEntry {
    MediaIndexEntry {
        media_id,
        fingerprint: fp,
        model_version: model_version.to_string(),
        chunks,
    }
}

#[test]
fn needs_reindex_is_true_when_no_entry_exists() {
    let index = SemanticIndex::new();
    assert!(index.needs_reindex(1, fingerprint(100), "v1"));
}

#[test]
fn needs_reindex_is_false_when_fingerprint_and_model_version_match() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(1, fingerprint(100), "v1", vec![]));
    assert!(!index.needs_reindex(1, fingerprint(100), "v1"));
}

#[test]
fn needs_reindex_is_true_when_fingerprint_changed() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(1, fingerprint(100), "v1", vec![]));
    assert!(index.needs_reindex(1, fingerprint(200), "v1"));
}

#[test]
fn needs_reindex_is_true_when_model_version_changed() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(1, fingerprint(100), "v1", vec![]));
    assert!(index.needs_reindex(1, fingerprint(100), "v2"));
}

#[test]
fn upsert_entry_replaces_an_existing_entry_for_the_same_media_id() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0])],
    ));
    index.upsert_entry(entry(1, fingerprint(200), "v2", vec![]));
    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].fingerprint, fingerprint(200));
    assert_eq!(index.entries[0].model_version, "v2");
}

#[test]
fn remove_entry_reports_whether_it_actually_removed_something() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(1, fingerprint(100), "v1", vec![]));
    assert!(index.remove_entry(1));
    assert!(!index.remove_entry(1));
    assert!(index.entries.is_empty());
}

#[test]
fn total_chunks_sums_across_every_entry() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![
            chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0]),
            chunk(1.0, 2.0, MatchSource::Transcript, vec![1.0]),
        ],
    ));
    index.upsert_entry(entry(
        2,
        fingerprint(200),
        "v1",
        vec![chunk(0.0, 1.0, MatchSource::Visual, vec![1.0])],
    ));
    assert_eq!(index.total_chunks(), 3);
}

#[test]
fn enforce_chunk_budget_evicts_the_oldest_whole_entry_first() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![
            chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0]),
            chunk(1.0, 2.0, MatchSource::Transcript, vec![1.0]),
        ],
    ));
    index.upsert_entry(entry(
        2,
        fingerprint(200),
        "v1",
        vec![chunk(0.0, 1.0, MatchSource::Visual, vec![1.0])],
    ));

    index.enforce_chunk_budget(2);

    assert_eq!(index.entries.len(), 1);
    assert_eq!(index.entries[0].media_id, 2);
}

#[test]
fn enforce_chunk_budget_is_a_no_op_within_budget() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0])],
    ));
    index.enforce_chunk_budget(100);
    assert_eq!(index.entries.len(), 1);
}

#[test]
fn search_ranks_by_cosine_similarity_highest_first() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![
            chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0, 0.0]),
            chunk(1.0, 2.0, MatchSource::Transcript, vec![0.0, 1.0]),
        ],
    ));

    let results = index.search(&[1.0, 0.0], "v1", 10);

    assert_eq!(results.len(), 2);
    assert!((results[0].score - 1.0).abs() < 1e-6);
    assert!(results[0].start_secs == 0.0);
    assert!(results[1].score < results[0].score);
}

#[test]
fn search_excludes_chunks_from_a_different_model_version() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0, 0.0])],
    ));

    let results = index.search(&[1.0, 0.0], "v2", 10);
    assert!(results.is_empty());
}

#[test]
fn search_respects_top_k() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![
            chunk(0.0, 1.0, MatchSource::Transcript, vec![1.0, 0.0]),
            chunk(1.0, 2.0, MatchSource::Transcript, vec![0.9, 0.1]),
            chunk(2.0, 3.0, MatchSource::Transcript, vec![0.0, 1.0]),
        ],
    ));

    let results = index.search(&[1.0, 0.0], "v1", 1);
    assert_eq!(results.len(), 1);
}

#[test]
fn search_seeks_to_the_matched_chunk_not_just_the_asset() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![
            chunk(0.0, 1.0, MatchSource::Transcript, vec![0.0, 1.0]),
            chunk(50.0, 51.0, MatchSource::Transcript, vec![1.0, 0.0]),
        ],
    ));

    let results = index.search(&[1.0, 0.0], "v1", 1);
    assert_eq!(results[0].start_secs, 50.0);
    assert_eq!(results[0].end_secs, 51.0);
}

#[test]
fn combine_search_results_merges_overlapping_hits_from_different_sources() {
    let results = vec![
        SearchResult {
            media_id: 1,
            start_secs: 10.0,
            end_secs: 11.0,
            source: MatchSource::Transcript,
            text: Some("boss fight".to_string()),
            score: 0.8,
        },
        SearchResult {
            media_id: 1,
            start_secs: 10.5,
            end_secs: 11.5,
            source: MatchSource::Visual,
            text: None,
            score: 0.6,
        },
    ];

    let combined = combine_search_results(results, DEFAULT_COMBINE_WINDOW_SECS);

    assert_eq!(combined.len(), 1);
    assert_eq!(combined[0].source, MatchSource::Combined);
    assert!((combined[0].score - 0.8).abs() < 1e-6); // (0.8+0.6)/2 + 0.1 = 0.8
    assert_eq!(combined[0].text, Some("boss fight".to_string()));
}

#[test]
fn combine_search_results_leaves_a_same_source_pair_unmerged() {
    let results = vec![
        SearchResult {
            media_id: 1,
            start_secs: 10.0,
            end_secs: 11.0,
            source: MatchSource::Transcript,
            text: None,
            score: 0.8,
        },
        SearchResult {
            media_id: 1,
            start_secs: 10.2,
            end_secs: 11.2,
            source: MatchSource::Transcript,
            text: None,
            score: 0.7,
        },
    ];

    let combined = combine_search_results(results, DEFAULT_COMBINE_WINDOW_SECS);
    assert_eq!(combined.len(), 2);
    assert!(combined.iter().all(|r| r.source == MatchSource::Transcript));
}

#[test]
fn combine_search_results_leaves_a_far_apart_pair_unmerged() {
    let results = vec![
        SearchResult {
            media_id: 1,
            start_secs: 0.0,
            end_secs: 1.0,
            source: MatchSource::Transcript,
            text: None,
            score: 0.8,
        },
        SearchResult {
            media_id: 1,
            start_secs: 100.0,
            end_secs: 101.0,
            source: MatchSource::Visual,
            text: None,
            score: 0.6,
        },
    ];

    let combined = combine_search_results(results, DEFAULT_COMBINE_WINDOW_SECS);
    assert_eq!(combined.len(), 2);
}

#[test]
fn combine_search_results_leaves_a_different_media_pair_unmerged() {
    let results = vec![
        SearchResult {
            media_id: 1,
            start_secs: 10.0,
            end_secs: 11.0,
            source: MatchSource::Transcript,
            text: None,
            score: 0.8,
        },
        SearchResult {
            media_id: 2,
            start_secs: 10.0,
            end_secs: 11.0,
            source: MatchSource::Visual,
            text: None,
            score: 0.6,
        },
    ];

    let combined = combine_search_results(results, DEFAULT_COMBINE_WINDOW_SECS);
    assert_eq!(combined.len(), 2);
}

#[test]
fn combine_search_results_clamps_the_bonus_to_1_0() {
    let results = vec![
        SearchResult {
            media_id: 1,
            start_secs: 10.0,
            end_secs: 11.0,
            source: MatchSource::Transcript,
            text: None,
            score: 0.99,
        },
        SearchResult {
            media_id: 1,
            start_secs: 10.2,
            end_secs: 11.2,
            source: MatchSource::Visual,
            text: None,
            score: 0.99,
        },
    ];

    let combined = combine_search_results(results, DEFAULT_COMBINE_WINDOW_SECS);
    assert_eq!(combined.len(), 1);
    assert!(combined[0].score <= 1.0);
}

#[test]
fn ocsi_framing_round_trips_a_real_index() {
    let mut index = SemanticIndex::new();
    index.upsert_entry(entry(
        1,
        fingerprint(100),
        "v1",
        vec![chunk(
            0.0,
            1.0,
            MatchSource::Transcript,
            vec![1.0, 2.0, 3.0],
        )],
    ));

    let bytes = crate::persistence::to_ocsi_bytes(&index).unwrap();
    assert_eq!(&bytes[..4], b"OCSI");
    let decoded: SemanticIndex = crate::persistence::from_ocsi_bytes(&bytes).unwrap();
    assert_eq!(decoded, index);
}

#[test]
fn ocsi_bytes_are_rejected_by_the_octr_decoder() {
    let index = SemanticIndex::new();
    let bytes = crate::persistence::to_ocsi_bytes(&index).unwrap();
    let result: Result<crate::transcript::TranscriptDocument, _> =
        crate::persistence::from_octr_bytes(&bytes);
    assert!(result.is_err());
}

fn word(id: u64, text: &str, start: f64, end: f64) -> TranscriptWord {
    TranscriptWord {
        id,
        text: text.to_string(),
        start_secs: start,
        end_secs: end,
        confidence: 0.9,
        speaker: None,
    }
}

#[test]
fn plan_representative_frames_is_empty_for_non_positive_duration() {
    assert!(plan_representative_frames(0.0, 0.1, 3, 200).is_empty());
    assert!(plan_representative_frames(-5.0, 0.1, 3, 200).is_empty());
}

#[test]
fn plan_representative_frames_clamps_to_min_and_max() {
    // A 1-second clip at a low sample rate would round to 0/1 samples without the floor.
    let few = plan_representative_frames(1.0, 0.1, 3, 200);
    assert_eq!(few.len(), 3);
    // A very long clip at a high rate would produce thousands of samples without the ceiling.
    let many = plan_representative_frames(10_000.0, 1.0, 3, 200);
    assert_eq!(many.len(), 200);
}

#[test]
fn plan_representative_frames_produces_frame_variants_in_range() {
    let planned = plan_representative_frames(20.0, 0.5, 3, 200);
    assert!(!planned.is_empty());
    for p in &planned {
        match p {
            PlannedChunk::Frame { at_secs } => {
                // even_sample_times spaces samples across [start, end] inclusive when there's
                // more than one sample (the last sample lands exactly on end_secs) — confirmed
                // by a real scratch-crate run, not assumed.
                assert!(*at_secs >= 0.0 && *at_secs <= 20.0);
            }
            PlannedChunk::Transcript { .. } => panic!("expected only Frame variants"),
        }
    }
}

#[test]
fn planned_chunk_source_maps_correctly() {
    assert_eq!(
        PlannedChunk::Frame { at_secs: 1.0 }.source(),
        MatchSource::Visual
    );
    assert_eq!(
        PlannedChunk::Transcript {
            start_secs: 0.0,
            end_secs: 1.0,
            text: "hi".to_string()
        }
        .source(),
        MatchSource::Transcript
    );
}

#[test]
fn plan_transcript_chunks_empty_input_produces_no_chunks() {
    assert!(plan_transcript_chunks(&[], 15.0, 40).is_empty());
}

#[test]
fn plan_transcript_chunks_groups_words_within_bounds_into_one_chunk() {
    let words = vec![
        word(1, "hello", 0.0, 0.5),
        word(2, "world", 0.5, 1.0),
        word(3, "friend", 1.0, 1.5),
    ];
    let chunks = plan_transcript_chunks(&words, 15.0, 40);
    assert_eq!(chunks.len(), 1);
    match &chunks[0] {
        PlannedChunk::Transcript {
            start_secs,
            end_secs,
            text,
        } => {
            assert_eq!(*start_secs, 0.0);
            assert_eq!(*end_secs, 1.5);
            assert_eq!(text, "hello world friend");
        }
        PlannedChunk::Frame { .. } => panic!("expected a Transcript variant"),
    }
}

#[test]
fn plan_transcript_chunks_splits_on_span_bound() {
    let words = vec![
        word(1, "a", 0.0, 1.0),
        word(2, "b", 10.0, 16.0), // pushes span past a 15s bound
        word(3, "c", 16.0, 17.0),
    ];
    let chunks = plan_transcript_chunks(&words, 15.0, 40);
    assert_eq!(chunks.len(), 2);
    let PlannedChunk::Transcript { text: t0, .. } = &chunks[0] else {
        panic!("expected Transcript")
    };
    assert_eq!(t0, "a");
    let PlannedChunk::Transcript { text: t1, .. } = &chunks[1] else {
        panic!("expected Transcript")
    };
    assert_eq!(t1, "b c");
}

#[test]
fn plan_transcript_chunks_splits_on_word_count_bound() {
    let words: Vec<_> = (0..5)
        .map(|i| word(i, "w", i as f64, i as f64 + 0.5))
        .collect();
    let chunks = plan_transcript_chunks(&words, 999.0, 2);
    assert_eq!(chunks.len(), 3); // 2 + 2 + 1
}

#[test]
fn plan_transcript_chunks_never_drops_a_word_whose_own_span_exceeds_the_bound() {
    let words = vec![word(1, "long", 0.0, 100.0)];
    let chunks = plan_transcript_chunks(&words, 15.0, 40);
    assert_eq!(chunks.len(), 1);
    let PlannedChunk::Transcript { text, .. } = &chunks[0] else {
        panic!("expected Transcript")
    };
    assert_eq!(text, "long");
}

#[test]
fn indexing_cursor_starts_finished_when_empty() {
    let cursor = IndexingCursor::new();
    assert!(cursor.is_finished());
}

#[test]
fn indexing_cursor_enqueue_replaces_existing_entry_for_same_media() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(1, vec![PlannedChunk::Frame { at_secs: 0.0 }]);
    cursor.enqueue(
        1,
        vec![
            PlannedChunk::Frame { at_secs: 1.0 },
            PlannedChunk::Frame { at_secs: 2.0 },
        ],
    );
    assert_eq!(cursor.pending.len(), 1);
    assert_eq!(cursor.pending[0].chunks.len(), 2);
}

#[test]
fn indexing_cursor_enqueue_with_empty_chunks_does_not_add_an_entry() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(1, Vec::new());
    assert!(cursor.pending.is_empty());
    assert!(cursor.is_finished());
}

#[test]
fn next_indexing_batch_is_bounded_by_max_items() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(
        1,
        (0..10)
            .map(|i| PlannedChunk::Frame { at_secs: i as f64 })
            .collect(),
    );
    let batch = next_indexing_batch(&mut cursor, 3);
    assert_eq!(batch.len(), 3);
    assert!(!cursor.is_finished());
    assert_eq!(cursor.pending[0].next_index, 3);
}

#[test]
fn next_indexing_batch_drains_across_multiple_pending_assets_in_order() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(1, vec![PlannedChunk::Frame { at_secs: 0.0 }]);
    cursor.enqueue(2, vec![PlannedChunk::Frame { at_secs: 0.0 }]);
    let batch = next_indexing_batch(&mut cursor, 5);
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].media_id, 1);
    assert_eq!(batch[1].media_id, 2);
    assert!(cursor.is_finished());
    assert!(cursor.pending.is_empty());
}

#[test]
fn next_indexing_batch_drops_finished_entries_and_resumes_correctly() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(
        1,
        vec![
            PlannedChunk::Frame { at_secs: 0.0 },
            PlannedChunk::Frame { at_secs: 1.0 },
        ],
    );
    let first = next_indexing_batch(&mut cursor, 1);
    assert_eq!(first.len(), 1);
    assert!(!cursor.is_finished());
    let second = next_indexing_batch(&mut cursor, 5);
    assert_eq!(second.len(), 1);
    assert!(cursor.is_finished());
    assert!(cursor.pending.is_empty());
}

#[test]
fn next_indexing_batch_returns_empty_when_cursor_already_finished() {
    let mut cursor = IndexingCursor::new();
    let batch = next_indexing_batch(&mut cursor, 10);
    assert!(batch.is_empty());
}

#[test]
fn indexing_cursor_round_trips_through_serde() {
    let mut cursor = IndexingCursor::new();
    cursor.enqueue(
        1,
        vec![
            PlannedChunk::Frame { at_secs: 0.0 },
            PlannedChunk::Transcript {
                start_secs: 0.0,
                end_secs: 1.0,
                text: "hi".to_string(),
            },
        ],
    );
    let bytes = serde_json::to_vec(&cursor).unwrap();
    let decoded: IndexingCursor = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded, cursor);
}
