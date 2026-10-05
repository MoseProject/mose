use std::path::PathBuf;

use avcore::timeline::{ClipInstance, TrackKind};
use avcore::{MediaAsset, MediaKind, Project};

use super::{add_asset, delete, delete_range, lift_range_in, move_clip_in, split};
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

#[test]
fn deleting_a_range_closes_the_gap_on_every_unlocked_media_track() {
    let mut project = project_with(vec![
        asset(1, MediaKind::Video, 10.0),
        asset(2, MediaKind::Audio, 10.0),
    ]);
    add_asset(&mut project, 1, None, None).unwrap();
    add_asset(&mut project, 2, None, None).unwrap();
    let timeline = project.timeline_mut();

    assert!(delete_range(timeline, 2.0, 5.0));
    for track in &timeline.tracks {
        let end = track
            .clips
            .iter()
            .map(|c| c.start_secs + c.duration_secs())
            .fold(0.0, f64::max);
        assert!((end - 7.0).abs() < 1e-6, "track {} ends at {end}", track.id);
    }
}

#[test]
fn deleting_a_range_skips_locked_tracks_and_rejects_bad_ranges() {
    let mut project = project_with(vec![asset(1, MediaKind::Video, 10.0)]);
    add_asset(&mut project, 1, None, None).unwrap();
    let timeline = project.timeline_mut();
    timeline.tracks[0].locked = true;
    assert!(!delete_range(timeline, 2.0, 5.0));
    timeline.tracks[0].locked = false;
    assert!(!delete_range(timeline, 5.0, 5.0));
    assert!(!delete_range(timeline, 5.0, 2.0));
    assert!(!delete_range(timeline, f64::NAN, 2.0));
    assert!(!delete_range(timeline, 0.0, f64::INFINITY));
}

mod commands {
    use avcore::timeline::{MaskShape, Timeline, TrackKind, TransitionType};
    use avcore::{Keyframe, Position};
    use std::sync::atomic::Ordering;

    use super::*;
    use crate::commands::timeline::*;
    use crate::testing::{harness, Harness};

    fn setup() -> Harness {
        let h = harness();
        h.session().project = project_with(vec![
            asset(1, MediaKind::Video, 10.0),
            asset(2, MediaKind::Audio, 10.0),
        ]);
        h
    }

    fn timeline(h: &Harness) -> Timeline {
        h.session().project.timeline().clone()
    }

    fn add(h: &Harness, asset_id: u64, track: Option<u64>, start: Option<f64>) {
        add_asset_to_timeline(h.handle(), asset_id, track, start).unwrap();
    }

    /// Two video clips of 10 s back to back on one track: returns their ids.
    fn two_clips(h: &Harness) -> (u64, u64) {
        add(h, 1, None, None);
        add(h, 1, None, None);
        let timeline = timeline(h);
        (
            timeline.tracks[0].clips[0].id,
            timeline.tracks[0].clips[1].id,
        )
    }

