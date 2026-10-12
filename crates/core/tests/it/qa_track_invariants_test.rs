// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! QA sweep over `Track` editing: random sequences of edits checked against invariants that must
//! hold whatever the user does (no NaN, no negative start, no zero-length leftovers, ids stay
//! unique, edits that report `false` change nothing). Deterministic: a fixed-seed generator, so a
//! failure reproduces from the printed seed and step.

use avcore::timeline::{AudioRole, ClipInstance, Track, TrackKind};

/// Small deterministic generator (SplitMix64), enough for test inputs without a dependency.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[low, high)`, rounded to a millisecond like a timeline position would be.
    fn secs(&mut self, low: f64, high: f64) -> f64 {
        let unit = (self.next() >> 11) as f64 / (1u64 << 53) as f64;
        ((low + (high - low) * unit) * 1000.0).round() / 1000.0
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

fn empty_track() -> Track {
    Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    }
}

/// Back-to-back clips of random length, starting at 0; some with a speed change or a ramp.
fn random_track(rng: &mut Rng, next_id: &mut u64) -> Track {
    let mut track = empty_track();
    let mut cursor = 0.0;
    for _ in 0..(2 + rng.below(5)) {
        let source_in = rng.secs(0.0, 5.0);
        let source_len = rng.secs(0.5, 12.0);
        let mut clip = ClipInstance::new(*next_id, 1, cursor, source_in, source_in + source_len);
        *next_id += 1;
        match rng.below(4) {
            0 => clip.speed_factor = [0.5, 2.0, 1.5][rng.below(3)],
            1 => {
                clip.speed_factor = 0.5;
                clip.speed_ramp_end_factor = Some(2.0);
            }
            _ => {}
        }
        cursor += clip.duration_secs();
        track.clips.push(clip);
    }
    track
}

fn total_duration(track: &Track) -> f64 {
    track.clips.iter().map(ClipInstance::duration_secs).sum()
}

fn assert_sane(track: &Track, context: &str) {
    let mut ids: Vec<u64> = track.clips.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "{context}: duplicate clip ids");
    for clip in &track.clips {
        assert!(
            clip.start_secs.is_finite() && clip.start_secs >= 0.0,
            "{context}: start {clip:?}"
        );
        assert!(clip.source_in_secs >= 0.0, "{context}: source_in {clip:?}");
        let duration = clip.duration_secs();
        assert!(
            duration.is_finite() && duration > 0.0,
            "{context}: duration {duration} {clip:?}"
        );
        assert!(
            clip.source_out_secs > clip.source_in_secs,
            "{context}: empty source {clip:?}"
        );
    }
}

#[test]
fn splitting_never_changes_the_total_length_or_leaves_a_gap() {
    for seed in 0..200 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        let before = total_duration(&track);
        let end = track.duration_secs();
        for step in 0..6 {
            let at = rng.secs(0.0, end);
            if track.split_clip_at(at, next_id) {
                next_id += 1;
            }
            let context = format!("seed {seed} step {step} split at {at}");
            assert_sane(&track, &context);
            assert!(
                (total_duration(&track) - before).abs() < 1e-3,
                "{context}: length drifted"
            );
            let mut ordered: Vec<&ClipInstance> = track.clips.iter().collect();
            ordered.sort_by(|a, b| a.start_secs.total_cmp(&b.start_secs));
            for pair in ordered.windows(2) {
                let first_end = pair[0].start_secs + pair[0].duration_secs();
                assert!(
                    (first_end - pair[1].start_secs).abs() < 1e-3,
                    "{context}: gap or overlap between {} and {}",
                    pair[0].id,
                    pair[1].id
                );
            }
        }
    }
}

#[test]
fn a_split_keeps_every_source_frame_exactly_once() {
    for seed in 0..200 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        let source_before: f64 = track
            .clips
            .iter()
            .map(|c| c.source_out_secs - c.source_in_secs)
            .sum();
        let at = rng.secs(0.0, track.duration_secs());
        track.split_clip_at(at, next_id);
        let source_after: f64 = track
            .clips
            .iter()
            .map(|c| c.source_out_secs - c.source_in_secs)
            .sum();
        assert!(
            (source_before - source_after).abs() < 1e-6,
            "seed {seed}: source range changed"
        );
        // The two halves of a split meet in the source: no frame skipped, none repeated.
        for pair in track.clips.windows(2) {
            if pair[1].id == next_id {
                assert!(
                    (pair[0].source_out_secs - pair[1].source_in_secs).abs() < 1e-9,
                    "seed {seed}: halves do not meet in the source"
                );
            }
        }
    }
}

