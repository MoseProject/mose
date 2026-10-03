use super::*;
use crate::commands::timeline::add_asset_to_timeline;
use crate::testing::{harness, video_asset, Harness};

/// A 6 s asset: loud for 2 s, silent for 2 s, loud for 2 s, in 0.5 s buckets, on one track.
fn gapped() -> (Harness, u64) {
    let h = harness();
    let mut asset = video_asset(1, 6.0);
    let loud = (-0.8, 0.8);
    let mut peaks = vec![loud; 4];
    peaks.extend(vec![(0.0, 0.0); 4]);
    peaks.extend(vec![loud; 4]);
    asset.waveform_peaks = Some(peaks);
    h.session().project.media_library.push(asset);
    add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    let clip = h.session().project.timeline().tracks[0].clips[0].id;
    (h, clip)
}

#[test]
fn a_silent_stretch_inside_a_clip_is_reported_for_review() {
    let (h, clip) = gapped();
    let review = detect_silence_gaps_for_clip(h.state(), clip).unwrap();
    assert_eq!(review.track_id, h.session().project.timeline().tracks[0].id);
    assert_eq!(review.gaps.len(), 1);
    assert!((review.gaps[0].start_secs - 2.0).abs() < 0.3);
    assert!((review.gaps[0].end_secs - 4.0).abs() < 0.3);
}

#[test]
fn clips_without_waveform_data_or_assets_contribute_no_gaps_and_unknown_clips_have_no_review() {
    let (h, clip) = gapped();
    h.session().project.media_library[0].waveform_peaks = None;
    assert!(detect_silence_gaps_for_clip(h.state(), clip)
        .unwrap()
        .gaps
        .is_empty());
    h.session().project.media_library.clear();
    assert!(detect_silence_gaps_for_clip(h.state(), clip)
        .unwrap()
        .gaps
        .is_empty());
    assert!(detect_silence_gaps_for_clip(h.state(), 999).is_none());
}

#[test]
fn gaps_from_several_clips_are_listed_in_time_order() {
    let (h, clip) = gapped();
    add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    let review = detect_silence_gaps_for_clip(h.state(), clip).unwrap();
    assert_eq!(review.gaps.len(), 2);
    assert!(review.gaps[0].start_secs < review.gaps[1].start_secs);
}

#[test]
fn applying_gaps_ripple_deletes_them_latest_first() {
    let (h, clip) = gapped();
    let review = detect_silence_gaps_for_clip(h.state(), clip).unwrap();
    apply_silence_gaps(h.handle(), review.track_id, review.gaps).unwrap();
    let session = h.session();
    let track = &session.project.timeline().tracks[0];
    let total: f64 = track.clips.iter().map(|c| c.duration_secs()).sum();
    assert!(total < 6.0 - 1.0, "the silent stretch is gone: {total}");
    assert!(session.dirty);
}

#[test]
fn applying_nothing_or_to_a_missing_track_changes_nothing() {
    let (h, _) = gapped();
    let events = h.count_session_events();
    let track = h.session().project.timeline().tracks[0].id;
    apply_silence_gaps(h.handle(), track, Vec::new()).unwrap();
    apply_silence_gaps(
        h.handle(),
        999,
        vec![SilenceGapView {
            start_secs: 1.0,
            end_secs: 2.0,
        }],
    )
    .unwrap();
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 0);
}