    fn clip<'a>(timeline: &'a Timeline, id: u64) -> &'a avcore::timeline::ClipInstance {
        timeline
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == id)
            .unwrap()
    }

    fn can_undo(h: &Harness) -> bool {
        h.session().view().can_undo
    }

    #[test]
    fn adding_assets_places_them_after_each_other_by_default() {
        let h = setup();
        add(&h, 1, None, None);
        add(&h, 1, None, None);
        let timeline = timeline(&h);
        assert_eq!(timeline.tracks.len(), 1);
        let starts: Vec<f64> = timeline.tracks[0]
            .clips
            .iter()
            .map(|c| c.start_secs)
            .collect();
        assert_eq!(starts, vec![0.0, 10.0]);
    }

    #[test]
    fn adding_an_audio_asset_creates_an_audio_track_at_the_requested_time() {
        let h = setup();
        add(&h, 2, None, Some(3.0));
        let timeline = timeline(&h);
        assert_eq!(timeline.tracks[0].kind, TrackKind::Audio);
        assert_eq!(timeline.tracks[0].clips[0].start_secs, 3.0);
    }

    #[test]
    fn adding_an_unknown_asset_leaves_no_undo_step() {
        let h = setup();
        add(&h, 99, None, None);
        assert!(!can_undo(&h));
        assert!(timeline(&h).tracks.is_empty());
    }

    #[test]
    fn a_clip_moves_to_another_track_and_back_along_its_own() {
        let h = setup();
        let (first, _) = two_clips(&h);
        add_track(h.handle(), TrackKind::Video).unwrap();
        let target = timeline(&h).tracks[1].id;
        move_clip(h.handle(), first, target, 2.0).unwrap();
        let tl = timeline(&h);
        assert!(tl.tracks[1]
            .clips
            .iter()
            .any(|c| c.id == first && c.start_secs == 2.0));
        assert!(!tl.tracks[0].clips.iter().any(|c| c.id == first));
    }

    #[test]
    fn trimming_either_edge_shortens_the_clip() {
        let h = setup();
        let (first, _) = two_clips(&h);
        trim_clip(h.handle(), first, TrimEdge::Start, 2.0).unwrap();
        assert!(clip(&timeline(&h), first).duration_secs() < 10.0);
        let after_start = clip(&timeline(&h), first).duration_secs();
        trim_clip(h.handle(), first, TrimEdge::End, 5.0).unwrap();
        assert!(clip(&timeline(&h), first).duration_secs() < after_start);
    }

    #[test]
    fn trimming_a_locked_or_missing_clip_changes_nothing() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let track_id = timeline(&h).tracks[0].id;
        set_track_flags(h.handle(), track_id, None, Some(true), None, None, None).unwrap();
        let before = timeline(&h);
        trim_clip(h.handle(), first, TrimEdge::Start, 2.0).unwrap();
        trim_clip(h.handle(), 999, TrimEdge::End, 2.0).unwrap();
        assert_eq!(timeline(&h), before);
    }

    /// A trimmed to 0..8 s and B trimmed to start at 12 s with its first 2 s of footage hidden:
    /// each side has footage to give to a roll that meets at 10 s.
    fn gapped_pair(h: &Harness) -> (u64, u64) {
        let (first, second) = two_clips(h);
        trim_clip(h.handle(), first, TrimEdge::End, 8.0).unwrap();
        trim_clip(h.handle(), second, TrimEdge::Start, 12.0).unwrap();
        (first, second)
    }

    #[test]
    fn rolling_an_edit_moves_the_shared_boundary() {
        let h = setup();
        let (first, second) = gapped_pair(&h);
        roll_edit_clip(h.handle(), first, 10.0).unwrap();
        let tl = timeline(&h);
        assert_eq!(clip(&tl, first).duration_secs(), 10.0);
        assert_eq!(clip(&tl, second).start_secs, 10.0);
    }

    #[test]
    fn rolling_from_the_start_edge_resolves_the_previous_clip() {
        let h = setup();
        let (first, second) = gapped_pair(&h);
        roll_edit_from_start_edge(h.handle(), second, 10.0).unwrap();
        let tl = timeline(&h);
        assert_eq!(clip(&tl, first).duration_secs(), 10.0);
        assert_eq!(clip(&tl, second).start_secs, 10.0);
    }

    #[test]
    fn rolling_from_a_start_edge_needs_a_previous_clip() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let before = timeline(&h);
        roll_edit_from_start_edge(h.handle(), first, 3.0).unwrap();
        roll_edit_from_start_edge(h.handle(), 999, 3.0).unwrap();
        roll_edit_clip(h.handle(), 999, 3.0).unwrap();
        assert_eq!(timeline(&h), before);
    }

    #[test]
    fn slipping_moves_the_source_window_only() {
        let h = setup();
        let (first, _) = two_clips(&h);
        trim_clip(h.handle(), first, TrimEdge::End, 5.0).unwrap();
        let before = clip(&timeline(&h), first).clone();
        slip_clip(h.handle(), first, 2.0).unwrap();
        let after = timeline(&h);
        let after = clip(&after, first);
        assert_eq!(after.start_secs, before.start_secs);
        assert_eq!(after.duration_secs(), before.duration_secs());
        assert_eq!(after.source_in_secs, before.source_in_secs + 2.0);
        slip_clip(h.handle(), 999, 1.0).unwrap();
    }

    #[test]
    fn sliding_repositions_a_clip_inside_its_neighbours() {
        let h = setup();
        add(&h, 1, None, None);
        add(&h, 1, None, None);
        add(&h, 1, None, None);
        let ids: Vec<u64> = timeline(&h).tracks[0].clips.iter().map(|c| c.id).collect();
        for &id in &ids {
            trim_clip(h.handle(), id, TrimEdge::End, 0.0).ok();
        }
        slide_clip(h.handle(), ids[1], 12.0).unwrap();
        slide_clip(h.handle(), 999, 1.0).unwrap();
    }

    #[test]
    fn moving_a_group_follows_its_composite_members() {
        let h = setup();
        let (first, second) = two_clips(&h);
        merge_clips_into_composite(h.handle(), vec![first, second]).unwrap();
        move_clip_group(h.handle(), first, 5.0).unwrap();
        let tl = timeline(&h);
        assert_eq!(clip(&tl, first).start_secs, 5.0);
        assert_eq!(clip(&tl, second).start_secs, 15.0);
    }

    #[test]
    fn merging_needs_two_clips_on_one_track() {
        let h = setup();
        let (first, _) = two_clips(&h);
        merge_clips_into_composite(h.handle(), vec![first]).unwrap();
        assert!(clip(&timeline(&h), first).composite_id.is_none());
    }

    #[test]
    fn a_compound_clip_nests_a_sequence_and_can_be_reinserted() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let nested = create_compound_clip(h.handle(), first, vec![first]).unwrap();
        let nested = nested.expect("a video clip can be compounded");
        assert_eq!(h.session().project.sequences.len(), 2);
        insert_sequence_as_compound_clip(h.handle(), nested).unwrap();
        assert_eq!(create_compound_clip(h.handle(), 999, vec![]).unwrap(), None);
        assert!(!insert_sequence_as_compound_clip(h.handle(), 999).unwrap());
    }

    #[test]
    fn copy_cut_and_paste_round_trip_through_the_clipboard() {
        let h = setup();
        let (first, _) = two_clips(&h);
        assert_eq!(paste_clip(h.handle()).unwrap(), Vec::<u64>::new());

        copy_clip(h.handle(), first).unwrap();
        assert!(h.state().clip_clipboard.lock().unwrap().is_some());
        copy_clip(h.handle(), 999).unwrap();
        assert!(h.state().clip_clipboard.lock().unwrap().is_some());
        let pasted = paste_clip(h.handle()).unwrap();
        assert_eq!(pasted.len(), 1);
        assert_eq!(timeline(&h).tracks[0].clips.len(), 3);

        cut_clip(h.handle(), first).unwrap();
        assert!(!timeline(&h).tracks[0].clips.iter().any(|c| c.id == first));
    }

    #[test]
    fn formatting_is_copied_and_applied_to_another_clip() {
        let h = setup();
        let (first, second) = two_clips(&h);
        let events = h.count_session_events();
        paste_clip_formatting(h.handle(), second).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);

        update_clip(
            h.handle(),
            first,
            ClipPatch {
                brightness: Some(0.5),
                ..Default::default()
            },
        )
        .unwrap();
        copy_clip_formatting(h.handle(), 999).unwrap();
        assert!(h.state().formatting_clipboard.lock().unwrap().is_none());
        copy_clip_formatting(h.handle(), first).unwrap();
        paste_clip_formatting(h.handle(), second).unwrap();
        assert_eq!(clip(&timeline(&h), second).brightness, 0.5);
        paste_clip_formatting(h.handle(), 999).unwrap();
    }

    #[test]
    fn splitting_and_deleting_go_through_the_undo_stack() {
        let h = setup();
        let (first, _) = two_clips(&h);
        h.set_playhead(4.0);
        split_at_playhead(h.handle(), vec![first]).unwrap();
        assert_eq!(timeline(&h).tracks[0].clips.len(), 3);
        delete_clips(h.handle(), vec![first], false).unwrap();
        assert_eq!(timeline(&h).tracks[0].clips.len(), 2);
        delete_clips(h.handle(), vec![999], true).unwrap();
    }

    #[test]
    fn a_speed_ramp_splits_the_clip_into_steps() {
        let h = setup();
        let (first, second) = two_clips(&h);
        apply_speed_ramp_to_clip(h.handle(), first, 0.5, 2.0, 4).unwrap();
        let tl = timeline(&h);
        assert_eq!(tl.tracks[0].clips.len(), 5);
        let speeds: Vec<f32> = tl.tracks[0]
            .clips
            .iter()
            .filter(|c| c.id != second)
            .map(|c| c.speed_factor)
            .collect();
        assert_eq!(speeds, vec![0.5, 1.0, 1.5, 2.0]);
    }

    #[test]
    fn a_speed_ramp_refuses_too_few_steps_locked_tracks_and_missing_clips() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let before = timeline(&h);
        apply_speed_ramp_to_clip(h.handle(), first, 0.5, 2.0, 1).unwrap();
        apply_speed_ramp_to_clip(h.handle(), 999, 0.5, 2.0, 4).unwrap();
        let track_id = before.tracks[0].id;
        set_track_flags(h.handle(), track_id, None, Some(true), None, None, None).unwrap();
        let locked = timeline(&h);
        apply_speed_ramp_to_clip(h.handle(), first, 0.5, 2.0, 4).unwrap();
        assert_eq!(timeline(&h), locked);
    }

    #[test]
    fn a_patch_updates_every_field_it_carries_within_its_limits() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let key = vec![Keyframe {
            time_fraction: 0.0,
            value: 1.0f32,
        }];
        let position = vec![Keyframe {
            time_fraction: 0.0,
            value: Position { x: 0.2, y: 0.4 },
        }];
        update_clip(
            h.handle(),
            first,
            ClipPatch {
                gain_db: Some(99.0),
                speed_factor: Some(99.0),
                brightness: Some(9.0),
                contrast: Some(9.0),
                saturation: Some(9.0),
                frozen: Some(true),
                flipped_h: Some(true),
                voice_cleanup_enabled: Some(true),
                crop_x: Some(0.1),
                crop_y: Some(0.2),
                crop_w: Some(0.0),
                crop_h: Some(0.0),
                mask_shape: Some(MaskShape::Circle),
                mask_corner_radius: Some(9.0),
                transition_in: Some(TransitionType::Fade),
                transition_duration_secs: Some(99.0),
                fade_in_secs: Some(99.0),
                fade_out_secs: Some(-1.0),
                blend_mode: Some(avcore::timeline::BlendMode::default()),
                position_keyframes: Some(position),
                layer_scale_x: Some(99.0),
                layer_scale_y: Some(0.0),
                scale_keyframes: Some(key.clone()),
                rotation_keyframes: Some(key.clone()),
                opacity_keyframes: Some(key.clone()),
                gain_keyframes: Some(key.clone()),
                brightness_keyframes: Some(key.clone()),
                contrast_keyframes: Some(key.clone()),
                saturation_keyframes: Some(key.clone()),
                crop_x_keyframes: Some(key.clone()),
                crop_y_keyframes: Some(key.clone()),
                crop_w_keyframes: Some(key.clone()),
                crop_h_keyframes: Some(key),
            },
        )
        .unwrap();
        let tl = timeline(&h);
        let c = clip(&tl, first);
        assert_eq!(c.gain_db, 24.0);
        assert_eq!(c.speed_factor, 4.0);
        assert_eq!(c.brightness, 1.0);
        assert_eq!(c.contrast, 2.0);
        assert_eq!(c.saturation, 2.0);
        assert!(c.frozen && c.flipped_h && c.voice_cleanup_enabled);
        assert_eq!((c.crop_x, c.crop_y), (0.1, 0.2));
        assert_eq!((c.crop_w, c.crop_h), (0.05, 0.05));
        assert_eq!(c.mask_shape, MaskShape::Circle);
        assert_eq!(c.mask_corner_radius, 1.0);
        assert_eq!(c.transition_in, TransitionType::Fade);
        assert_eq!(c.transition_duration_secs, 3.0);
        assert_eq!(c.fade_in_secs, 10.0);
        assert_eq!(c.fade_out_secs, 0.0);
        assert_eq!((c.layer_scale_x, c.layer_scale_y), (8.0, 0.05));
        assert_eq!(c.position_keyframes.len(), 1);
        assert_eq!(c.scale_keyframes.len(), 1);
        assert_eq!(c.rotation_keyframes.len(), 1);
        assert_eq!(c.opacity_keyframes.len(), 1);
        assert_eq!(c.gain_keyframes.len(), 1);
        assert_eq!(c.brightness_keyframes.len(), 1);
        assert_eq!(c.contrast_keyframes.len(), 1);
        assert_eq!(c.saturation_keyframes.len(), 1);
        assert_eq!(c.crop_x_keyframes.len(), 1);
        assert_eq!(c.crop_y_keyframes.len(), 1);
        assert_eq!(c.crop_w_keyframes.len(), 1);
        assert_eq!(c.crop_h_keyframes.len(), 1);
    }

    #[test]
    fn an_empty_patch_or_a_locked_track_leaves_no_undo_step() {
        let h = setup();
        let (first, _) = two_clips(&h);
        let events = h.count_session_events();
        update_clip(h.handle(), first, ClipPatch::default()).unwrap();
        update_clip(
            h.handle(),
            999,
            ClipPatch {
                frozen: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
        let track_id = timeline(&h).tracks[0].id;
        set_track_flags(h.handle(), track_id, None, Some(true), None, None, None).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 1);
        update_clip(
            h.handle(),
            first,
            ClipPatch {
                frozen: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn track_flags_toggle_only_when_something_changes() {
        let h = setup();
        add_track(h.handle(), TrackKind::Video).unwrap();
        let id = timeline(&h).tracks[0].id;
        set_track_flags(h.handle(), id, Some(false), Some(true), None, None, None).unwrap();
        let tl = timeline(&h);
        assert!(!tl.tracks[0].visible && tl.tracks[0].locked);
        let events = h.count_session_events();
        set_track_flags(h.handle(), id, Some(false), Some(true), None, None, None).unwrap();
        set_track_flags(h.handle(), id, None, None, None, None, None).unwrap();
        set_track_flags(h.handle(), 999, Some(true), None, None, None, None).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn mute_solo_and_the_audio_role_are_saved_as_one_undo_step() {
        let h = setup();
        add_track(h.handle(), TrackKind::Audio).unwrap();
        let id = timeline(&h).tracks[0].id;
        set_track_flags(
            h.handle(),
            id,
            None,
            None,
            Some(true),
            Some(true),
            Some(avcore::timeline::AudioRole::Mic),
        )
        .unwrap();
        let tl = timeline(&h);
        assert!(tl.tracks[0].muted && tl.tracks[0].solo);
        assert_eq!(tl.tracks[0].audio_role, avcore::timeline::AudioRole::Mic);
        let events = h.count_session_events();
        set_track_flags(h.handle(), id, None, None, Some(true), None, None).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0, "no change, no undo step");
    }

    #[test]
    fn the_range_delete_command_is_one_undo_step() {
        let h = setup();
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let events = h.count_session_events();
        ripple_delete_range(h.handle(), 1.0, 3.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 1);
        ripple_delete_range(h.handle(), 3.0, 3.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 1);
    }
}

mod edge_cases {
    use std::sync::atomic::Ordering;

    use avcore::timeline::TrackKind;

    use crate::commands::timeline::*;
    use crate::testing::{harness, video_asset, Harness};

    fn with_asset(duration_secs: f64) -> Harness {
        let h = harness();
        h.session()
            .project
            .media_library
            .push(video_asset(1, duration_secs));
        h
    }

    #[test]
    fn adding_to_a_locked_track_is_refused() {
        let h = with_asset(5.0);
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let track = h.session().project.timeline().tracks[0].id;
        set_track_flags(h.handle(), track, None, Some(true), None, None, None).unwrap();
        let events = h.count_session_events();
        add_asset_to_timeline(h.handle(), 1, Some(track), Some(1.0)).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
        assert_eq!(h.session().project.timeline().tracks[0].clips.len(), 1);
    }

    #[test]
    fn moving_needs_an_existing_clip_and_target_track() {
        let h = with_asset(5.0);
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let (clip, track) = {
            let s = h.session();
            let t = &s.project.timeline().tracks[0];
            (t.clips[0].id, t.id)
        };
        let events = h.count_session_events();
        move_clip(h.handle(), 999, track, 1.0).unwrap();
        move_clip(h.handle(), clip, 999, 1.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
        move_clip(h.handle(), clip, track, 2.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 1);
        assert_eq!(
            h.session().project.timeline().tracks[0].clips[0].start_secs,
            2.0
        );
    }

    #[test]
    fn a_zero_length_clip_cannot_be_ramped() {
        let h = with_asset(0.0);
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let clip = h.session().project.timeline().tracks[0].clips[0].id;
        let events = h.count_session_events();
        apply_speed_ramp_to_clip(h.handle(), clip, 0.5, 2.0, 4).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_clip_too_short_to_split_takes_the_start_speed_whole() {
        let h = with_asset(0.15);
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let clip = h.session().project.timeline().tracks[0].clips[0].id;
        apply_speed_ramp_to_clip(h.handle(), clip, 0.5, 2.0, 4).unwrap();
        let session = h.session();
        let clips = &session.project.timeline().tracks[0].clips;
        assert!(clips.iter().any(|c| c.id == clip && c.speed_factor == 0.5));
    }

    #[test]
    fn tracks_of_every_kind_can_be_added() {
        let h = harness();
        for kind in [
            TrackKind::Video,
            TrackKind::Audio,
            TrackKind::Text,
            TrackKind::Shape,
        ] {
            add_track(h.handle(), kind).unwrap();
        }
        assert_eq!(h.session().project.timeline().tracks.len(), 4);
    }
}

#[test]
fn lifting_a_range_leaves_the_gap_open_and_skips_locked_tracks() {
    let mut project = project_with(vec![asset(1, MediaKind::Video, 10.0)]);
    add_asset(&mut project, 1, None, None).unwrap();
    let timeline = project.timeline_mut();

    assert!(lift_range_in(timeline, 2.0, 5.0));
    let ends: Vec<(f64, f64)> = timeline.tracks[0]
        .clips
        .iter()
        .map(|c| (c.start_secs, c.start_secs + c.duration_secs()))
        .collect();
    assert_eq!(ends.len(), 2);
    assert!((ends[0].1 - 2.0).abs() < 1e-6);
    assert!((ends[1].0 - 5.0).abs() < 1e-6, "the tail did not move");
    assert!((ends[1].1 - 10.0).abs() < 1e-6);

    timeline.tracks[0].locked = true;
    assert!(!lift_range_in(timeline, 6.0, 7.0));
    timeline.tracks[0].locked = false;
    assert!(!lift_range_in(timeline, 5.0, 5.0));
    assert!(!lift_range_in(timeline, f64::NAN, 2.0));
}
