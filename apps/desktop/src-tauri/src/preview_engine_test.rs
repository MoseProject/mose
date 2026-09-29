use std::path::{Path, PathBuf};

use avcore::preview_worker::PreviewSeek;
use avcore::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};
use avcore::{MediaAsset, MediaKind};

use super::{live_updates_between, resolve_layers, seek_for, timeline_position};

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/core/tests/fixtures/video.mp4")
}

fn asset(id: u64, source_path: PathBuf) -> MediaAsset {
    MediaAsset {
        id,
        file_name: format!("asset-{id}.mp4"),
        source_path,
        kind: MediaKind::Video,
        has_audio: true,
        duration_secs: 10.0,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: Some((1920, 1080)),
        fps: Some(30.0),
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    }
}

fn track(id: u64, kind: TrackKind, clips: Vec<ClipInstance>) -> Track {
    Track {
        id,
        name: format!("T{id}"),
        kind,
        clips,
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        color_label: None,
        locked: false,
    }
}

fn timeline(tracks: Vec<Track>) -> Timeline {
    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

#[test]
fn first_visible_video_clip_is_the_background_and_later_ones_overlay() {
    let media = vec![asset(1, fixture()), asset(2, fixture())];
    let timeline = timeline(vec![
        track(
            1,
            TrackKind::Video,
            vec![ClipInstance::new(10, 1, 0.0, 0.0, 5.0)],
        ),
        track(
            2,
            TrackKind::Video,
            vec![ClipInstance::new(20, 2, 1.0, 0.0, 2.0)],
        ),
    ]);

    let layers = resolve_layers(&timeline, &media, 1.5);

    assert_eq!(layers.background.as_ref().map(|(_, c)| c.id), Some(10));
    assert_eq!(
        layers
            .overlays
            .iter()
            .map(|(_, c)| c.id)
            .collect::<Vec<_>>(),
        vec![20]
    );
}

#[test]
fn a_gap_on_the_top_track_promotes_the_next_track_to_background() {
    let media = vec![asset(1, fixture())];
    let timeline = timeline(vec![
        track(
            1,
            TrackKind::Video,
            vec![ClipInstance::new(10, 1, 5.0, 0.0, 1.0)],
        ),
        track(
            2,
            TrackKind::Video,
            vec![ClipInstance::new(20, 1, 0.0, 0.0, 3.0)],
        ),
    ]);

    let layers = resolve_layers(&timeline, &media, 1.0);

    assert_eq!(layers.background.as_ref().map(|(_, c)| c.id), Some(20));
    assert!(layers.overlays.is_empty());
}

#[test]
fn a_compound_clip_resolves_via_its_synthetic_asset_id() {
    // Regression test for the bug this session found and fixed in `avcore::nested_sequence`/
    // `ClipInstance::resolved_asset_id`: a compound clip's own `asset_id` field is left at 0
    // (never rewritten), so it only resolves once `resolve_layers`' lookup goes through
    // `resolved_asset_id()` against the synthetic asset `Command::Content`'s handler merges in.
    let nested_sequence_id = 7u64;
    let synthetic_id = avcore::nested_sequence::synthetic_asset_id(nested_sequence_id);
    let media = vec![asset(synthetic_id, fixture())];
    let mut nested_clip = ClipInstance::new(10, 0, 0.0, 0.0, 3.0);
    nested_clip.nested_sequence_id = Some(nested_sequence_id);
    let timeline = timeline(vec![track(1, TrackKind::Video, vec![nested_clip])]);

    let layers = resolve_layers(&timeline, &media, 1.0);

    assert_eq!(layers.background.as_ref().map(|(_, c)| c.id), Some(10));
}

#[test]
fn missing_media_and_hidden_tracks_are_left_out() {
    let media = vec![asset(1, PathBuf::from("Z:/definitely/missing.mp4"))];
    let mut hidden = track(
        2,
        TrackKind::Video,
        vec![ClipInstance::new(20, 1, 0.0, 0.0, 3.0)],
    );
    hidden.visible = false;
    let timeline = timeline(vec![
        track(
            1,
            TrackKind::Video,
            vec![ClipInstance::new(10, 1, 0.0, 0.0, 3.0)],
        ),
        hidden,
    ]);

    assert_eq!(resolve_layers(&timeline, &media, 1.0), Default::default());
}

#[test]
fn composited_seek_orders_background_then_overlays_then_audio() {
    let media = vec![asset(1, fixture())];
    let mut sped_up = ClipInstance::new(30, 1, 0.0, 2.0, 6.0);
    sped_up.speed_factor = 2.0;
    let timeline = timeline(vec![
        track(
            1,
            TrackKind::Video,
            vec![ClipInstance::new(10, 1, 0.0, 1.0, 5.0)],
        ),
        track(2, TrackKind::Audio, vec![sped_up]),
    ]);
    let layers = resolve_layers(&timeline, &media, 1.0);

    let Some(PreviewSeek::Composited { offsets, rates }) = seek_for(&layers, 1.0) else {
        panic!("audio branch should make the preview composited");
    };
    assert_eq!(offsets, vec![2.0, 4.0]);
    assert_eq!(rates, vec![1.0, 2.0]);
}

#[test]
fn single_clip_seek_uses_the_clip_offset() {
    let media = vec![asset(1, fixture())];
    let timeline = timeline(vec![track(
        1,
        TrackKind::Video,
        vec![ClipInstance::new(10, 1, 2.0, 3.0, 8.0)],
    )]);
    let layers = resolve_layers(&timeline, &media, 4.0);

    let Some(PreviewSeek::Single { offset_secs, rate }) = seek_for(&layers, 4.0) else {
        panic!("a lone video clip should use a single pipeline");
    };
    assert_eq!(offset_secs, 5.0);
    assert_eq!(rate, 1.0);
}

#[test]
fn source_position_maps_back_to_timeline_and_clamps_at_the_out_point() {
    let mut clip = ClipInstance::new(1, 1, 10.0, 4.0, 8.0);
    clip.speed_factor = 2.0;

    assert_eq!(timeline_position(&clip, 6.0), 11.0);
    assert_eq!(timeline_position(&clip, 9.0), 12.0);
}

#[test]
fn live_video_property_changes_keep_the_loaded_layers() {
    let media = vec![asset(1, fixture())];
    let before = timeline(vec![track(
        1,
        TrackKind::Video,
        vec![ClipInstance::new(10, 1, 0.0, 0.0, 3.0)],
    )]);
    let mut after = before.clone();
    after.tracks[0].clips[0].brightness = 0.25;
    let before = resolve_layers(&before, &media, 1.0);
    let after = resolve_layers(&after, &media, 1.0);
    assert!(live_updates_between(&before, &after).is_some());
}

#[test]
fn trim_changes_reopen_the_loaded_layers() {
    let media = vec![asset(1, fixture())];
    let before = timeline(vec![track(
        1,
        TrackKind::Video,
        vec![ClipInstance::new(10, 1, 0.0, 0.0, 3.0)],
    )]);
    let mut after = before.clone();
    after.tracks[0].clips[0].source_out_secs = 2.0;
    let before = resolve_layers(&before, &media, 1.0);
    let after = resolve_layers(&after, &media, 1.0);
    assert!(live_updates_between(&before, &after).is_none());
}