#[test]
fn ripple_delete_closes_exactly_the_removed_span() {
    for seed in 0..300 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        let before = total_duration(&track);
        let a = rng.secs(0.0, before);
        let b = rng.secs(0.0, before);
        let (start, end) = (a.min(b), a.max(b));
        let applied = track.ripple_delete_range(start, end, &mut next_id);
        let context = format!("seed {seed} delete {start}..{end}");
        if !applied {
            assert!((total_duration(&track) - before).abs() < 1e-9, "{context}");
            continue;
        }
        if !track.clips.is_empty() {
            assert_sane(&track, &context);
        }
        let removed = end - start;
        assert!(
            (total_duration(&track) - (before - removed)).abs() < 2e-3,
            "{context}: expected {} got {}",
            before - removed,
            total_duration(&track)
        );
        // Still back-to-back from 0: nothing left dangling after the ripple.
        let mut ordered: Vec<&ClipInstance> = track.clips.iter().collect();
        ordered.sort_by(|x, y| x.start_secs.total_cmp(&y.start_secs));
        let mut cursor = 0.0;
        for clip in ordered {
            assert!(
                (clip.start_secs - cursor).abs() < 2e-3,
                "{context}: gap before {}",
                clip.id
            );
            cursor = clip.start_secs + clip.duration_secs();
        }
    }
}

#[test]
fn lift_removes_the_range_and_moves_nothing() {
    for seed in 0..300 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        let before = total_duration(&track);
        let a = rng.secs(0.0, before);
        let b = rng.secs(0.0, before);
        let (start, end) = (a.min(b), a.max(b));
        if !track.lift_range(start, end, &mut next_id) {
            continue;
        }
        let context = format!("seed {seed} lift {start}..{end}");
        if !track.clips.is_empty() {
            assert_sane(&track, &context);
        }
        assert!(
            (total_duration(&track) - (before - (end - start))).abs() < 2e-3,
            "{context}"
        );
        for clip in &track.clips {
            let clip_end = clip.start_secs + clip.duration_secs();
            assert!(
                clip_end <= start + 1e-3 || clip.start_secs >= end - 1e-3,
                "{context}: clip {} still inside the lifted range",
                clip.id
            );
        }
    }
}

#[test]
fn a_rejected_edit_leaves_the_track_untouched() {
    for seed in 0..300 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        let id = track.clips[rng.below(track.clips.len())].id;
        let snapshot = track.clone();
        let target = rng.secs(-3.0, 40.0);
        let min = rng.secs(0.0, 2.0);
        let applied = match rng.below(5) {
            0 => track.roll_edit(id, target, min, Some(30.0)),
            1 => track.slide_clip(id, target, min, Some(30.0)),
            2 => track.ripple_trim_start(id, target, min),
            3 => track.ripple_trim_end(id, target, min, Some(30.0)),
            _ => track.move_clip(id, target),
        };
        if !applied {
            assert_eq!(
                track, snapshot,
                "seed {seed}: a rejected edit changed the track"
            );
        } else {
            assert_sane(&track, &format!("seed {seed} edit to {target}"));
        }
    }
}

#[test]
fn rolling_an_edit_keeps_the_pair_span_and_every_other_clip_in_place() {
    for seed in 0..300 {
        let mut rng = Rng(seed);
        let mut next_id = 1;
        let mut track = random_track(&mut rng, &mut next_id);
        for clip in &mut track.clips {
            clip.speed_factor = 1.0;
            clip.speed_ramp_end_factor = None;
        }
        let first = track.clips[0].clone();
        let pair_end = track.clips[1].start_secs + track.clips[1].duration_secs();
        let boundary = rng.secs(first.start_secs, pair_end);
        let others: Vec<(u64, f64)> = track.clips[2..]
            .iter()
            .map(|c| (c.id, c.start_secs))
            .collect();
        if !track.roll_edit(first.id, boundary, 0.05, None) {
            continue;
        }
        let context = format!("seed {seed} roll to {boundary}");
        let new_pair_end = track.clips[1].start_secs + track.clips[1].duration_secs();
        assert!(
            (new_pair_end - pair_end).abs() < 1e-3,
            "{context}: pair span changed"
        );
        assert!(
            (track.clips[0].start_secs - first.start_secs).abs() < 1e-9,
            "{context}"
        );
        for (id, start) in others {
            let clip = track.clips.iter().find(|c| c.id == id).unwrap();
            assert!(
                (clip.start_secs - start).abs() < 1e-9,
                "{context}: clip {id} moved"
            );
        }
    }
}

