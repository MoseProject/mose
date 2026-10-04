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

mod commands {
    use std::sync::atomic::Ordering;

    use avcore::timeline::{
        ShapeKind, ShapePreset, TextAlign, TextFontFamily, TextFontStyle, TrackKind,
    };
    use avcore::{Keyframe, TextDirection};

    use crate::commands::overlays::*;
    use crate::commands::timeline::{add_track, set_track_flags, TrimEdge};
    use crate::testing::{harness, Harness};

    fn text(h: &Harness) -> u64 {
        add_text_clip(h.handle(), "Hello".into(), None, None)
            .unwrap()
            .unwrap()
    }

    fn shape(h: &Harness) -> u64 {
        add_shape_clip(h.handle()).unwrap().unwrap()
    }

    fn key() -> Vec<Keyframe<f32>> {
        vec![Keyframe {
            time_fraction: 0.5,
            value: 0.5,
        }]
    }

    #[test]
    fn the_catalog_command_lists_fonts_and_presets() {
        let catalog = overlay_catalog();
        assert!(!catalog.fonts.is_empty());
        assert!(!catalog.shape_presets.is_empty());
    }

    #[test]
    fn a_text_clip_starts_at_the_playhead_on_a_new_text_track() {
        let h = harness();
        h.session().project.timeline_mut().playhead_secs = 0.0;
        let id = text(&h);
        let session = h.session();
        let track = &session.project.timeline().tracks[0];
        assert_eq!(track.kind, TrackKind::Text);
        assert_eq!(track.text_clips[0].id, id);
        assert_eq!(track.text_clips[0].text, "Hello");
    }

    #[test]
    fn a_text_clip_takes_a_clamped_click_position_and_ignores_non_finite_ones() {
        let h = harness();
        let id = add_text_clip(h.handle(), "A".into(), Some(2.0), Some(-1.0))
            .unwrap()
            .unwrap();
        let other = add_text_clip(h.handle(), "B".into(), Some(f32::NAN), Some(f32::INFINITY))
            .unwrap()
            .unwrap();
        let session = h.session();
        let clips = &session.project.timeline().tracks[0].text_clips;
        let a = clips.iter().find(|c| c.id == id).unwrap();
        assert_eq!((a.pos_x, a.pos_y), (1.0, 0.0));
        let b = clips.iter().find(|c| c.id == other).unwrap();
        assert!(b.pos_x.is_finite() && b.pos_y.is_finite());
    }

