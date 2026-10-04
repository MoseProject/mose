use std::sync::mpsc;
use std::time::Duration;

use tauri::Manager;

use super::*;
use crate::testing::{fixture, harness, video_asset, Harness};

fn answer(h: &Harness, path: &str) -> Response<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    handle_with(&h.handle(), path, move |response| {
        tx.send(response).unwrap();
    });
    rx.recv_timeout(Duration::from_secs(30))
        .expect("the handler answers")
}

fn harness_with_cache() -> Harness {
    let h = harness();
    h.app.manage(ThumbnailCache::default());
    h
}

#[test]
fn the_frame_route_serves_the_latest_preview_frame_or_a_404() {
    let h = harness_with_cache();
    assert_eq!(answer(&h, "/frame/1").status(), StatusCode::NOT_FOUND);
    h.state().preview.set_latest_frame(vec![0xFF, 0xD8, 0xFF]);
    let response = answer(&h, "/frame/2");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.body(), &vec![0xFF, 0xD8, 0xFF]);
}

#[test]
fn the_thumbnail_route_serves_a_jpeg_for_a_video_asset() {
    let h = harness_with_cache();
    let mut asset = video_asset(7, 5.0);
    asset.source_path = fixture("video.mp4");
    h.session().project.media_library.push(asset);
    let response = answer(&h, "/thumb/7");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(&response.body()[..2], &[0xFF, 0xD8]);
}

#[test]
fn a_thumbnail_comes_from_the_proxy_when_there_is_one() {
    let h = harness_with_cache();
    let mut asset = video_asset(7, 5.0);
    asset.source_path = h.dir.join("missing-original.mp4");
    asset.proxy_path = Some(fixture("video.mp4"));
    h.session().project.media_library.push(asset);
    assert_eq!(answer(&h, "/thumb/7").status(), StatusCode::OK);
}

#[test]
fn thumbnails_for_unknown_audio_or_malformed_ids_are_404() {
    let h = harness_with_cache();
    let mut audio = video_asset(8, 5.0);
    audio.kind = avcore::MediaKind::Audio;
    h.session().project.media_library.push(audio);
    for path in [
        "/thumb/99",
        "/thumb/8",
        "/thumb/not-a-number",
        "/thumb",
        "/other",
        "/",
    ] {
        assert_eq!(answer(&h, path).status(), StatusCode::NOT_FOUND, "{path}");
    }
}

#[test]
fn a_video_whose_file_is_gone_has_no_thumbnail() {
    let h = harness_with_cache();
    h.session().project.media_library.push(video_asset(9, 5.0));
    assert_eq!(answer(&h, "/thumb/9").status(), StatusCode::NOT_FOUND);
}
