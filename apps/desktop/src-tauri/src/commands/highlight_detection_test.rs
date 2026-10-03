use avcore::timeline::{AudioRole, MarkerKind};

use super::*;
use crate::commands::timeline::{add_asset_to_timeline, add_track};
use crate::testing::{harness, video_asset, Harness};

/// Game audio and a mic track that are both loud for the whole 6 s asset.
fn loud_pair() -> Harness {
    let h = harness();
    let mut asset = video_asset(1, 6.0);
    asset.waveform_peaks = Some(vec![(-0.9, 0.9); 12]);
    h.session().project.media_library.push(asset);
    add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    add_track(h.handle(), avcore::timeline::TrackKind::Video).unwrap();
    let second = h.session().project.timeline().tracks[1].id;
    add_asset_to_timeline(h.handle(), 1, Some(second), Some(0.0)).unwrap();
    {
        let mut session = h.session();
        let tracks = &mut session.project.timeline_mut().tracks;
        tracks[0].audio_role = AudioRole::GameAudio;
        tracks[1].audio_role = AudioRole::Mic;
    }
    h
}

#[test]
fn loud_game_audio_under_a_loud_mic_becomes_a_numbered_highlight_marker() {
    let h = loud_pair();
    detect_highlights(h.handle()).unwrap();
    let session = h.session();
    let markers = &session.project.timeline().markers;
    assert!(!markers.is_empty());
    assert!(markers.iter().all(|m| m.kind == MarkerKind::Highlight));
    assert_eq!(markers[0].label, "Highlight 1");
}

#[test]
fn a_second_detection_continues_the_numbering() {
    let h = loud_pair();
    detect_highlights(h.handle()).unwrap();
    let first = h.session().project.timeline().markers.len();
    detect_highlights(h.handle()).unwrap();
    let session = h.session();
    let markers = &session.project.timeline().markers;
    assert!(markers.len() > first);
    assert_eq!(markers[first].label, format!("Highlight {}", first + 1));
}

#[test]
fn nothing_is_marked_without_both_audio_roles_or_waveform_data() {
    let h = loud_pair();
    let events = h.count_session_events();
    h.session().project.timeline_mut().tracks[1].audio_role = AudioRole::Music;
    detect_highlights(h.handle()).unwrap();
    h.session().project.timeline_mut().tracks[1].audio_role = AudioRole::Unspecified;
    detect_highlights(h.handle()).unwrap();
    h.session().project.timeline_mut().tracks[1].audio_role = AudioRole::Mic;
    h.session().project.media_library[0].waveform_peaks = None;
    detect_highlights(h.handle()).unwrap();
    h.session().project.media_library.clear();
    detect_highlights(h.handle()).unwrap();
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 0);
}

#[test]
fn quiet_audio_has_no_highlights() {
    let h = loud_pair();
    h.session().project.media_library[0].waveform_peaks = Some(vec![(-0.01, 0.01); 12]);
    let events = h.count_session_events();
    detect_highlights(h.handle()).unwrap();
    assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 0);
}
