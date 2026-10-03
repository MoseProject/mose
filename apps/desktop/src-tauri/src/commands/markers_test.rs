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

mod commands {
    use std::sync::atomic::Ordering;

    use avcore::MarkerKind;

    use crate::commands::markers::*;
    use crate::testing::harness;

    #[test]
    fn a_marker_lands_on_the_live_playhead() {
        let h = harness();
        h.set_playhead(3.5);
        add_marker(h.handle(), MarkerKind::Chapter).unwrap();
        let session = h.session();
        let marker = &session.project.timeline().markers[0];
        assert_eq!(marker.position_secs, 3.5);
        assert_eq!(marker.kind, MarkerKind::Chapter);
    }

    #[test]
    fn a_marker_is_edited_and_removed() {
        let h = harness();
        add_marker(h.handle(), MarkerKind::Standard).unwrap();
        let id = h.session().project.timeline().markers[0].id;
        update_marker(
            h.handle(),
            id,
            Some("Intro".into()),
            Some(MarkerKind::ToDo),
            Some(true),
        )
        .unwrap();
        {
            let session = h.session();
            let marker = &session.project.timeline().markers[0];
            assert_eq!(marker.label, "Intro");
            assert_eq!(marker.kind, MarkerKind::ToDo);
            assert!(marker.completed);
        }
        remove_marker(h.handle(), id).unwrap();
        assert!(h.session().project.timeline().markers.is_empty());
    }

    #[test]
    fn editing_or_removing_a_missing_marker_publishes_nothing() {
        let h = harness();
        let events = h.count_session_events();
        update_marker(h.handle(), 99, Some("x".into()), None, None).unwrap();
        remove_marker(h.handle(), 99).unwrap();
        assert_eq!(events.load(Ordering::SeqCst), 0);
    }
}
