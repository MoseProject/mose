use std::time::{Duration, Instant};

use super::*;
use crate::testing::{harness, Harness};

/// A project whose one clip is a compound clip over the fixture's clip, and the id of the nested
/// sequence.
fn compound_project(h: &Harness) -> (Arc<Project>, u64) {
    let (_, clip) = h.add_fixture_clip();
    crate::commands::timeline::create_compound_clip(h.handle(), clip, vec![clip]).unwrap();
    let project = h.session().project.clone();
    let nested_id = project
        .timeline()
        .tracks
        .iter()
        .flat_map(|track| &track.clips)
        .find_map(|clip| clip.nested_sequence_id)
        .expect("a compound clip");
    (Arc::new(project), nested_id)
}

fn wait_for_render(renderer: &mut NestedRenderer) {
    let started = Instant::now();
    while !renderer.poll() {
        assert!(
            started.elapsed() < Duration::from_secs(120),
            "the render never finished"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_compound_clip_that_was_never_rendered_does_not_hold_the_caller_up() {
    let h = harness();
    let (project, nested_id) = compound_project(&h);
    let mut renderer = NestedRenderer::default();
    let timeline = project.timeline().clone();

    // Nothing to show yet, and the render is going on in the background.
    let started = Instant::now();
    let first = renderer.assets_for(&project, &timeline);
    assert!(first.is_empty());
    assert!(renderer.is_running());
    assert!(started.elapsed() < Duration::from_millis(500));

    wait_for_render(&mut renderer);
    assert!(!renderer.is_running());
    let rendered = renderer.assets_for(&project, &timeline);
    assert_eq!(rendered.len(), 1);
    assert!(rendered.contains_key(&nested_id));
    // Current now: asking again starts nothing.
    assert!(!renderer.is_running());
}

#[test]
fn an_edit_inside_shows_the_previous_render_until_the_new_one_is_ready() {
    let h = harness();
    let (project, nested_id) = compound_project(&h);
    let mut renderer = NestedRenderer::default();
    let timeline = project.timeline().clone();
    renderer.assets_for(&project, &timeline);
    wait_for_render(&mut renderer);
    let before = renderer.assets_for(&project, &timeline)[&nested_id].clone();

    // Shorten the clip inside the compound clip.
    let mut edited = (*project).clone();
    let nested = edited
        .sequences
        .iter_mut()
        .find(|sequence| sequence.id == nested_id)
        .unwrap();
    nested.timeline.tracks[0].clips[0].source_out_secs -= 0.2;
    let edited = Arc::new(edited);

    let stale = renderer.assets_for(&edited, &timeline);
    assert!(renderer.is_running());
    assert_eq!(stale[&nested_id].duration_secs, before.duration_secs);

    wait_for_render(&mut renderer);
    let fresh = renderer.assets_for(&edited, &timeline);
    assert!(fresh[&nested_id].duration_secs < before.duration_secs);
}

#[test]
fn a_project_without_compound_clips_costs_nothing() {
    let h = harness();
    h.add_fixture_clip();
    let project = Arc::new(h.session().project.clone());
    let mut renderer = NestedRenderer::default();
    assert!(renderer
        .assets_for(&project, &project.timeline().clone())
        .is_empty());
    assert!(!renderer.is_running());
    assert!(!renderer.poll());
}
