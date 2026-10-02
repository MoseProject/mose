use std::path::PathBuf;

use avcore::timeline::{ClipInstance, TrackKind};
use avcore::{MediaAsset, MediaKind, Project};

use super::{add_asset, delete, move_clip_in, split};
use crate::state::blank_project;

fn asset(id: u64, kind: MediaKind, duration_secs: f64) -> MediaAsset {
    MediaAsset {
        id,
        file_name: format!("asset-{id}"),
        source_path: PathBuf::from(format!("asset-{id}")),
        kind,
        has_audio: true,
        duration_secs,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: None,
        fps: None,
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    }
}

fn project_with(media: Vec<MediaAsset>) -> Project {
    let mut project = blank_project("p".into(), "s".into());
    project.media_library = media;
    project
}

#[test]
fn adding_assets_appends_to_the_matching_track_kind() {
    let mut project = project_with(vec![
        asset(1, MediaKind::Video, 4.0),
        asset(2, MediaKind::Audio, 3.0),
    ]);

    let first = add_asset(&mut project, 1, None, None).unwrap();
    let second = add_asset(&mut project, 1, None, None).unwrap();
    add_asset(&mut project, 2, None, None).unwrap();

    let tracks = &project.timeline().tracks;
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].kind, TrackKind::Video);
    assert_eq!(tracks[0].clips[0].id, first);
    assert_eq!(tracks[0].clips[1].id, second);
    assert_eq!(tracks[0].clips[1].start_secs, 4.0);
    assert_eq!(tracks[1].kind, TrackKind::Audio);
    assert_eq!(project.recent_asset_ids, vec![2, 1]);
}

#[test]
fn unknown_assets_are_rejected() {
    let mut project = project_with(Vec::new());
    assert!(add_asset(&mut project, 9, None, None).is_none());
    assert!(project.timeline().tracks.is_empty());
}

#[test]
fn locked_tracks_refuse_moves_and_deletes() {
    let mut project = project_with(vec![asset(1, MediaKind::Video, 4.0)]);
    let clip = add_asset(&mut project, 1, None, None).unwrap();
    let timeline = project.timeline_mut();
    let track_id = timeline.tracks[0].id;
    timeline.tracks[0].locked = true;

    assert!(!move_clip_in(timeline, clip, track_id, 2.0));
    assert!(!delete(timeline, &[clip], false));
    assert_eq!(timeline.tracks[0].clips.len(), 1);
}

#[test]
fn clips_move_across_tracks_of_the_same_kind() {
    let mut project = project_with(vec![asset(1, MediaKind::Video, 4.0)]);
    let clip = add_asset(&mut project, 1, None, None).unwrap();
    let timeline = project.timeline_mut();
    let index = timeline.create_track(TrackKind::Video);
    let target = timeline.tracks[index].id;

    assert!(move_clip_in(timeline, clip, target, 1.5));
    assert!(timeline.tracks[0].clips.is_empty());
    assert_eq!(timeline.tracks[index].clips[0].start_secs, 1.5);
}

#[test]
fn split_without_selection_cuts_every_track_under_the_playhead() {
    let mut project = project_with(vec![
        asset(1, MediaKind::Video, 4.0),
        asset(2, MediaKind::Audio, 4.0),
    ]);
    add_asset(&mut project, 1, None, None);
    add_asset(&mut project, 2, None, None);
    let timeline = project.timeline_mut();
    timeline.playhead_secs = 1.0;

    assert!(split(timeline, &[]));
    assert!(timeline.tracks.iter().all(|t| t.clips.len() == 2));
    let ids: Vec<u64> = timeline
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|c| c.id))
        .collect();
    let mut unique = ids.clone();
    unique.sort_unstable();
    unique.dedup();
    assert_eq!(ids.len(), unique.len(), "split clip ids must stay unique");
}

#[test]
fn ripple_delete_closes_the_gap() {
    let mut project = project_with(vec![asset(1, MediaKind::Video, 2.0)]);
    let first = add_asset(&mut project, 1, None, None).unwrap();
    let second = add_asset(&mut project, 1, None, None).unwrap();
    let timeline = project.timeline_mut();

    assert!(delete(timeline, &[first], true));
    let clips: Vec<&ClipInstance> = timeline.tracks[0].clips.iter().collect();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0].id, second);
    assert_eq!(clips[0].start_secs, 0.0);
}
