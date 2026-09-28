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
