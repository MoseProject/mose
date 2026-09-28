use avcore::MarkerKind;

use super::update;
use crate::state::blank_project;

#[test]
fn marker_patch_updates_only_requested_fields() {
    let mut project = blank_project("project".into(), "sequence".into());
    let id = project.timeline_mut().add_marker(2.5, MarkerKind::Standard);

    assert!(update(
        &mut project,
        id,
        Some("Review".into()),
        Some(MarkerKind::ToDo),
        Some(true)
    ));
    let marker = project.timeline().markers.first().unwrap();
    assert_eq!(marker.label, "Review");
    assert_eq!(marker.kind, MarkerKind::ToDo);
    assert!(marker.completed);
    assert!(!update(&mut project, id, None, None, None));
    assert!(!update(
        &mut project,
        99,
        Some("missing".into()),
        None,
        None
    ));
}
