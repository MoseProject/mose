use avcore::project::{Sequence, SequenceExportSettings};
use avcore::timeline::Timeline;
use avcore::undo::UndoStack;

fn sequence_named(name: &str) -> Sequence {
    Sequence {
        id: 1,
        name: name.to_string(),
        timeline: Timeline {
            tracks: Vec::new(),
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        },
        export_settings: SequenceExportSettings::default(),
    }
}

#[test]
fn a_fresh_stack_has_nothing_to_undo_or_redo() {
    let stack = UndoStack::new();
    assert!(!stack.can_undo());
    assert!(!stack.can_redo());
}

#[test]
fn undo_restores_the_pushed_snapshot() {
    let mut stack = UndoStack::new();
    let before = sequence_named("before");
    stack.push(before.clone());

    let current = sequence_named("after");
    let restored = stack
        .undo(current)
        .expect("should have a snapshot to undo to");
    assert_eq!(restored, before);
}

#[test]
fn redo_restores_what_undo_moved_away_from() {
    let mut stack = UndoStack::new();
    let before = sequence_named("before");
    let after = sequence_named("after");
    stack.push(before.clone());

    let restored = stack.undo(after.clone()).unwrap();
    assert_eq!(restored, before);

    let redone = stack
        .redo(restored)
        .expect("should have a snapshot to redo to");
    assert_eq!(redone, after);
}

#[test]
fn a_new_push_after_undo_clears_the_redo_branch() {
    let mut stack = UndoStack::new();
    stack.push(sequence_named("a"));
    let _ = stack.undo(sequence_named("b"));
    assert!(stack.can_redo());

    stack.push(sequence_named("c"));
    assert!(
        !stack.can_redo(),
        "a fresh edit after undo should invalidate the redo branch, same as any real editor"
    );
}

#[test]
fn undo_beyond_history_returns_none_without_touching_redo() {
    let mut stack = UndoStack::new();
    assert!(stack.undo(sequence_named("only")).is_none());
    assert!(!stack.can_redo());
}

#[test]
fn capacity_bounds_the_undo_stack_by_dropping_the_oldest_entry() {
    let mut stack = UndoStack::with_capacity(2);
    stack.push(sequence_named("1"));
    stack.push(sequence_named("2"));
    stack.push(sequence_named("3"));

    // Oldest ("1") should have been dropped - only "3" then "2" are recoverable.
    let restored_1 = stack.undo(sequence_named("current")).unwrap();
    assert_eq!(restored_1.name, "3");
    let restored_2 = stack.undo(restored_1).unwrap();
    assert_eq!(restored_2.name, "2");
    assert!(!stack.can_undo());
}

#[test]
fn clear_drops_both_stacks() {
    let mut stack = UndoStack::new();
    stack.push(sequence_named("a"));
    let _ = stack.undo(sequence_named("b"));
    assert!(stack.can_redo());

    stack.clear();
    assert!(!stack.can_undo());
    assert!(!stack.can_redo());
}

fn sequence_with_clips(name: &str, clips: usize) -> Sequence {
    use avcore::timeline::{AudioRole, ClipInstance, Track, TrackKind};
    let mut sequence = sequence_named(name);
    sequence.timeline.tracks.push(Track {
        id: 1,
        name: "V1".into(),
        kind: TrackKind::Video,
        clips: (0..clips)
            .map(|i| ClipInstance::new(i as u64, 1, i as f64, 0.0, 1.0))
            .collect(),
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    });
    sequence
}

#[test]
fn a_huge_timeline_keeps_fewer_undo_steps_but_always_the_newest() {
    let mut stack = UndoStack::new();
    for step in 0..100 {
        stack.push(sequence_with_clips(&format!("step {step}"), 30_000));
    }
    // 120k clips of budget over 30k-clip snapshots: only the last few survive.
    let mut kept = 0;
    let mut current = sequence_named("now");
    let mut newest = None;
    while let Some(previous) = stack.undo(current.clone()) {
        newest.get_or_insert_with(|| previous.name.clone());
        current = previous;
        kept += 1;
    }
    assert!((1..=4).contains(&kept), "kept {kept}");
    assert_eq!(newest.as_deref(), Some("step 99"));

    // One snapshot bigger than the whole budget is still kept.
    let mut single = UndoStack::new();
    single.push(sequence_with_clips("giant", 500_000));
    assert!(single.can_undo());
}

#[test]
fn small_timelines_keep_every_step_up_to_the_capacity() {
    let mut stack = UndoStack::new();
    for step in 0..150 {
        stack.push(sequence_with_clips(&format!("step {step}"), 50));
    }
    let mut kept = 0;
    let mut current = sequence_named("now");
    while let Some(previous) = stack.undo(current.clone()) {
        current = previous;
        kept += 1;
    }
    assert_eq!(kept, 100);
}
