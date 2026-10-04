use std::path::Path;
use std::sync::atomic::Ordering;

use avcore::TrackKind;

use super::*;
use crate::commands::timeline::{add_asset_to_timeline, update_clip, ClipPatch};
use crate::testing::{harness, path_in, video_asset, Harness};

fn age(path: &Path, seconds: u64) {
    let file = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(seconds))
        .unwrap();
}

/// Saves a project named `name` at `path`.
fn save_named(path: &Path, name: &str) {
    let project = crate::state::blank_project(name.into(), "Seq".into());
    avcore::save_project_to_file(&project, path).unwrap();
}

/// A harness whose library holds one video asset with two clips on the timeline, the first
/// brightened so it carries non-default formatting.
fn with_clips() -> (Harness, Vec<u64>) {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    let ids: Vec<u64> = h.session().project.timeline().tracks[0]
        .clips
        .iter()
        .map(|c| c.id)
        .collect();
    update_clip(
        h.handle(),
        ids[0],
        ClipPatch {
            brightness: Some(0.5),
            ..Default::default()
        },
    )
    .unwrap();
    (h, ids)
}

#[test]
fn prefs_are_read_back_as_stored() {
    let h = harness();
    h.prefs().view.autosave_minutes = 7;
    assert_eq!(get_prefs(h.state()).autosave_minutes, 7);
}

#[test]
fn setting_prefs_saves_them_updates_telemetry_and_publishes() {
    let h = harness();
    let events = h.count_session_events();
    let mut view = h.prefs().view.clone();
    view.autosave_minutes = 9;
    view.telemetry_enabled = false;
    set_prefs(h.handle(), view).unwrap();
    assert_eq!(h.prefs().view.autosave_minutes, 9);
    assert!(!h.state().telemetry_enabled.load(Ordering::Relaxed));
    assert_eq!(events.load(Ordering::SeqCst), 1);
    assert!(crate::prefs::prefs_path().exists());
}

#[test]
fn recent_projects_list_only_readable_projects_with_their_autosave() {
    let h = harness();
    let good = path_in(&h.dir, "good.ocproj");
    let missing = path_in(&h.dir, "missing.ocproj");
    let corrupt = path_in(&h.dir, "corrupt.ocproj");
    save_named(&good, "Good");
    std::fs::write(&corrupt, b"not a project").unwrap();
    std::fs::write(crate::prefs::autosave_path(&good), b"recovery").unwrap();
    age(&good, 600);
    h.prefs().view.recent_project_paths = [&good, &missing, &corrupt]
        .iter()
        .map(|p| p.display().to_string())
        .collect();

    let recent = get_recent_projects(h.state());
    assert_eq!(recent.len(), 1);
    assert_eq!(recent[0].name, "Good");
    assert_eq!(recent[0].path, good.display().to_string());
    assert!(recent[0].modified_unix_secs > 0);
    assert_eq!(
        recent[0].autosave_path,
        Some(crate::prefs::autosave_path(&good).display().to_string())
    );
}

#[test]
fn a_recent_project_moves_to_the_front_without_duplicates_and_the_list_is_capped() {
    let h = harness();
    for index in 0..12 {
        record_recent_project(&h.handle(), Path::new(&format!("p{index}.ocproj"))).unwrap();
    }
    record_recent_project(&h.handle(), Path::new("p5.ocproj")).unwrap();
    let list = h.prefs().view.recent_project_paths.clone();
    assert_eq!(list.len(), 10);
    assert_eq!(list[0], "p5.ocproj");
    assert_eq!(list.iter().filter(|p| *p == "p5.ocproj").count(), 1);
}

#[test]
fn removing_a_recent_project_saves_and_publishes() {
    let h = harness();
    record_recent_project(&h.handle(), Path::new("a.ocproj")).unwrap();
    record_recent_project(&h.handle(), Path::new("b.ocproj")).unwrap();
    let events = h.count_session_events();
    remove_recent_project(h.handle(), "a.ocproj".into()).unwrap();
    assert_eq!(
        h.prefs().view.recent_project_paths,
        vec!["b.ocproj".to_string()]
    );
    assert_eq!(events.load(Ordering::SeqCst), 1);
}