#[test]
fn nonsense_arguments_are_refused_not_applied() {
    let mut next_id = 100;
    let mut track = empty_track();
    track.clips.push(ClipInstance::new(1, 1, 0.0, 0.0, 10.0));
    let snapshot = track.clone();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        track.split_clip_at(bad, next_id);
        track.move_clip(1, bad);
        track.ripple_delete_range(bad, 5.0, &mut next_id);
        track.ripple_delete_range(1.0, bad, &mut next_id);
        track.lift_range(bad, bad, &mut next_id);
        track.slide_clip(1, bad, 0.1, None);
        track.ripple_trim_start(1, bad, 0.1);
        track.ripple_trim_end(1, bad, 0.1, None);
        assert_eq!(track, snapshot, "{bad} changed the track");
    }
}

#[test]
fn edits_that_change_nothing_report_false() {
    let mut next_id = 100;
    let mut track = empty_track();
    track.clips.push(ClipInstance::new(1, 1, 0.0, 0.0, 10.0));
    let snapshot = track.clone();
    // Past the end of the content: nothing to remove, nothing to move.
    assert!(!track.ripple_delete_range(20.0, 30.0, &mut next_id));
    assert!(!track.lift_range(20.0, 30.0, &mut next_id));
    assert!(!track.clips[0].slip(0.0, None));
    assert_eq!(track, snapshot);
    assert_eq!(next_id, 100, "no id is spent on an edit that did nothing");
}

#[test]
fn a_track_mix_is_neutral_by_default_clamped_and_follows_the_pan_law() {
    use avcore::timeline::TrackMix;

    let neutral = TrackMix::default();
    assert!(neutral.is_neutral());
    assert_eq!(
        neutral.pan_gains(),
        (1.0, 1.0),
        "a centred track is left alone"
    );
    assert!((neutral.linear_gain() - 1.0).abs() < 1e-6);

    // Out-of-range and non-finite values are brought inside the limits.
    let wild = TrackMix {
        volume_db: 500.0,
        pan: -9.0,
        ..Default::default()
    }
    .clamped();
    assert_eq!((wild.volume_db, wild.pan), (TrackMix::MAX_VOLUME_DB, -1.0));
    let broken = TrackMix {
        volume_db: f32::NAN,
        pan: f32::INFINITY,
        ..Default::default()
    }
    .clamped();
    assert!(broken.is_neutral());

    // The far side falls off to silence at the extreme, the near side stays at unity.
    let (left, right) = TrackMix {
        volume_db: 0.0,
        pan: 1.0,
        ..Default::default()
    }
    .pan_gains();
    assert!(left < 1e-6 && right == 1.0);
    let (left, right) = TrackMix {
        volume_db: 0.0,
        pan: -1.0,
        ..Default::default()
    }
    .pan_gains();
    assert!(right < 1e-6 && left == 1.0);
    // Symmetric, and monotonic as the pan moves.
    let mut previous = 1.0;
    for step in 0..=20 {
        let pan = step as f32 / 20.0;
        let (far, near) = TrackMix {
            volume_db: 0.0,
            pan,
            ..Default::default()
        }
        .pan_gains();
        let (mirror_near, mirror_far) = TrackMix {
            volume_db: 0.0,
            pan: -pan,
            ..Default::default()
        }
        .pan_gains();
        assert_eq!((far, near), (mirror_far, mirror_near));
        assert!(far <= previous + 1e-6 && near == 1.0);
        previous = far;
    }
    assert!(
        (TrackMix {
            volume_db: -6.0,
            pan: 0.0,
            ..Default::default()
        }
        .linear_gain()
            - 0.501187)
            .abs()
            < 1e-5
    );
}

