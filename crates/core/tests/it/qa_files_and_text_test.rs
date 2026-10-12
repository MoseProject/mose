// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! QA sweep over what reads untrusted or awkward input: project files damaged on disk, subtitle
//! export with unusual text, and silence detection on odd waveforms. None of these may panic, and
//! what they return must stay well-formed.

use avcore::persistence::{from_project_bytes, to_project_bytes};
use avcore::timeline::{AudioRole, TextClip, Timeline, Track, TrackKind};
use avcore::{detect_silence_gaps, export_srt, Project, Recency, Sequence};

fn project() -> Project {
    let mut text = Track {
        id: 1,
        name: "Text".to_string(),
        kind: TrackKind::Text,
        clips: vec![],
        text_clips: vec![TextClip::new(1, 1.0, "hello".to_string())],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    text.text_clips
        .push(TextClip::new(2, 5.0, "world".to_string()));
    Project {
        id: 1,
        name: "QA".to_string(),
        last_edited: Recency::DaysAgo(1),
        summary: String::new(),
        media_library: vec![],
        sequences: vec![Sequence {
            id: 1,
            name: "Main".to_string(),
            timeline: Timeline {
                tracks: vec![text],
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: Default::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
        frame_rate: None,
    }
}

#[test]
fn a_project_file_cut_short_anywhere_is_an_error_never_a_panic() {
    let bytes = to_project_bytes(&project()).unwrap();
    for length in 0..bytes.len() {
        // Ok is allowed only if the cut happened to leave a valid file; it must equal the
        // original then. Anything else must be an Err.
        if let Ok(parsed) = from_project_bytes(&bytes[..length]) {
            assert_eq!(
                parsed,
                project(),
                "a cut at {length} parsed to something else"
            );
        }
    }
}

#[test]
fn a_project_file_with_any_single_byte_changed_never_panics() {
    let bytes = to_project_bytes(&project()).unwrap();
    for index in 0..bytes.len() {
        for flip in [0x01u8, 0x80, 0xFF] {
            let mut damaged = bytes.clone();
            damaged[index] ^= flip;
            let _ = from_project_bytes(&damaged);
        }
    }
}

#[test]
fn a_project_file_claiming_a_huge_size_is_refused_without_allocating_it() {
    let mut bytes = to_project_bytes(&project()).unwrap();
    // The declared decompressed size sits right after the 4-byte magic and the version byte.
    bytes[5..9].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(from_project_bytes(&bytes).is_err());
}

#[test]
fn a_project_round_trips_through_its_bytes() {
    let original = project();
    assert_eq!(
        from_project_bytes(&to_project_bytes(&original).unwrap()).unwrap(),
        original
    );
}

fn text_timeline(clips: Vec<TextClip>) -> Timeline {
    Timeline {
        tracks: vec![Track {
            id: 1,
            name: "Text".to_string(),
            kind: TrackKind::Text,
            clips: vec![],
            text_clips: clips,
            shape_clips: vec![],
            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            color_label: None,
        }],
        playhead_secs: 0.0,
        markers: vec![],
        multicam_groups: vec![],
    }
}

/// Splits an SRT file into its entries the way a player does: a blank line ends one.
fn srt_entries(srt: &str) -> Vec<Vec<&str>> {
    let mut entries = Vec::new();
    let mut current = Vec::new();
    for line in srt.lines() {
        if line.trim().is_empty() {
            if !current.is_empty() {
                entries.push(std::mem::take(&mut current));
            }
        } else {
            current.push(line);
        }
    }
    if !current.is_empty() {
        entries.push(current);
    }
    entries
}

#[test]
fn a_caption_with_a_blank_line_in_it_stays_one_srt_entry() {
    let mut clip = TextClip::new(1, 0.0, "first paragraph\n\nsecond paragraph".to_string());
    clip.duration_secs = 2.0;
    let srt = export_srt(&text_timeline(vec![clip]));
    let entries = srt_entries(&srt);
    assert_eq!(
        entries.len(),
        1,
        "a blank line inside the text split the entry: {entries:?}"
    );
    assert_eq!(entries[0][0], "1");
    assert!(entries[0][2..].join(" ").contains("second paragraph"));
}

#[test]
fn srt_numbers_entries_in_order_and_survives_non_finite_times() {
    let mut late = TextClip::new(1, 9.0, "late".to_string());
    late.duration_secs = 1.0;
    let mut early = TextClip::new(2, 1.0, "early".to_string());
    early.duration_secs = 1.0;
    let mut broken = TextClip::new(3, f64::NAN, "broken".to_string());
    broken.duration_secs = f64::INFINITY;
    // Enough clips that a sort with an inconsistent order would be exercised.
    let mut many: Vec<TextClip> = (0..64)
        .map(|i| TextClip::new(10 + i, (63 - i) as f64, format!("clip {i}")))
        .collect();
    many.extend([late, early, broken]);
    let srt = export_srt(&text_timeline(many));
    let entries = srt_entries(&srt);
    assert_eq!(entries.len(), 67);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry[0],
            (index + 1).to_string(),
            "entries are numbered in order"
        );
        assert!(entry[1].contains(" --> "), "{entry:?}");
    }
}

#[test]
fn srt_never_ends_a_caption_before_it_starts() {
    let mut clip = TextClip::new(1, 5.0, "backwards".to_string());
    clip.duration_secs = -2.0;
    let srt = export_srt(&text_timeline(vec![clip]));
    let line = srt.lines().nth(1).unwrap();
    let (start, end) = line.split_once(" --> ").unwrap();
    assert!(end >= start, "{line}");
}

#[test]
fn silence_gaps_are_ordered_disjoint_inside_the_file_and_long_enough() {
    let mut state = 12345u64;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as u32
    };
    for round in 0..300 {
        let len = 1 + (next() % 400) as usize;
        let peaks: Vec<(f32, f32)> = (0..len)
            .map(|_| {
                let loud = next() % 3 == 0;
                let amp = if loud {
                    0.2 + (next() % 80) as f32 / 100.0
                } else {
                    0.001
                };
                (-amp, amp)
            })
            .collect();
        let duration = 1.0 + (next() % 600) as f64 / 10.0;
        let min_gap = 0.1 + (next() % 20) as f64 / 10.0;
        let gaps = detect_silence_gaps(&peaks, duration, 0.01, min_gap);
        let mut previous_end = 0.0;
        for gap in &gaps {
            let context = format!("round {round}: {gap:?} of {duration} s, min {min_gap}");
            assert!(
                gap.start_secs >= previous_end - 1e-9,
                "{context}: overlaps the one before"
            );
            assert!(
                gap.end_secs <= duration + 1e-9,
                "{context}: past the end of the file"
            );
            assert!(
                gap.duration_secs() >= min_gap - 1e-9,
                "{context}: shorter than the minimum"
            );
            previous_end = gap.end_secs;
        }
    }
}

#[test]
fn silence_detection_on_a_degenerate_input_is_empty() {
    assert!(detect_silence_gaps(&[], 10.0, 0.01, 0.5).is_empty());
    assert!(detect_silence_gaps(&[(0.0, 0.0); 10], 0.0, 0.01, 0.5).is_empty());
    assert!(detect_silence_gaps(&[(0.0, 0.0); 10], f64::NAN, 0.01, 0.5).is_empty());
    assert!(detect_silence_gaps(&[(f32::NAN, f32::NAN); 10], 10.0, 0.01, 0.5).is_empty());
}

#[test]
fn a_fully_silent_file_is_one_gap_covering_all_of_it() {
    let gaps = detect_silence_gaps(&[(0.0, 0.0); 100], 30.0, 0.01, 0.5);
    assert_eq!(gaps.len(), 1);
    assert!((gaps[0].start_secs).abs() < 1e-9);
    assert!((gaps[0].end_secs - 30.0).abs() < 1e-9);
}
