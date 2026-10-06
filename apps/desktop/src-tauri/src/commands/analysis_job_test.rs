use super::*;
use crate::testing::harness;

#[test]
fn a_project_epoch_change_rejects_results_even_when_clip_ids_are_reused() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    h.session().undo.clear();
    let token = AnalysisToken::capture(&h.state(), Some(clip), None);
    let before = h.session().project.clone();
    h.session().project_epoch += 1;
    assert!(!token.is_current(&h.handle()));
    assert!(token
        .apply(&h.handle(), |project| project.name = "stale".into())
        .is_err());
    assert_eq!(h.session().project, before);
    assert!(!h.session().undo.can_undo());
}

#[test]
fn a_clip_edit_invalidates_the_captured_analysis() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    h.session().undo.clear();
    let token = AnalysisToken::capture(&h.state(), Some(clip), None);
    h.session()
        .project
        .timeline_mut()
        .clip_mut(clip)
        .unwrap()
        .source_in_secs += 0.1;
    assert!(!token.is_current(&h.handle()));
}

#[test]
fn admission_is_exclusive_and_drop_releases_feature_flags() {
    let h = harness();
    let permit = AnalysisPermit::acquire(&h.handle()).unwrap();
    *crate::state::lock_or_recover(&h.state().scene_cut_detection_clip_id) = Some(7);
    assert!(AnalysisPermit::acquire(&h.handle()).is_err());
    drop(permit);
    assert!(crate::state::lock_or_recover(&h.state().scene_cut_detection_clip_id).is_none());
    assert!(AnalysisPermit::acquire(&h.handle()).is_ok());
}

#[test]
fn a_current_result_is_applied_with_an_undo_step() {
    let h = harness();
    let (_, clip) = h.add_fixture_clip();
    h.session().undo.clear();
    let token = AnalysisToken::capture(&h.state(), Some(clip), None);
    token
        .apply(&h.handle(), |project| {
            project.timeline_mut().clip_mut(clip).unwrap().gain_db = 3.0;
        })
        .unwrap();
    assert_eq!(
        h.session()
            .project
            .timeline()
            .tracks
            .iter()
            .flat_map(|track| &track.clips)
            .find(|item| item.id == clip)
            .unwrap()
            .gain_db,
        3.0
    );
    assert!(h.session().undo.can_undo());
}

#[test]
fn a_source_file_revision_change_invalidates_the_analysis() {
    let h = harness();
    let (asset, clip) = h.add_fixture_clip();
    let source = h.dir.join("analysis-source.mp4");
    std::fs::copy(crate::testing::fixture("video.mp4"), &source).unwrap();
    h.session()
        .project
        .media_library
        .iter_mut()
        .find(|item| item.id == asset)
        .unwrap()
        .source_path = source.clone();
    let token = AnalysisToken::capture(&h.state(), Some(clip), None);
    assert!(token.is_current(&h.handle()));
    let mut bytes = std::fs::read(&source).unwrap();
    bytes.push(0);
    std::fs::write(&source, bytes).unwrap();
    assert!(!token.is_current(&h.handle()));
}
