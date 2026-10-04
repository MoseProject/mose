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

//! Composite clip groups, multicam groups, track lookup and the refusal paths of the track
//! edit operations.

use std::collections::{HashMap, HashSet};

use avcore::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};

fn timeline() -> Timeline {
    Timeline {
        tracks: Vec::new(),
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

fn clip(id: u64, asset: u64, start: f64, source_in: f64, source_out: f64) -> ClipInstance {
    ClipInstance::new(id, asset, start, source_in, source_out)
}

/// One video track with the given clips.
fn video_timeline(clips: Vec<ClipInstance>) -> Timeline {
    let mut timeline = timeline();
    let index = timeline.create_track(TrackKind::Video);
    timeline.tracks[index].clips = clips;
    timeline
}

#[test]
fn a_preferred_track_wins_over_the_first_track_of_the_kind() {
    let mut timeline = timeline();
    timeline.create_track(TrackKind::Video);
    timeline.create_track(TrackKind::Video);
    let second = timeline.tracks[1].id;
    assert_eq!(
        timeline.resolve_or_create_track(TrackKind::Video, Some(second)),
        1
    );
    // A preferred id that does not exist falls back to the first track of the kind, and a
    // kind with no track gets a new one.
    assert_eq!(
        timeline.resolve_or_create_track(TrackKind::Video, Some(999)),
        0
    );
    assert_eq!(
        timeline.resolve_or_create_track(TrackKind::Audio, Some(second)),
        2
    );
    assert_eq!(timeline.tracks[2].name, "A1");
    assert_eq!(TrackKind::Text.name_prefix(), "T");
    assert_eq!(TrackKind::Shape.name_prefix(), "S");
    assert_eq!(TrackKind::Audio.name_prefix(), "A");
}

#[test]
fn moving_a_clip_between_tracks_refuses_bad_requests() {
    let mut timeline = video_timeline(vec![clip(1, 1, 0.0, 0.0, 2.0)]);
    let other = timeline.create_track(TrackKind::Video);
    let target = timeline.tracks[other].id;
    assert!(!timeline.move_clip_to_track(1, target, -1.0));
    assert!(!timeline.move_clip_to_track(99, target, 0.0));
    assert!(!timeline.move_clip_to_track(1, 999, 0.0));
    assert!(timeline.move_clip_to_track(1, target, 3.0));
    assert_eq!(timeline.tracks[other].clips[0].start_secs, 3.0);
}

#[test]
fn composite_clips_move_copy_and_paste_as_one_group() {
    let mut timeline = video_timeline(vec![
        clip(1, 1, 0.0, 0.0, 1.0),
        clip(2, 1, 1.0, 0.0, 1.0),
        clip(3, 1, 5.0, 0.0, 1.0),
    ]);
    // Merging needs two clips on one track.
    assert!(!timeline.merge_into_composite(&HashSet::from([1])));
    assert!(!timeline.merge_into_composite(&HashSet::from([1, 99])));
    assert!(timeline.merge_into_composite(&HashSet::from([1, 2])));
    let group = timeline.tracks[0].clips[0].composite_id.unwrap();
    assert_eq!(timeline.tracks[0].clips[1].composite_id, Some(group));
    assert_eq!(timeline.tracks[0].clips[2].composite_id, None);

    // Moving one member moves the whole group; an ungrouped clip moves alone; unknown is refused.
    assert!(timeline.move_clip_with_group(1, 10.0));
    assert_eq!(timeline.tracks[0].clips[0].start_secs, 10.0);
    assert_eq!(timeline.tracks[0].clips[1].start_secs, 11.0);
    assert!(timeline.move_clip_with_group(3, 20.0));
    assert_eq!(timeline.tracks[0].clips[2].start_secs, 20.0);
    assert!(!timeline.move_clip_with_group(99, 0.0));

    // Copying a member copies the group, copying an ungrouped clip copies just it.
    let (copied, kind) = timeline.copy_clip_group(2).unwrap();
    assert_eq!((copied.len(), kind), (2, TrackKind::Video));
    assert_eq!(timeline.copy_clip_group(3).unwrap().0.len(), 1);
    assert!(timeline.copy_clip_group(99).is_none());

    // Pasting keeps the relative offsets from the playhead and starts a new group.
    assert!(timeline
        .paste_clip_group(Vec::new(), TrackKind::Video, 0.0)
        .is_empty());
    let ids = timeline.paste_clip_group(copied, TrackKind::Video, 40.0);
    assert_eq!(ids.len(), 2);
    let pasted: Vec<_> = timeline.tracks[0]
        .clips
        .iter()
        .filter(|c| ids.contains(&c.id))
        .collect();
    assert_eq!((pasted[0].start_secs, pasted[1].start_secs), (40.0, 41.0));
    let new_group = pasted[0].composite_id.unwrap();
    assert_ne!(new_group, group);
    assert_eq!(pasted[1].composite_id, Some(new_group));

    // A single clip pasted is not grouped, and pasting creates the track when needed.
    let mut empty = self::timeline();
    let ids = empty.paste_clip_group(vec![clip(7, 1, 3.0, 0.0, 1.0)], TrackKind::Audio, 2.0);
    assert_eq!(ids.len(), 1);
    assert_eq!(empty.tracks[0].kind, TrackKind::Audio);
    assert_eq!(empty.tracks[0].clips[0].start_secs, 2.0);
    assert_eq!(empty.tracks[0].clips[0].composite_id, None);
}

#[test]
fn multicam_groups_are_validated_switched_and_removed() {
    let mut timeline = timeline();
    for _ in 0..3 {
        timeline.create_track(TrackKind::Video);
    }
    timeline.create_track(TrackKind::Audio);
    let ids: Vec<u64> = timeline.tracks.iter().map(|t| t.id).collect();
    timeline.tracks[0].clips = vec![clip(1, 1, 0.0, 0.0, 10.0)];
    timeline.tracks[1].clips = vec![clip(2, 2, 0.0, 5.0, 15.0)];

    // Fewer than two members, a program outside the group, or a non-video member: refused.
    let offsets = HashMap::new();
    assert!(timeline
        .add_multicam_group("g".into(), vec![ids[0]], ids[0], offsets.clone())
        .is_none());
    assert!(timeline
        .add_multicam_group("g".into(), vec![ids[0], ids[1]], ids[2], offsets.clone())
        .is_none());
    assert!(timeline
        .add_multicam_group("g".into(), vec![ids[0], ids[3]], ids[0], offsets.clone())
        .is_none());

    let group = timeline
        .add_multicam_group("g".into(), vec![ids[0], ids[1]], ids[0], offsets)
        .unwrap();
    assert!(timeline.multicam_group_mut(group).is_some());
    assert!(timeline.multicam_group_mut(99).is_none());
    // Non-program members are hidden while grouped.
    assert!(!timeline.tracks[1].visible);

    // Switching refuses unknown groups, angles, the program itself and uncovered times.
    assert!(!timeline.switch_multicam_angle(99, 1, 2.0, 50));
    assert!(!timeline.switch_multicam_angle(group, 9, 2.0, 50));
    assert!(!timeline.switch_multicam_angle(group, 0, 2.0, 50));
    assert!(!timeline.switch_multicam_angle(group, 1, 20.0, 50));
    // The target angle has no clip yet at this time.
    timeline.tracks[1].clips.clear();
    assert!(!timeline.switch_multicam_angle(group, 1, 2.0, 50));
    timeline.tracks[1].clips = vec![clip(2, 2, 0.0, 5.0, 15.0)];
    assert!(timeline.switch_multicam_angle(group, 1, 2.0, 50));
    let switched = timeline.tracks[0]
        .clips
        .iter()
        .find(|c| c.start_secs == 2.0)
        .unwrap();
    assert_eq!(switched.asset_id, 2);
    // Switching again exactly at the new boundary reuses it instead of splitting.
    assert!(timeline.switch_multicam_angle(group, 1, 2.0, 51));

    assert!(!timeline.remove_multicam_group(99));
    assert!(timeline.remove_multicam_group(group));
    assert!(timeline.tracks[1].visible);
    assert!(timeline.multicam_groups.is_empty());
}

#[test]
fn track_edits_refuse_unknown_clips() {
    let mut track = Track {
        id: 1,
        name: "V1".into(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 1, 0.0, 0.0, 2.0), clip(2, 1, 2.0, 0.0, 2.0)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    assert!(track.text_clip_mut(99).is_none());
    assert!(track.shape_clip_mut(99).is_none());

    assert!(!track.ripple_trim_start(99, 1.0, 0.1));
    assert!(!track.ripple_trim_end(99, 1.0, 0.1, None));
    assert!(!track.ripple_trim_end(1, 0.01, 0.5, None), "too short");
    assert!(!track.roll_edit(99, 1.0, 0.1, None));
    assert!(
        !track.roll_edit(2, 3.0, 0.1, None),
        "the last clip has no neighbour"
    );
    assert!(!track.slide_clip(99, 1.0, 0.1, None));
    assert!(!track.slide_clip(1, -1.0, 0.1, None), "before time zero");
}