    #[test]
    fn creating_on_a_locked_track_returns_nothing_and_leaves_no_undo_step() {
        let h = harness();
        text(&h);
        shape(&h);
        let ids: Vec<u64> = h
            .session()
            .project
            .timeline()
            .tracks
            .iter()
            .map(|t| t.id)
            .collect();
        for id in ids {
            set_track_flags(h.handle(), id, None, Some(true)).unwrap();
        }
        let events = h.count_session_events();
        assert_eq!(
            add_text_clip(h.handle(), "x".into(), None, None).unwrap(),
            None
        );
        assert_eq!(add_shape_clip(h.handle()).unwrap(), None);
        assert_eq!(
            add_custom_shape_clip(h.handle(), vec![(0.0, 0.0), (1.0, 0.0), (0.5, 1.0)]).unwrap(),
            None
        );
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_custom_shape_is_centered_on_its_points() {
        let h = harness();
        let id = add_custom_shape_clip(h.handle(), vec![(0.2, 0.2), (0.6, 0.2), (0.4, 0.8)])
            .unwrap()
            .unwrap();
        let session = h.session();
        let clip = session.project.timeline().tracks[0]
            .shape_clips
            .iter()
            .find(|c| c.id == id)
            .unwrap();
        assert!((clip.center_x - 0.4).abs() < 1e-6);
        assert!((clip.center_y - 0.5).abs() < 1e-6);
        assert!(matches!(&clip.shape_kind, ShapeKind::Polygon(v) if v.len() == 3));
    }

    #[test]
    fn a_custom_shape_needs_three_points_and_survives_collinear_ones() {
        let h = harness();
        assert_eq!(
            add_custom_shape_clip(h.handle(), vec![(0.0, 0.0), (1.0, 1.0)]).unwrap(),
            None
        );
        let id = add_custom_shape_clip(h.handle(), vec![(0.5, 0.1), (0.5, 0.5), (0.5, 0.9)])
            .unwrap()
            .unwrap();
        let session = h.session();
        let clip = session.project.timeline().tracks[0]
            .shape_clips
            .iter()
            .find(|c| c.id == id)
            .unwrap();
        assert_eq!(clip.width, 0.02);
    }

    #[test]
    fn overlays_move_between_tracks_of_their_kind_and_within_one() {
        let h = harness();
        let id = text(&h);
        add_track(h.handle(), TrackKind::Text).unwrap();
        let (first, second) = {
            let s = h.session();
            let t = &s.project.timeline().tracks;
            (t[0].id, t[1].id)
        };
        move_overlay_clip(h.handle(), id, first, 2.0).unwrap();
        assert_eq!(
            h.session().project.timeline().tracks[0].text_clips[0].start_secs,
            2.0
        );
        move_overlay_clip(h.handle(), id, second, 4.0).unwrap();
        let session = h.session();
        let tracks = &session.project.timeline().tracks;
        assert!(tracks[0].text_clips.is_empty());
        assert_eq!(tracks[1].text_clips[0].start_secs, 4.0);
    }

    #[test]
    fn a_shape_moves_to_another_shape_track_but_not_onto_a_text_track() {
        let h = harness();
        let id = shape(&h);
        add_track(h.handle(), TrackKind::Shape).unwrap();
        add_track(h.handle(), TrackKind::Text).unwrap();
        let (shape_a, shape_b, text_track) = {
            let s = h.session();
            let t = &s.project.timeline().tracks;
            (t[0].id, t[1].id, t[2].id)
        };
        move_overlay_clip(h.handle(), id, text_track, 1.0).unwrap();
        assert_eq!(
            h.session().project.timeline().tracks[0].shape_clips.len(),
            1
        );
        move_overlay_clip(h.handle(), id, shape_b, 1.0).unwrap();
        assert_eq!(
            h.session().project.timeline().tracks[1].shape_clips.len(),
            1
        );
        move_overlay_clip(h.handle(), 999, shape_a, 1.0).unwrap();
        move_overlay_clip(h.handle(), id, 999, 1.0).unwrap();
    }

    #[test]
    fn trimming_an_overlay_keeps_a_minimum_length() {
        let h = harness();
        let id = text(&h);
        trim_overlay_clip(h.handle(), id, TrimEdge::End, 2.0).unwrap();
        assert_eq!(
            h.session().project.timeline().tracks[0].text_clips[0].duration_secs,
            2.0
        );
        trim_overlay_clip(h.handle(), id, TrimEdge::Start, 1.0).unwrap();
        let session = h.session();
        let clip = &session.project.timeline().tracks[0].text_clips[0];
        assert_eq!(clip.start_secs, 1.0);
        assert_eq!(clip.duration_secs, 1.0);
    }

    #[test]
    fn a_text_patch_applies_every_field_within_its_limits() {
        let h = harness();
        let id = text(&h);
        update_text_clip(
            h.handle(),
            id,
            TextClipPatch {
                text: Some("World".into()),
                font_family: Some(TextFontFamily::BebasNeue),
                font_style: Some(TextFontStyle::Regular),
                font_size: Some(500.0),
                font_weight: Some(5000),
                color_rgba: Some([1, 2, 3, 4]),
                background_rgba: Some([5, 6, 7, 8]),
                background_padding: Some(500.0),
                background_corner_radius: Some(500.0),
                direction: Some(TextDirection::Rtl),
                text_align: Some(TextAlign::Center),
                pos_x: Some(5.0),
                pos_y: Some(-5.0),
                pos_x_keyframes: Some(key()),
                pos_y_keyframes: Some(key()),
                scale_keyframes: Some(key()),
                rotation_keyframes: Some(key()),
                opacity_keyframes: Some(key()),
                highlight_enabled: Some(true),
                highlight_color_rgba: Some([9, 9, 9, 9]),
                start_secs: Some(-3.0),
                duration_secs: Some(0.0),
            },
        )
        .unwrap();
        let session = h.session();
        let clip = &session.project.timeline().tracks[0].text_clips[0];
        assert_eq!(clip.text, "World");
        assert_eq!(clip.font_size, 120.0);
        assert_eq!(clip.font_weight, Some(900));
        assert_eq!(clip.color_rgba, [1, 2, 3, 4]);
        assert_eq!(clip.background_rgba, [5, 6, 7, 8]);
        assert_eq!(clip.background_padding, 64.0);
        assert_eq!(clip.background_corner_radius, 64.0);
        assert_eq!(clip.direction, TextDirection::Rtl);
        assert_eq!(clip.text_align, TextAlign::Center);
        assert_eq!((clip.pos_x, clip.pos_y), (1.0, 0.0));
        assert_eq!(clip.pos_x_keyframes.len(), 1);
        assert_eq!(clip.pos_y_keyframes.len(), 1);
        assert_eq!(clip.scale_keyframes.len(), 1);
        assert_eq!(clip.rotation_keyframes.len(), 1);
        assert_eq!(clip.opacity_keyframes.len(), 1);
        assert!(clip.highlight_enabled);
        assert_eq!(clip.highlight_color_rgba, [9, 9, 9, 9]);
        assert_eq!(clip.start_secs, 0.0);
        assert_eq!(clip.duration_secs, 0.1);
    }

    #[test]
    fn a_text_patch_ignores_non_finite_numbers_and_unsupported_bold() {
        let h = harness();
        let id = text(&h);
        let before = h.session().project.timeline().tracks[0].text_clips[0].clone();
        let events = h.count_session_events();
        update_text_clip(
            h.handle(),
            id,
            TextClipPatch {
                font_size: Some(f32::NAN),
                background_padding: Some(f32::NAN),
                background_corner_radius: Some(f32::NAN),
                pos_x: Some(f32::NAN),
                pos_y: Some(f32::NAN),
                start_secs: Some(f64::NAN),
                duration_secs: Some(f64::NAN),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
        assert_eq!(
            h.session().project.timeline().tracks[0].text_clips[0],
            before
        );

        update_text_clip(
            h.handle(),
            id,
            TextClipPatch {
                font_family: Some(TextFontFamily::AnonymousPro),
                font_style: Some(TextFontStyle::Bold),
                ..Default::default()
            },
        )
        .unwrap();
        let session = h.session();
        let clip = &session.project.timeline().tracks[0].text_clips[0];
        assert_eq!(
            clip.font_style == TextFontStyle::Bold,
            clip.font_family.supports_bold()
        );
    }

    #[test]
    fn patching_a_missing_or_locked_overlay_publishes_nothing() {
        let h = harness();
        let id = text(&h);
        let shape_id = shape(&h);
        let tracks: Vec<u64> = h
            .session()
            .project
            .timeline()
            .tracks
            .iter()
            .map(|t| t.id)
            .collect();
        for track in tracks {
            set_track_flags(h.handle(), track, None, Some(true)).unwrap();
        }
        let events = h.count_session_events();
        let text_patch = || TextClipPatch {
            text: Some("x".into()),
            ..Default::default()
        };
        let shape_patch = || ShapeClipPatch {
            width: Some(0.5),
            ..Default::default()
        };
        update_text_clip(h.handle(), id, text_patch()).unwrap();
        update_text_clip(h.handle(), 999, text_patch()).unwrap();
        update_text_clip(h.handle(), shape_id, text_patch()).unwrap();
        update_shape_clip(h.handle(), shape_id, shape_patch()).unwrap();
        update_shape_clip(h.handle(), 999, shape_patch()).unwrap();
        update_shape_clip(h.handle(), id, shape_patch()).unwrap();
        trim_overlay_clip(h.handle(), id, TrimEdge::End, 3.0).unwrap();
        move_overlay_clip(h.handle(), id, 1, 3.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_shape_patch_applies_every_field_within_its_limits() {
        let h = harness();
        let id = shape(&h);
        update_shape_clip(
            h.handle(),
            id,
            ShapeClipPatch {
                shape_kind: Some(ShapePreset::Triangle),
                polygon_vertices: None,
                color_rgb: Some([10, 20, 30]),
                opacity: Some(77),
                center_x: Some(5.0),
                center_y: Some(-5.0),
                center_x_keyframes: Some(key()),
                center_y_keyframes: Some(key()),
                width: Some(5.0),
                height: Some(0.0),
                width_keyframes: Some(key()),
                height_keyframes: Some(key()),
                rotation_deg: Some(720.0),
                rotation_keyframes: Some(key()),
                stroke_thickness_px: Some(-4.0),
                start_secs: Some(-2.0),
                duration_secs: Some(0.0),
            },
        )
        .unwrap();
        let session = h.session();
        let clip = &session.project.timeline().tracks[0].shape_clips[0];
        assert_eq!(clip.shape_kind, ShapePreset::Triangle.to_kind().unwrap());
        assert_eq!(clip.color_rgba, [10, 20, 30, 77]);
        assert_eq!((clip.center_x, clip.center_y), (1.0, 0.0));
        assert_eq!((clip.width, clip.height), (1.0, 0.01));
        assert_eq!(clip.rotation_deg, 360.0);
        assert_eq!(clip.stroke_thickness_px, 0.0);
        assert_eq!(clip.start_secs, 0.0);
        assert_eq!(clip.duration_secs, 0.1);
        assert_eq!(clip.center_x_keyframes.len(), 1);
        assert_eq!(clip.center_y_keyframes.len(), 1);
        assert_eq!(clip.width_keyframes.len(), 1);
        assert_eq!(clip.height_keyframes.len(), 1);
        assert_eq!(clip.rotation_keyframes.len(), 1);
    }

    #[test]
    fn hand_edited_vertices_win_over_a_preset_and_non_finite_numbers_are_ignored() {
        let h = harness();
        let id = shape(&h);
        update_shape_clip(
            h.handle(),
            id,
            ShapeClipPatch {
                shape_kind: Some(ShapePreset::Triangle),
                polygon_vertices: Some(vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)]),
                ..Default::default()
            },
        )
        .unwrap();
        let before = {
            let session = h.session();
            let clip = &session.project.timeline().tracks[0].shape_clips[0];
            assert_eq!(
                clip.shape_kind,
                ShapeKind::Polygon(vec![(0.0, 0.0), (1.0, 0.0), (0.0, 1.0)])
            );
            clip.clone()
        };
        let events = h.count_session_events();
        update_shape_clip(
            h.handle(),
            id,
            ShapeClipPatch {
                center_x: Some(f32::NAN),
                center_y: Some(f32::NAN),
                width: Some(f32::NAN),
                height: Some(f32::NAN),
                rotation_deg: Some(f32::NAN),
                stroke_thickness_px: Some(f32::NAN),
                start_secs: Some(f64::NAN),
                duration_secs: Some(f64::NAN),
                shape_kind: Some(ShapePreset::Custom),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
        assert_eq!(
            h.session().project.timeline().tracks[0].shape_clips[0],
            before
        );
    }
}

mod edge_cases {
    use std::sync::atomic::Ordering;

    use crate::commands::overlays::*;
    use crate::commands::timeline::{trim_clip, TrimEdge};
    use crate::testing::harness;

    #[test]
    fn trimming_a_missing_overlay_or_patching_the_wrong_kind_changes_nothing() {
        let h = harness();
        let text = add_text_clip(h.handle(), "t".into(), None, None)
            .unwrap()
            .unwrap();
        let shape = add_shape_clip(h.handle()).unwrap().unwrap();
        let events = h.count_session_events();
        trim_overlay_clip(h.handle(), 999, TrimEdge::End, 3.0).unwrap();
        update_text_clip(
            h.handle(),
            shape,
            TextClipPatch {
                text: Some("x".into()),
                ..Default::default()
            },
        )
        .unwrap();
        update_shape_clip(
            h.handle(),
            text,
            ShapeClipPatch {
                width: Some(0.5),
                ..Default::default()
            },
        )
        .unwrap();
        trim_clip(h.handle(), text, TrimEdge::End, 1.0).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }
}
