use crate::state::blank_project;

#[test]
fn tab_operations_preserve_or_change_active_identity_as_expected() {
    let mut project = blank_project("project".into(), "one".into());
    let first = project.active_sequence().id;
    project.new_sequence("two".into());
    let second = project.active_sequence().id;

    assert!(project.move_sequence(1, 0));
    assert_eq!(
        project.active_sequence().id,
        second,
        "reorder preserves identity"
    );
    assert!(project.remove_sequence(0));
    assert_eq!(project.active_sequence().id, first);
    assert!(
        !project.remove_sequence(0),
        "the last sequence is protected"
    );
}

mod commands {
    use crate::commands::sequences::*;
    use crate::testing::{harness, Harness};

    fn two_sequences() -> Harness {
        let h = harness();
        add_sequence(h.handle(), "two".into()).unwrap();
        h
    }

    #[test]
    fn adding_a_sequence_selects_it_and_clears_the_undo_history() {
        let h = two_sequences();
        let session = h.session();
        assert_eq!(session.project.sequences.len(), 2);
        assert_eq!(session.project.active_sequence().name, "two");
        assert!(session.dirty);
        assert!(!session.view().can_undo);
    }

    #[test]
    fn selecting_switches_the_active_sequence_and_rejects_a_missing_one() {
        let h = two_sequences();
        select_sequence(h.handle(), 0).unwrap();
        assert_eq!(h.session().project.active_sequence, 0);
        select_sequence(h.handle(), 0).unwrap();
        assert_eq!(h.session().project.active_sequence, 0);
        assert_eq!(
            select_sequence(h.handle(), 5),
            Err("Sequence does not exist.".to_string())
        );
    }

    #[test]
    fn duplicating_copies_a_sequence_and_rejects_a_missing_one() {
        let h = two_sequences();
        duplicate_sequence(h.handle(), 0, "copy".into()).unwrap();
        assert_eq!(h.session().project.sequences.len(), 3);
        assert_eq!(
            duplicate_sequence(h.handle(), 9, "copy".into()),
            Err("Sequence does not exist.".to_string())
        );
    }

    #[test]
    fn renaming_marks_dirty_only_for_a_real_change() {
        let h = two_sequences();
        let id = h.session().project.sequences[0].id;
        h.session().dirty = false;
        rename_sequence(h.handle(), id, "Sequence 1".into()).unwrap();
        assert!(!h.session().dirty);
        rename_sequence(h.handle(), id, "Renamed".into()).unwrap();
        assert_eq!(h.session().project.sequences[0].name, "Renamed");
        assert!(h.session().dirty);
        assert_eq!(
            rename_sequence(h.handle(), 999, "x".into()),
            Err("Sequence does not exist.".to_string())
        );
    }

    #[test]
    fn removing_drops_a_sequence_and_resets_the_context_when_the_active_one_goes() {
        let h = two_sequences();
        let first = h.session().project.sequences[0].id;
        let second = h.session().project.sequences[1].id;
        remove_sequence(h.handle(), first).unwrap();
        assert_eq!(h.session().project.sequences.len(), 1);
        assert_eq!(h.session().project.active_sequence().id, second);

        let h = two_sequences();
        let second = h.session().project.sequences[1].id;
        remove_sequence(h.handle(), second).unwrap();
        assert_eq!(h.session().project.active_sequence, 0);
        assert!(h.session().dirty);
        assert!(!h.session().view().can_undo);
    }

    #[test]
    fn removing_rejects_missing_last_and_referenced_sequences() {
        let h = harness();
        let only = h.session().project.sequences[0].id;
        assert_eq!(
            remove_sequence(h.handle(), 999),
            Err("Sequence does not exist.".to_string())
        );
        assert_eq!(
            remove_sequence(h.handle(), only),
            Err("The last sequence cannot be removed.".to_string())
        );
    }

    #[test]
    fn a_sequence_used_as_a_compound_clip_cannot_be_removed() {
        use crate::commands::timeline::{add_asset_to_timeline, create_compound_clip};
        use avcore::{MediaAsset, MediaKind};

        let h = harness();
        h.session().project.media_library.push(MediaAsset {
            id: 1,
            file_name: "a".into(),
            source_path: "a".into(),
            kind: MediaKind::Video,
            has_audio: true,
            duration_secs: 5.0,
            codec: "h264".into(),
            source_bitrate_mbps: 8.0,
            resolution: None,
            fps: None,
            sample_rate_khz: None,
            loudness: None,
            proxy_path: None,
            waveform_peaks: None,
            favorited: false,
            streams: Default::default(),
            hdr_handling: Default::default(),
            audio_stream: None,
        });
        add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
        let clip = h.session().project.timeline().tracks[0].clips[0].id;
        let nested = create_compound_clip(h.handle(), clip, vec![clip])
            .unwrap()
            .unwrap();
        assert_eq!(
            remove_sequence(h.handle(), nested),
            Err("A sequence used as a compound clip cannot be removed.".to_string())
        );
    }

    #[test]
    fn moving_reorders_tabs_and_rejects_a_bad_index() {
        let h = two_sequences();
        move_sequence(h.handle(), 1, 0).unwrap();
        assert_eq!(h.session().project.sequences[0].name, "two");
        assert_eq!(
            move_sequence(h.handle(), 7, 0),
            Err("Sequence cannot be moved.".to_string())
        );
    }
}
