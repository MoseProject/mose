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

use avcore::project::{Project, Sequence};
use avcore::timeline::{
    BlendMode, ClipInstance, ColorFilter, MaskShape, Timeline, Track, TrackKind, TransitionType,
};
use avcore::{ExportAspectRatio, MediaAsset, Recency};

/// A minimal `ClipInstance` with everything but `id`/`nested_sequence_id` at its default value
/// — only `sequences_referencing_as_compound_clip`'s own tests need a real clip at all, and only
/// `nested_sequence_id` matters to that function.
fn compound_clip(id: u64, nested_sequence_id: Option<u64>) -> ClipInstance {
    ClipInstance {
        id,
        asset_id: 0,
        start_secs: 0.0,
        source_in_secs: 0.0,
        source_out_secs: 1.0,
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor: 1.0,
        speed_ramp_end_factor: None,
        nested_sequence_id,
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

fn compound_video_track(id: u64, clips: Vec<ClipInstance>) -> Track {
    Track {
        id,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: avcore::AudioRole::Unspecified,
        locked: false,
        color_label: None,
    }
}

fn test_project() -> Project {
    Project {
        id: 1,
        name: "Test".to_string(),
        last_edited: Recency::HoursAgo(0),
        summary: String::new(),
        media_library: Vec::<MediaAsset>::new(),
        sequences: vec![Sequence {
            id: 1,
            name: "Main".to_string(),
            timeline: Timeline {
                tracks: Vec::new(),
                playhead_secs: 5.0,
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
    }
}

#[test]
fn timeline_reads_the_active_sequence() {
    let project = test_project();
    assert_eq!(project.timeline().playhead_secs, 5.0);
}

#[test]
fn timeline_mut_writes_the_active_sequence() {
    let mut project = test_project();
    project.timeline_mut().playhead_secs = 42.0;
    assert_eq!(project.sequences[0].timeline.playhead_secs, 42.0);
}

#[test]
fn new_sequence_appends_and_switches_to_it() {
    let mut project = test_project();
    let id = project.new_sequence("Second".to_string());

    assert_eq!(project.sequences.len(), 2);
    assert_eq!(id, 2);
    assert_eq!(project.sequences[1].id, 2);
    assert_eq!(project.sequences[1].name, "Second");
    assert_eq!(project.active_sequence, 1);
    assert!(project.timeline().tracks.is_empty());
}

#[test]
fn new_sequence_ids_keep_increasing_after_multiple_calls() {
    let mut project = test_project();
    project.new_sequence("A".to_string());
    let third_id = project.new_sequence("B".to_string());

    assert_eq!(third_id, 3);
}

#[test]
fn new_sequence_inherits_the_active_sequences_export_settings() {
    let mut project = test_project();
    project.sequences[0].export_settings.aspect_ratio = ExportAspectRatio::Portrait;
    project.sequences[0].export_settings.target_lufs = -23.0;

    project.new_sequence("Short variant".to_string());

    assert_eq!(
        project.sequences[1].export_settings.aspect_ratio,
        ExportAspectRatio::Portrait
    );
    assert_eq!(project.sequences[1].export_settings.target_lufs, -23.0);
}

#[test]
fn duplicate_sequence_copies_the_complete_tab_with_a_fresh_id_and_name() {
    let mut project = test_project();
    project.sequences[0].timeline.playhead_secs = 12.5;
    project.sequences[0].export_settings.aspect_ratio = ExportAspectRatio::Portrait;

    let id = project
        .duplicate_sequence(0, "Main copy".to_string())
        .unwrap();

    assert_eq!(id, 2);
    assert_eq!(project.sequences.len(), 2);
    assert_eq!(project.active_sequence, 1);
    assert_eq!(project.sequences[1].id, 2);
    assert_eq!(project.sequences[1].name, "Main copy");
    assert_eq!(project.sequences[1].timeline.playhead_secs, 12.5);
    assert_eq!(
        project.sequences[1].export_settings.aspect_ratio,
        ExportAspectRatio::Portrait
    );
}

#[test]
fn duplicate_sequence_is_a_no_op_for_an_out_of_range_index() {
    let mut project = test_project();

    assert_eq!(project.duplicate_sequence(4, "Copy".to_string()), None);
    assert_eq!(project.sequences.len(), 1);
    assert_eq!(project.active_sequence, 0);
}

#[test]
fn remove_sequence_never_removes_the_projects_last_tab() {
    let mut project = test_project();

    assert!(!project.remove_sequence(0));
    assert_eq!(project.sequences.len(), 1);
    assert_eq!(project.active_sequence, 0);
}

#[test]
fn removing_the_active_last_sequence_selects_its_previous_neighbor() {
    let mut project = test_project();
    project.new_sequence("Second".to_string());
    project.new_sequence("Third".to_string());

    assert!(project.remove_sequence(2));
    assert_eq!(project.active_sequence, 1);
    assert_eq!(project.sequences[1].name, "Second");
}

#[test]
fn removing_a_sequence_before_the_active_one_preserves_the_active_identity() {
    let mut project = test_project();
    project.new_sequence("Second".to_string());
    project.new_sequence("Third".to_string());
    let active_id = project.sequences[2].id;

    assert!(project.remove_sequence(0));
    assert_eq!(project.active_sequence, 1);
    assert_eq!(project.sequences[project.active_sequence].id, active_id);
}

#[test]
fn move_sequence_reorders_tabs_without_changing_the_active_identity() {
    let mut project = test_project();
    project.new_sequence("Second".to_string());
    project.new_sequence("Third".to_string());
    project.active_sequence = 1;
    let active_id = project.sequences[1].id;

    assert!(project.move_sequence(0, 2));

    let names: Vec<&str> = project
        .sequences
        .iter()
        .map(|sequence| sequence.name.as_str())
        .collect();
    assert_eq!(names, vec!["Second", "Third", "Main"]);
    assert_eq!(project.sequences[project.active_sequence].id, active_id);
}

#[test]
fn move_sequence_rejects_invalid_or_unchanged_positions() {
    let mut project = test_project();
    project.new_sequence("Second".to_string());

    assert!(!project.move_sequence(0, 0));
    assert!(!project.move_sequence(0, 5));
    assert!(!project.move_sequence(5, 0));
    assert_eq!(project.sequences[0].name, "Main");
    assert_eq!(project.sequences[1].name, "Second");
}

#[test]
fn sequences_referencing_as_compound_clip_finds_the_referencing_sequence() {
    let mut project = test_project();
    let target_id = project.new_sequence("Nested".to_string());
    let referencing_id = project.new_sequence("Uses the nested one".to_string());
    project.sequences[2].timeline.tracks = vec![compound_video_track(
        1,
        vec![compound_clip(1, Some(target_id))],
    )];

    let referencing = project.sequences_referencing_as_compound_clip(target_id);

    assert_eq!(referencing.len(), 1);
    assert_eq!(referencing[0].id, referencing_id);
}

#[test]
fn sequences_referencing_as_compound_clip_ignores_ordinary_clips() {
    let mut project = test_project();
    let target_id = project.new_sequence("Nested".to_string());
    project.new_sequence("Unrelated".to_string());
    project.sequences[2].timeline.tracks =
        vec![compound_video_track(1, vec![compound_clip(1, None)])];

    assert!(project
        .sequences_referencing_as_compound_clip(target_id)
        .is_empty());
}

#[test]
fn sequences_referencing_as_compound_clip_excludes_the_sequence_itself() {
    // A sequence can't nest itself (nested_sequence::materialize_nested_sequences enforces this
    // via cycle detection at render time), but this function still shouldn't ever report a
    // sequence as referencing itself even if such a dangling/malformed state existed.
    let mut project = test_project();
    let self_id = project.sequences[0].id;
    project.sequences[0].timeline.tracks = vec![compound_video_track(
        1,
        vec![compound_clip(1, Some(self_id))],
    )];

    assert!(project
        .sequences_referencing_as_compound_clip(self_id)
        .is_empty());
}

#[test]
fn sequences_referencing_as_compound_clip_finds_every_referencing_sequence() {
    let mut project = test_project();
    let target_id = project.new_sequence("Nested".to_string());
    let first_id = project.new_sequence("First user".to_string());
    let second_id = project.new_sequence("Second user".to_string());
    project.sequences[2].timeline.tracks = vec![compound_video_track(
        1,
        vec![compound_clip(1, Some(target_id))],
    )];
    project.sequences[3].timeline.tracks = vec![compound_video_track(
        1,
        vec![compound_clip(1, Some(target_id))],
    )];

    let referencing = project.sequences_referencing_as_compound_clip(target_id);
    let ids: Vec<u64> = referencing.iter().map(|s| s.id).collect();

    assert_eq!(ids, vec![first_id, second_id]);
}

#[test]
fn create_compound_clip_wraps_a_single_clip_into_a_new_nested_sequence() {
    let mut project = test_project();
    let clip = ClipInstance {
        start_secs: 2.0,
        source_in_secs: 0.5,
        source_out_secs: 3.5,
        ..compound_clip(7, None)
    };
    project.sequences[0].timeline.tracks = vec![compound_video_track(1, vec![clip])];

    let new_sequence_id = project.create_compound_clip(7, &[7]).expect("should compound");

    assert_eq!(project.sequences.len(), 2);
    let nested = project
        .sequences
        .iter()
        .find(|s| s.id == new_sequence_id)
        .expect("nested sequence should exist");
    assert_eq!(nested.timeline.tracks.len(), 1);
    let inner = &nested.timeline.tracks[0].clips[0];
    assert_eq!(inner.id, 7);
    assert_eq!(inner.start_secs, 0.0, "rebased to the nested track's own 0.0");
    assert_eq!(inner.source_in_secs, 0.5);
    assert_eq!(inner.source_out_secs, 3.5);

    let wrapper = &project.sequences[0].timeline.tracks[0].clips[0];
    assert_eq!(wrapper.id, 7, "wrapper reuses the original clip's own id");
    assert_eq!(wrapper.nested_sequence_id, Some(new_sequence_id));
    assert_eq!(wrapper.start_secs, 2.0, "wrapper keeps the original timeline position");
    assert_eq!(wrapper.duration_secs(), 3.0, "wrapper spans the nested content's own length");
}

#[test]
fn create_compound_clip_is_a_no_op_off_a_video_track() {
    let mut project = test_project();
    let audio_track = Track {
        id: 1,
        name: "A1".to_string(),
        kind: TrackKind::Audio,
        clips: vec![compound_clip(9, None)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: avcore::AudioRole::Unspecified,
        locked: false,
        color_label: None,
    };
    project.sequences[0].timeline.tracks = vec![audio_track];

    assert_eq!(project.create_compound_clip(9, &[9]), None);
    assert_eq!(project.sequences.len(), 1, "no nested sequence created");
}

#[test]
fn create_compound_clip_compounds_the_whole_multi_selection_when_unambiguous() {
    let mut project = test_project();
    let first = ClipInstance {
        start_secs: 0.0,
        source_in_secs: 0.0,
        source_out_secs: 1.0,
        ..compound_clip(1, None)
    };
    let second = ClipInstance {
        start_secs: 2.0,
        source_in_secs: 0.0,
        source_out_secs: 1.0,
        ..compound_clip(2, None)
    };
    project.sequences[0].timeline.tracks =
        vec![compound_video_track(1, vec![first, second])];

    let new_sequence_id = project
        .create_compound_clip(1, &[1, 2])
        .expect("should compound both");

    let nested = project
        .sequences
        .iter()
        .find(|s| s.id == new_sequence_id)
        .unwrap();
    let mut inner_ids: Vec<u64> = nested.timeline.tracks[0]
        .clips
        .iter()
        .map(|c| c.id)
        .collect();
    inner_ids.sort();
    assert_eq!(inner_ids, vec![1, 2], "both selected clips moved into the nested sequence");

    assert_eq!(project.sequences[0].timeline.tracks[0].clips.len(), 1);
    let wrapper = &project.sequences[0].timeline.tracks[0].clips[0];
    assert_eq!(wrapper.start_secs, 0.0, "spans from the earliest member's start");
    assert_eq!(wrapper.duration_secs(), 3.0, "spans through the latest member's own end");
}

#[test]
fn create_compound_clip_falls_back_to_single_clip_when_selection_already_has_a_compound_clip() {
    let mut project = test_project();
    let plain = ClipInstance {
        start_secs: 0.0,
        ..compound_clip(1, None)
    };
    let already_compound = ClipInstance {
        start_secs: 2.0,
        ..compound_clip(2, Some(99))
    };
    project.sequences[0].timeline.tracks =
        vec![compound_video_track(1, vec![plain, already_compound])];

    let new_sequence_id = project
        .create_compound_clip(1, &[1, 2])
        .expect("should still compound the primary clip alone");

    let nested = project
        .sequences
        .iter()
        .find(|s| s.id == new_sequence_id)
        .unwrap();
    assert_eq!(nested.timeline.tracks[0].clips.len(), 1);
    assert_eq!(nested.timeline.tracks[0].clips[0].id, 1);
    // The already-compound clip (id 2) is left untouched on the original track.
    assert!(project.sequences[0].timeline.tracks[0]
        .clips
        .iter()
        .any(|c| c.id == 2 && c.nested_sequence_id == Some(99)));
}

#[test]
fn record_recent_asset_inserts_new_ids_at_the_front() {
    let mut project = test_project();

    project.record_recent_asset(1);
    project.record_recent_asset(2);

    assert_eq!(project.recent_asset_ids, vec![2, 1]);
}

#[test]
fn record_recent_asset_moves_an_existing_id_to_the_front_without_duplicating_it() {
    let mut project = test_project();
    project.record_recent_asset(1);
    project.record_recent_asset(2);
    project.record_recent_asset(3);

    project.record_recent_asset(1);

    assert_eq!(project.recent_asset_ids, vec![1, 3, 2]);
}

#[test]
fn record_recent_asset_caps_the_list_at_recent_asset_capacity() {
    let mut project = test_project();

    for id in 0..(avcore::project::RECENT_ASSET_CAPACITY as u64 + 5) {
        project.record_recent_asset(id);
    }

    assert_eq!(
        project.recent_asset_ids.len(),
        avcore::project::RECENT_ASSET_CAPACITY
    );
    let expected_most_recent = avcore::project::RECENT_ASSET_CAPACITY as u64 + 4;
    assert_eq!(project.recent_asset_ids[0], expected_most_recent);
}
