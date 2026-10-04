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
