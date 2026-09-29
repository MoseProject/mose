use avcore::timeline::{
    ShapeKind, ShapePreset, TextFontFamily, TextFontStyle, Timeline, TrackKind,
};

use super::{
    catalog, insert_shape, insert_text, move_overlay, patch_shape, patch_text, remove_overlay,
    trim_overlay, ShapeClipPatch, TextClipPatch,
};
use crate::commands::timeline::TrimEdge;
use crate::state::blank_project;

fn timeline() -> Timeline {
    blank_project("p".into(), "s".into()).timeline().clone()
}

#[test]
fn inserting_overlays_auto_creates_their_track_at_the_playhead() {
    let mut timeline = timeline();
    timeline.playhead_secs = 2.0;
    let text = insert_text(&mut timeline, "Texto".into(), None, None).unwrap();
    let shape = insert_shape(&mut timeline).unwrap();
    let again = insert_text(&mut timeline, "Texto".into(), None, None).unwrap();

    let kinds: Vec<_> = timeline.tracks.iter().map(|t| t.kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == TrackKind::Text).count(), 1);
    assert_eq!(kinds.iter().filter(|k| **k == TrackKind::Shape).count(), 1);
    assert!(text != shape && shape != again);
    let text_track = timeline
        .tracks
        .iter()
        .find(|t| t.kind == TrackKind::Text)
        .unwrap();
    assert_eq!(text_track.text_clips.len(), 2);
    assert_eq!(text_track.text_clips[0].start_secs, 2.0);
    assert_eq!(text_track.text_clips[0].text, "Texto");
}

#[test]
fn inserting_onto_a_locked_track_is_rejected() {
    let mut timeline = timeline();
    insert_shape(&mut timeline).unwrap();
    timeline.tracks.iter_mut().for_each(|t| t.locked = true);
    assert_eq!(insert_shape(&mut timeline), None);
}

#[test]
fn overlays_move_within_and_across_tracks_of_the_same_kind() {
    let mut timeline = timeline();
    let id = insert_text(&mut timeline, "a".into(), None, None).unwrap();
    let first = timeline.tracks[0].id;
    let second = timeline.create_track(TrackKind::Text);
    let second = timeline.tracks[second].id;
    let shape_track = timeline.create_track(TrackKind::Shape);
    let shape_track = timeline.tracks[shape_track].id;

    assert!(move_overlay(&mut timeline, id, first, 4.0));
    assert!(!move_overlay(&mut timeline, id, first, 4.0));
    assert!(!move_overlay(&mut timeline, id, shape_track, 1.0));
    assert!(move_overlay(&mut timeline, id, second, -1.0));
    let moved = &timeline.tracks[1].text_clips;
    assert_eq!((moved.len(), moved[0].start_secs), (1, 0.0));
    assert!(timeline.tracks[0].text_clips.is_empty());
}

#[test]
fn trimming_keeps_a_minimum_duration() {
    let mut timeline = timeline();
    let id = insert_shape(&mut timeline).unwrap();
    assert!(trim_overlay(&mut timeline, id, TrimEdge::End, 5.0));
    assert!(trim_overlay(&mut timeline, id, TrimEdge::Start, 1.0));
    let clip = &timeline.tracks[0].shape_clips[0];
    assert_eq!((clip.start_secs, clip.duration_secs), (1.0, 4.0));

    assert!(trim_overlay(&mut timeline, id, TrimEdge::End, 0.0));
    let clip = &timeline.tracks[0].shape_clips[0];
    assert!((clip.duration_secs - 0.1).abs() < 1e-9);
    assert!(!trim_overlay(&mut timeline, id, TrimEdge::End, 0.0));
}

#[test]
fn text_patches_clamp_and_respect_bold_support() {
    let mut timeline = timeline();
    let id = insert_text(&mut timeline, "a".into(), None, None).unwrap();
    assert!(patch_text(
        &mut timeline,
        id,
        TextClipPatch {
            font_size: Some(500.0),
            pos_x: Some(-3.0),
            font_family: Some(TextFontFamily::BebasNeue),
            font_style: Some(TextFontStyle::Bold),
            ..Default::default()
        },
    ));
    let clip = &timeline.tracks[0].text_clips[0];
    assert_eq!((clip.font_size, clip.pos_x), (120.0, 0.0));
    assert_eq!(clip.font_style, TextFontStyle::Regular);
    assert!(!patch_text(&mut timeline, id, TextClipPatch::default()));
}

#[test]
fn shape_patches_switch_presets_and_keep_alpha_separate_from_color() {
    let mut timeline = timeline();
    let id = insert_shape(&mut timeline).unwrap();
    assert!(patch_shape(
        &mut timeline,
        id,
        ShapeClipPatch {
            shape_kind: Some(ShapePreset::Ellipse),
            color_rgb: Some([10, 20, 30]),
            width: Some(0.0),
            ..Default::default()
        },
    ));
    assert!(patch_shape(
        &mut timeline,
        id,
        ShapeClipPatch {
            opacity: Some(128),
            shape_kind: Some(ShapePreset::Custom),
            ..Default::default()
        },
    ));
    let clip = &timeline.tracks[0].shape_clips[0];
    assert_eq!(clip.shape_kind, ShapeKind::Ellipse);
    assert_eq!(clip.color_rgba, [10, 20, 30, 128]);
    assert_eq!(clip.width, 0.01);
}

#[test]
fn removing_overlays_skips_locked_tracks() {
    let mut timeline = timeline();
    let text = insert_text(&mut timeline, "a".into(), None, None).unwrap();
    let shape = insert_shape(&mut timeline).unwrap();
    timeline.tracks[1].locked = true;
    assert!(remove_overlay(&mut timeline, text));
    assert!(!remove_overlay(&mut timeline, shape));
    assert!(!remove_overlay(&mut timeline, text));
}

#[test]
fn catalog_lists_every_bundled_font_and_pickable_preset() {
    let catalog = catalog();
    assert_eq!(catalog.fonts.len(), TextFontFamily::ALL.len());
    assert_eq!(catalog.fonts[0].id, "lato");
    assert_eq!(catalog.shape_presets.len(), ShapePreset::PICKABLE.len());
}
