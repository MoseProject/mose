use std::sync::atomic::Ordering;

use super::*;
use crate::testing::{fixture, harness, path_in, video_asset, Harness};

/// A harness whose project lives in its own folder, so generated proxies stay in the scratch dir.
fn saved_harness() -> Harness {
    let h = harness();
    h.session().project.file_path = Some(path_in(&h.dir, "project.ocproj"));
    h
}

#[test]
fn favoriting_toggles_and_ignores_unknown_assets() {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    toggle_favorite(h.handle(), 1).unwrap();
    assert!(h.session().project.media_library[0].favorited);
    assert!(h.session().dirty);
    toggle_favorite(h.handle(), 1).unwrap();
    assert!(!h.session().project.media_library[0].favorited);
    toggle_favorite(h.handle(), 404).unwrap();
}

#[test]
fn removing_media_is_refused_while_a_clip_uses_it() {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    h.session().project.media_library.push(video_asset(2, 5.0));
    crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();

    assert_eq!(remove_asset(h.handle(), 1), Err("assetInUse".to_string()));
    assert_eq!(h.session().project.media_library.len(), 2);

    remove_asset(h.handle(), 2).unwrap();
    let ids: Vec<u64> = h
        .session()
        .project
        .media_library
        .iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(ids, vec![1]);
    assert!(h.session().dirty);
    remove_asset(h.handle(), 404).unwrap();
}

#[test]
fn removing_media_drops_the_undo_history_that_could_bring_its_clips_back() {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    h.session().project.media_library.push(video_asset(2, 5.0));
    crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    assert!(h.session().undo.can_undo());

    remove_asset(h.handle(), 2).unwrap();
    assert!(!h.session().undo.can_undo());
}

#[test]
fn asking_to_remove_unknown_media_keeps_the_undo_history() {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    crate::commands::timeline::add_asset_to_timeline(h.handle(), 1, None, None).unwrap();
    remove_asset(h.handle(), 404).unwrap();
    assert!(h.session().undo.can_undo());
}

#[test]
fn media_ids_are_never_reused_after_a_removal() {
    let h = harness();
    h.session().project.media_library.push(video_asset(1, 5.0));
    let mut session = h.session();
    let second = session.allocate_asset_id();
    assert_eq!(second, 2);
    session.project.media_library.push(video_asset(second, 5.0));
    assert!(session.project.remove_asset(second));
    assert_eq!(session.allocate_asset_id(), 3, "2 is not handed out again");

    let project = session.project.clone();
    session.replace_project(project);
    assert_eq!(
        session.allocate_asset_id(),
        2,
        "a new project starts from its own library"
    );
}

#[test]
fn an_imported_video_gets_its_loudness_proxy_and_waveform() {
    let h = saved_harness();
    import_media(h.handle(), vec![fixture("video.mp4").display().to_string()]);
    h.wait_until("the video import to finish", |s| {
        s.project
            .media_library
            .first()
            .is_some_and(|a| a.proxy_path.is_some() && a.waveform_peaks.is_some())
    });
    let session = h.session();
    let asset = &session.project.media_library[0];
    assert_eq!(asset.kind, MediaKind::Video);
    assert_eq!(asset.file_name, "video.mp4");
    assert!(asset.duration_secs > 0.0);
    assert!(session.dirty);
}

#[test]
fn an_imported_audio_file_has_no_proxy() {
    let h = saved_harness();
    import_media(h.handle(), vec![fixture("audio.m4a").display().to_string()]);
    h.wait_until("the audio import to finish", |s| {
        s.project
            .media_library
            .first()
            .is_some_and(|a| a.loudness.is_some())
    });
    let session = h.session();
    assert_eq!(session.project.media_library[0].kind, MediaKind::Audio);
    assert!(session.project.media_library[0].proxy_path.is_none());
}

#[test]
fn importing_assigns_increasing_ids() {
    let h = saved_harness();
    let audio = fixture("audio.m4a");
    let (epoch, proxy_dir) = {
        let s = h.session();
        (
            s.project_epoch,
            avcore::proxy::cache_dir_for_project(&s.project),
        )
    };
    import_one(&h.handle(), &audio, epoch, &proxy_dir);
    import_one(&h.handle(), &audio, epoch, &proxy_dir);
    let ids: Vec<u64> = h
        .session()
        .project
        .media_library
        .iter()
        .map(|a| a.id)
        .collect();
    assert_eq!(ids, vec![1, 2]);
}

#[test]
fn a_file_that_cannot_be_probed_adds_nothing() {
    let h = saved_harness();
    let events = h.count_session_events();
    let (epoch, proxy_dir) = {
        let s = h.session();
        (
            s.project_epoch,
            avcore::proxy::cache_dir_for_project(&s.project),
        )
    };
    import_one(
        &h.handle(),
        &path_in(&h.dir, "missing.mp4"),
        epoch,
        &proxy_dir,
    );
    assert!(h.session().project.media_library.is_empty());
    assert_eq!(events.load(Ordering::SeqCst), 0);
}

#[test]
fn an_import_for_a_replaced_project_is_dropped() {
    let h = saved_harness();
    let proxy_dir = avcore::proxy::cache_dir_for_project(&h.session().project);
    let stale = h.session().project_epoch + 1;
    import_one(&h.handle(), &fixture("audio.m4a"), stale, &proxy_dir);
    assert!(h.session().project.media_library.is_empty());
}

#[test]
fn the_import_dialog_imports_the_picked_files_and_a_cancel_imports_none() {
    let h = saved_harness();
    crate::dialogs::scripted::answer(None);
    tauri::async_runtime::block_on(import_media_dialog(h.handle())).unwrap();
    assert!(h.session().project.media_library.is_empty());

    crate::dialogs::scripted::answer(Some(vec![fixture("audio.m4a")]));
    tauri::async_runtime::block_on(import_media_dialog(h.handle())).unwrap();
    h.wait_until("the dialog import", |s| {
        s.project
            .media_library
            .first()
            .is_some_and(|a| a.loudness.is_some())
    });
}

#[test]
fn rehydrating_rebuilds_missing_proxies_for_video_assets_that_exist() {
    let h = saved_harness();
    let mut present = video_asset(1, 1.0);
    present.source_path = fixture("video.mp4");
    let gone = video_asset(2, 1.0);
    let mut audio = video_asset(3, 1.0);
    audio.kind = MediaKind::Audio;
    h.session()
        .project
        .media_library
        .extend([present, gone, audio]);
    let epoch = h.session().project_epoch;
    rehydrate_proxies(&h.handle(), epoch);
    h.wait_until("the proxy rebuild", |s| {
        s.project.media_library[0].proxy_path.is_some()
    });
    let session = h.session();
    assert!(session.project.media_library[1].proxy_path.is_none());
    assert!(session.project.media_library[2].proxy_path.is_none());
}

#[test]
fn rehydrating_with_nothing_to_rebuild_does_nothing() {
    let h = saved_harness();
    let events = h.count_session_events();
    let epoch = h.session().project_epoch;
    rehydrate_proxies(&h.handle(), epoch);
    assert_eq!(events.load(Ordering::SeqCst), 0);
}

#[test]
fn a_rebuild_for_a_replaced_project_stops_without_touching_it() {
    let h = saved_harness();
    let mut asset = video_asset(1, 1.0);
    asset.source_path = fixture("video.mp4");
    h.session().project.media_library.push(asset);
    let stale = h.session().project_epoch + 1;
    rehydrate_proxies(&h.handle(), stale);
    std::thread::sleep(std::time::Duration::from_secs(3));
    assert!(h.session().project.media_library[0].proxy_path.is_none());
}
