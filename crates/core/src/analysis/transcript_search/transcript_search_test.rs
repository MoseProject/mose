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

use super::*;
use crate::media::MediaKind;
use crate::transcribe::TranscribeSegment;
use crate::transcribe::TranscribeWord;
use crate::transcript::{save_transcript_document, transcript_path, TranscriptDocument};

fn word(text: &str, start: f64, end: f64) -> TranscribeWord {
    TranscribeWord {
        text: text.to_string(),
        start_secs: start,
        end_secs: end,
        confidence: 0.9,
    }
}

fn asset(id: u64, file_name: &str) -> MediaAsset {
    MediaAsset {
        id,
        file_name: file_name.to_string(),
        source_path: std::path::PathBuf::new(),
        kind: MediaKind::Video,
        has_audio: true,
        duration_secs: 100.0,
        codec: String::new(),
        source_bitrate_mbps: 1.0,
        resolution: Some((1920, 1080)),
        fps: Some(30.0),
        sample_rate_khz: Some(48.0),
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    }
}

fn doc(asset_id: u64, words: Vec<TranscribeWord>) -> TranscriptDocument {
    let segments = vec![TranscribeSegment {
        start_secs: 0.0,
        end_secs: words.last().map(|w| w.end_secs).unwrap_or(0.0),
        text: String::new(),
        words,
    }];
    TranscriptDocument::from_transcribe_segments(asset_id, Some("en".to_string()), &segments)
}

/// A scratch dir under temp, cleaned up after the test.
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oca_search_test_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn matches_across_every_asset_with_a_transcript() {
    let dir = scratch_dir("multiple");
    let a1 = asset(1, "first.mp4");
    let a2 = asset(2, "second.mp4");
    let a3 = asset(3, "no_transcript.mp4");
    save_transcript_document(
        &dir,
        &doc(1, vec![word("hello", 0.0, 0.5), word("world", 0.5, 1.0)]),
    )
    .unwrap();
    save_transcript_document(
        &dir,
        &doc(2, vec![word("hello", 2.0, 2.5), word("again", 2.5, 3.0)]),
    )
    .unwrap();

    let hits = search_transcripts_in_project(&dir, &[a1, a2, a3], "HELLO");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].asset_id, 1);
    assert_eq!(hits[0].asset_name, "first.mp4");
    assert_eq!(hits[0].word.text, "hello");
    assert_eq!(hits[1].asset_id, 2);
    assert_eq!(hits[1].asset_name, "second.mp4");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn case_insensitive_and_partial_word_match() {
    let dir = scratch_dir("case");
    save_transcript_document(&dir, &doc(1, vec![word("Pacoca", 0.0, 0.5)])).unwrap();
    let hits = search_transcripts_in_project(&dir, &[asset(1, "a.mp4")], "OCA");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].word.text, "Pacoca");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn skips_assets_without_a_transcript_and_corrupt_sidecars() {
    let dir = scratch_dir("skip");
    // Asset 1 has no sidecar at all; asset 2 gets a corrupt file (writes garbage bytes).
    save_transcript_document(&dir, &doc(2, vec![word("bogus", 0.0, 0.3)])).unwrap();
    let corrupt_path = transcript_path(&dir, 3);
    std::fs::write(&corrupt_path, b"this is not an octr file at all").unwrap();
    let assets = vec![asset(1, "none.mp4"), asset(3, "corrupt.mp4")];

    let hits = search_transcripts_in_project(&dir, &assets, "anything");
    assert!(hits.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_or_blank_query_yields_no_hits() {
    let dir = scratch_dir("blank");
    save_transcript_document(&dir, &doc(1, vec![word("hello", 0.0, 0.5)])).unwrap();
    let assets = [asset(1, "a.mp4")];
    assert!(search_transcripts_in_project(&dir, &assets, "").is_empty());
    assert!(search_transcripts_in_project(&dir, &assets, "   ").is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn results_are_sorted_by_source_time_within_an_asset() {
    let dir = scratch_dir("order");
    // Same asset, two matches out of order in the input list must come back in time order.
    let mut words = vec![
        word("alpha", 5.0, 5.5),
        word("beta", 0.5, 1.0),
        word("alpha", 2.0, 2.5),
    ];
    // Build a valid word list (monotonic) regardless.
    words.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
    save_transcript_document(&dir, &doc(1, words)).unwrap();

    let hits = search_transcripts_in_project(&dir, &[asset(1, "a.mp4")], "alpha");
    assert_eq!(hits.len(), 2);
    assert!(hits[0].word.start_secs < hits[1].word.start_secs);
    let _ = std::fs::remove_dir_all(&dir);
}
