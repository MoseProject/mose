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
use crate::timeline::{BlendMode, ColorFilter, MaskShape, TransitionType};

fn word(text: &str, start: f64, end: f64, id: u64) -> TranscriptWord {
    TranscriptWord {
        id,
        text: text.to_string(),
        start_secs: start,
        end_secs: end,
        confidence: 0.95,
        speaker: None,
    }
}

fn test_clip(
    start_secs: f64,
    source_in_secs: f64,
    source_out_secs: f64,
    speed_factor: f32,
) -> ClipInstance {
    ClipInstance {
        id: 1,
        asset_id: 1,
        start_secs,
        source_in_secs,
        source_out_secs,
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor,
        speed_ramp_end_factor: None,
        nested_sequence_id: None,
        crop_x: 0.0,
        crop_y: 0.0,
        crop_w: 1.0,
        crop_h: 1.0,
        mask_shape: MaskShape::None,
        mask_corner_radius: 0.0,
        flipped_h: false,
        color_filter: ColorFilter::None,
        vignette_intensity: 0.0,
        brightness: 0.0,
        contrast: 1.0,
        saturation: 1.0,
        sharpen: 0.0,
        chroma_key_enabled: false,
        chroma_key_color: [0, 255, 0],
        chroma_key_tolerance: 0.4,
        blur_intensity: 0.0,
        shake_intensity: 0.0,
        glitch_intensity: 0.0,
        pixelize_intensity: 0.0,
        transition_in: TransitionType::None,
        transition_duration_secs: 0.5,
        position_keyframes: vec![],
        scale_keyframes: vec![],
        rotation_keyframes: vec![],
        opacity_keyframes: vec![],
        gain_keyframes: vec![],
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        brightness_keyframes: vec![],
        contrast_keyframes: vec![],
        saturation_keyframes: vec![],
        crop_x_keyframes: vec![],
        crop_y_keyframes: vec![],
        crop_w_keyframes: vec![],
        crop_h_keyframes: vec![],
        deflicker_enabled: false,
        lut_path: String::new(),
        layer_scale_x: 1.0,
        layer_scale_y: 1.0,
        stabilization_intensity: 0.0,
        background_removal_enabled: false,
        background_removal_mask_path: String::new(),
        blend_mode: BlendMode::Normal,
        anchor_x: 0.5,
        anchor_y: 0.5,
        reframe_seed_point: None,
        privacy_blur_enabled: false,
        privacy_blur_mask_path: String::new(),
        privacy_blur_sigma: 15.0,
        privacy_blur_seed_vertices: Vec::new(),
        privacy_blur_seed_center_x_frac: 0.0,
        privacy_blur_seed_center_y_frac: 0.0,
    }
}

#[test]
fn detects_a_dead_air_gap_between_consecutive_words() {
    // Word 1 ends at 2.0, next starts at 3.0 -> a 1.0s pause, above the 0.8s threshold.
    let words = vec![word("hello", 0.0, 2.0, 1), word("world", 3.0, 4.0, 2)];
    let proposals = detect_proposals(&words, Some("en"));
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].kind, TranscriptEditKind::DeadAir);
    assert_eq!(proposals[0].start_secs, 2.0);
    assert_eq!(proposals[0].end_secs, 3.0);
}

#[test]
fn ignores_a_short_pause_below_the_threshold() {
    let words = vec![word("hello", 0.0, 2.0, 1), word("world", 2.3, 3.0, 2)];
    assert!(detect_proposals(&words, Some("en")).is_empty());
}

#[test]
fn groups_consecutive_filler_words_into_one_run() {
    // "tipo uh" both fillers, consecutive -> one FillerWord proposal spanning both.
    let words = vec![
        word("tipo", 0.0, 0.4, 1),
        word("uh", 0.4, 0.7, 2),
        word("vamos", 0.7, 1.5, 3),
    ];
    let proposals = detect_proposals(&words, Some("pt"));
    let fillers = proposals
        .iter()
        .filter(|p| p.kind == TranscriptEditKind::FillerWord)
        .collect::<Vec<_>>();
    assert_eq!(fillers.len(), 1);
    assert_eq!(fillers[0].first_word_id, 1);
    assert_eq!(fillers[0].last_word_id, 2);
    assert_eq!(fillers[0].detail, "tipo uh");
}

#[test]
fn filler_set_is_not_language_sensitive_when_unknown() {
    // A pt filler works even with an unknown language code (combined fallback set).
    let words = vec![word("tipo", 0.0, 0.5, 1), word("ok", 0.5, 1.0, 2)];
    let proposals = detect_proposals(&words, Some("xx"));
    assert!(proposals
        .iter()
        .any(|p| p.kind == TranscriptEditKind::FillerWord));
}

#[test]
fn detects_an_immediate_retake_of_the_same_word() {
    // "the the" -> second occurrence is the retake tail.
    let words = vec![
        word("the", 0.0, 0.3, 1),
        word("the", 0.3, 0.6, 2),
        word("cat", 0.6, 1.2, 3),
    ];
    let proposals = detect_proposals(&words, Some("en"));
    let retakes = proposals
        .iter()
        .filter(|p| p.kind == TranscriptEditKind::Retake)
        .collect::<Vec<_>>();
    assert_eq!(retakes.len(), 1);
    assert_eq!(retakes[0].first_word_id, 2);
    assert_eq!(retakes[0].last_word_id, 2);
    assert_eq!(retakes[0].start_secs, 0.3);
    assert_eq!(retakes[0].end_secs, 0.6);
}