#[test]
fn a_mixer_strip_is_neutral_by_default_and_every_control_stays_inside_its_limits() {
    use avcore::timeline::TrackMix;

    let neutral = TrackMix::default();
    assert!(neutral.is_neutral() && neutral.strip_is_neutral());
    // A fader move alone leaves the strip untouched.
    let fader = TrackMix {
        volume_db: -3.0,
        ..Default::default()
    };
    assert!(!fader.is_neutral() && fader.strip_is_neutral());
    // Any one tone or dynamics control makes the strip active.
    for touched in [
        TrackMix {
            eq_low_db: 3.0,
            ..Default::default()
        },
        TrackMix {
            eq_mid_db: -3.0,
            ..Default::default()
        },
        TrackMix {
            eq_high_db: 3.0,
            ..Default::default()
        },
        TrackMix {
            highpass_hz: 80.0,
            ..Default::default()
        },
        TrackMix {
            lowpass_hz: 9000.0,
            ..Default::default()
        },
        TrackMix {
            comp_ratio: 4.0,
            ..Default::default()
        },
    ] {
        assert!(!touched.strip_is_neutral(), "{touched:?}");
    }
    // A threshold with no ratio does nothing.
    assert!(TrackMix {
        comp_threshold_db: -40.0,
        ..Default::default()
    }
    .strip_is_neutral());

    let wild = TrackMix {
        eq_low_db: 99.0,
        eq_mid_db: -99.0,
        eq_high_db: 99.0,
        highpass_hz: 5.0,
        lowpass_hz: 50.0,
        comp_threshold_db: 12.0,
        comp_ratio: 500.0,
        ..Default::default()
    }
    .clamped();
    assert_eq!(wild.eq_low_db, TrackMix::MAX_EQ_DB);
    assert_eq!(wild.eq_mid_db, -TrackMix::MAX_EQ_DB);
    assert_eq!(wild.eq_high_db, TrackMix::MAX_EQ_DB);
    assert_eq!(wild.highpass_hz, TrackMix::MIN_HIGHPASS_HZ);
    assert_eq!(wild.lowpass_hz, TrackMix::MIN_LOWPASS_HZ);
    assert_eq!(wild.comp_threshold_db, 0.0);
    assert_eq!(wild.comp_ratio, TrackMix::MAX_COMP_RATIO);

    // A corner at zero or below stays off instead of snapping to the minimum.
    let open = TrackMix {
        highpass_hz: -3.0,
        lowpass_hz: 0.0,
        ..Default::default()
    }
    .clamped();
    assert_eq!((open.highpass_hz, open.lowpass_hz), (0.0, 0.0));

    let broken = TrackMix {
        eq_low_db: f32::NAN,
        highpass_hz: f32::INFINITY,
        lowpass_hz: f32::NAN,
        comp_threshold_db: f32::NAN,
        comp_ratio: f32::NAN,
        ..Default::default()
    }
    .clamped();
    assert!(broken.is_neutral());
}

#[test]
fn a_hard_cut_gets_a_declick_fade_and_a_split_does_not() {
    let mut track = empty_track();
    // Two clips of one asset split from a single take: the source carries on across the join.
    let first = ClipInstance::new(1, 5, 0.0, 10.0, 12.0);
    let second = ClipInstance::new(2, 5, 2.0, 12.0, 14.0);
    // A jump cut: the same asset, but the source skips ahead.
    let jump = ClipInstance::new(3, 5, 4.0, 20.0, 22.0);
    // A different asset straight after.
    let other = ClipInstance::new(4, 6, 6.0, 0.0, 2.0);
    // A clip with a gap before it.
    let apart = ClipInstance::new(5, 5, 9.0, 0.0, 1.0);
    track.clips = vec![first, second, jump, other, apart];
    let clips = track.clips.clone();
    let declick = Track::AUDIO_DECLICK_SECS;

    assert_eq!(
        track.audio_fade_in_secs(&clips[0]),
        0.0,
        "nothing before it"
    );
    assert_eq!(
        track.audio_fade_out_secs(&clips[0]),
        0.0,
        "a split is seamless"
    );
    assert_eq!(track.audio_fade_in_secs(&clips[1]), 0.0);
    assert_eq!(
        track.audio_fade_out_secs(&clips[1]),
        declick,
        "the jump follows"
    );
    assert_eq!(track.audio_fade_in_secs(&clips[2]), declick);
    assert_eq!(
        track.audio_fade_out_secs(&clips[2]),
        declick,
        "another asset follows"
    );
    assert_eq!(track.audio_fade_in_secs(&clips[3]), declick);
    assert_eq!(
        track.audio_fade_out_secs(&clips[3]),
        0.0,
        "a gap follows, no neighbour to meet"
    );
    assert_eq!(track.audio_fade_in_secs(&clips[4]), 0.0);

    assert!(!track.audio_has_hard_cut(&clips[0]));
    assert!(track.audio_has_hard_cut(&clips[1]));
    assert!(track.audio_has_hard_cut(&clips[2]));
    assert!(!track.audio_has_hard_cut(&clips[4]));

    // A longer fade handle wins over the declick.
    track.clips[2].fade_in_secs = 0.5;
    let longer = track.clips[2].clone();
    assert_eq!(track.audio_fade_in_secs(&longer), 0.5);
}