#[test]
fn a_newer_autosave_is_restored_as_unsaved_work_on_the_original_path() {
    let h = harness();
    let project = path_in(&h.dir, "show.ocproj");
    save_named(&project, "Saved");
    age(&project, 600);
    save_named(&crate::prefs::autosave_path(&project), "Recovered");

    restore_autosave(h.handle(), project.display().to_string()).unwrap();
    let session = h.session();
    assert_eq!(session.project.name, "Recovered");
    assert_eq!(
        session.project.file_path.as_deref(),
        Some(project.as_path())
    );
    assert!(session.dirty);
}

#[test]
fn restoring_without_a_newer_autosave_fails() {
    let h = harness();
    let project = path_in(&h.dir, "none.ocproj");
    assert_eq!(
        restore_autosave(h.handle(), project.display().to_string()),
        Err("no newer autosave is available".to_string())
    );
    std::fs::write(crate::prefs::autosave_path(&project), b"not a project").unwrap();
    assert!(restore_autosave(h.handle(), project.display().to_string()).is_err());
}

#[test]
fn discarding_removes_the_autosave_and_tolerates_a_missing_one() {
    let h = harness();
    let project = path_in(&h.dir, "x.ocproj");
    let autosave = crate::prefs::autosave_path(&project);
    discard_autosave(project.display().to_string()).unwrap();
    std::fs::write(&autosave, b"recovery").unwrap();
    discard_autosave(project.display().to_string()).unwrap();
    assert!(!autosave.exists());
}

#[test]
fn a_layer_template_captures_the_chosen_clips_formatting() {
    let (h, ids) = with_clips();
    let templates = save_layer_template(h.handle(), "  Look  ".into(), ids.clone()).unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].name, "Look");
    assert_eq!(templates[0].layers.len(), 2);
    assert_eq!(templates[0].layers[0].0, TrackKind::Video);
    assert_eq!(templates[0].layers[0].1.brightness, 0.5);
}

#[test]
fn a_template_needs_a_name_and_at_least_one_matching_clip() {
    let (h, ids) = with_clips();
    assert!(save_layer_template(h.handle(), "   ".into(), ids.clone())
        .unwrap()
        .is_empty());
    assert!(save_layer_template(h.handle(), "Look".into(), vec![999])
        .unwrap()
        .is_empty());
}

#[test]
fn deleting_a_template_ignores_an_index_out_of_range() {
    let (h, ids) = with_clips();
    save_layer_template(h.handle(), "A".into(), ids.clone()).unwrap();
    save_layer_template(h.handle(), "B".into(), ids).unwrap();
    assert_eq!(delete_layer_template(h.handle(), 9).unwrap().len(), 2);
    let left = delete_layer_template(h.handle(), 0).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].name, "B");
}

#[test]
fn applying_a_template_stacks_a_track_per_layer_with_its_formatting() {
    let (h, ids) = with_clips();
    save_layer_template(h.handle(), "Look".into(), vec![ids[0]]).unwrap();
    h.set_playhead(2.0);
    apply_layer_template(h.handle(), 0, vec![Some(1)]).unwrap();
    let session = h.session();
    let tracks = &session.project.timeline().tracks;
    assert_eq!(tracks.len(), 2);
    let clip = &tracks[1].clips[0];
    assert_eq!(clip.asset_id, 1);
    assert_eq!(clip.start_secs, 2.0);
    assert_eq!(clip.brightness, 0.5);
}

#[test]
fn applying_skips_unassigned_and_unknown_assets_and_missing_templates() {
    let (h, ids) = with_clips();
    save_layer_template(h.handle(), "Look".into(), ids).unwrap();
    let events = h.count_session_events();
    apply_layer_template(h.handle(), 5, vec![Some(1)]).unwrap();
    apply_layer_template(h.handle(), 0, vec![None, Some(404)]).unwrap();
    assert_eq!(events.load(Ordering::SeqCst), 0);
    assert_eq!(h.session().project.timeline().tracks.len(), 1);
}

#[test]
fn applying_a_template_to_an_audio_asset_makes_an_audio_track() {
    let (h, ids) = with_clips();
    save_layer_template(h.handle(), "Look".into(), vec![ids[0]]).unwrap();
    let mut audio = video_asset(2, 4.0);
    audio.kind = avcore::MediaKind::Audio;
    h.session().project.media_library.push(audio);
    apply_layer_template(h.handle(), 0, vec![Some(2)]).unwrap();
    let session = h.session();
    let tracks = &session.project.timeline().tracks;
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[1].kind, TrackKind::Audio);
}