#[test]
fn retake_extends_across_a_long_identical_run() {
    // "I I I I" -> the three repeats after the first form one retake.
    let words = vec![
        word("I", 0.0, 0.2, 1),
        word("I", 0.2, 0.4, 2),
        word("I", 0.4, 0.6, 3),
        word("I", 0.6, 0.8, 4),
        word("think", 0.8, 1.4, 5),
    ];
    let proposals = detect_proposals(&words, Some("en"));
    let retakes = proposals
        .iter()
        .filter(|p| p.kind == TranscriptEditKind::Retake)
        .collect::<Vec<_>>();
    assert_eq!(retakes.len(), 1);
    assert_eq!(retakes[0].first_word_id, 2);
    assert_eq!(retakes[0].last_word_id, 4);
}

#[test]
fn detects_a_phrase_repeated_within_the_window_as_a_duplicate() {
    // "vamos jogar" appears at 0 and again within 5s -> the second is proposed.
    let words = vec![
        word("vamos", 0.0, 0.3, 1),
        word("jogar", 0.3, 0.6, 2),
        word("agora", 0.6, 1.0, 3),
        word("vamos", 3.0, 3.3, 4),
        word("jogar", 3.3, 3.6, 5),
    ];
    let proposals = detect_proposals(&words, Some("pt"));
    let repeated = proposals
        .iter()
        .filter(|p| p.kind == TranscriptEditKind::RepeatedPhrase)
        .collect::<Vec<_>>();
    assert!(!repeated.is_empty());
    let dup = repeated
        .iter()
        .find(|p| p.first_word_id == 4)
        .expect("second occurrence should be proposed");
    assert_eq!(dup.detail, "vamos jogar");
}

#[test]
fn a_phrase_repeated_much_later_is_not_a_proposal() {
    // Repeated after the window -> treated as intentional, not a duplicate.
    let words = vec![
        word("go", 0.0, 0.2, 1),
        word("now", 0.2, 0.5, 2),
        word("go", 10.0, 10.2, 3),
        word("now", 10.2, 10.5, 4),
    ];
    let proposals = detect_proposals(&words, Some("en"));
    assert!(!proposals
        .iter()
        .any(|p| p.kind == TranscriptEditKind::RepeatedPhrase));
}

// --- merge_source_ranges ---

fn proposal(start: f64, end: f64) -> TranscriptProposal {
    TranscriptProposal {
        kind: TranscriptEditKind::DeadAir,
        first_word_id: 0,
        last_word_id: 0,
        start_secs: start,
        end_secs: end,
        detail: String::new(),
    }
}

#[test]
fn merge_collapses_overlapping_and_touching_ranges_earliest_first() {
    let merged = merge_source_ranges(&[
        proposal(5.0, 7.0),
        proposal(1.0, 3.0),
        proposal(6.5, 9.0),
        proposal(3.0, 5.0), // touches [1,3)
    ]);
    assert_eq!(
        merged,
        vec![SourceRange {
            start_secs: 1.0,
            end_secs: 9.0
        }]
    );
}

#[test]
fn merge_keeps_disjoint_ranges_and_drops_empty_ones() {
    let merged = merge_source_ranges(&[
        proposal(1.0, 2.0),
        proposal(5.0, 5.0), // empty -> dropped
        proposal(8.0, 9.0),
    ]);
    assert_eq!(
        merged,
        vec![
            SourceRange {
                start_secs: 1.0,
                end_secs: 2.0
            },
            SourceRange {
                start_secs: 8.0,
                end_secs: 9.0
            }
        ]
    );
}

// --- map_source_range_to_timeline ---

#[test]
fn maps_a_source_range_onto_the_clip_at_unity_speed() {
    let clip = test_clip(100.0, 5.0, 15.0, 1.0);
    let mapped = map_source_range_to_timeline(&clip, 8.0, 10.0);
    assert_eq!(mapped, Some((103.0, 105.0)));
}

#[test]
fn maps_a_source_range_scaled_by_speed_factor() {
    // 2x speed: 2 source seconds -> 1 timeline second.
    let clip = test_clip(0.0, 0.0, 20.0, 2.0);
    let mapped = map_source_range_to_timeline(&clip, 6.0, 8.0);
    assert_eq!(mapped, Some((3.0, 4.0)));
}

#[test]
fn clips_a_range_that_straddles_the_clips_visible_source() {
    let clip = test_clip(100.0, 5.0, 12.0, 1.0);
    let mapped = map_source_range_to_timeline(&clip, 10.0, 15.0);
    assert_eq!(mapped, Some((105.0, 107.0)));
}

#[test]
fn returns_none_for_a_range_outside_the_clips_source() {
    let clip = test_clip(100.0, 5.0, 12.0, 1.0);
    assert_eq!(map_source_range_to_timeline(&clip, 20.0, 25.0), None);
}

#[test]
fn returns_none_for_a_non_positive_speed_factor() {
    let clip = test_clip(0.0, 0.0, 20.0, 0.0);
    assert_eq!(map_source_range_to_timeline(&clip, 1.0, 2.0), None);
}
