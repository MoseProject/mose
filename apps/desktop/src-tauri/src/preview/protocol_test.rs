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
fn a_thumbnail_falls_back_to_the_original_when_the_proxy_was_cleaned_away() {
    let h = harness_with_cache();
    let mut asset = video_asset(7, 5.0);
    asset.source_path = fixture("video.mp4");
    asset.proxy_path = Some(h.dir.join("cleaned-away.mp4"));
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

#[test]
fn the_timed_thumbnail_route_serves_the_frame_at_a_source_time() {
    let h = harness_with_cache();
    let mut asset = video_asset(7, 5.0);
    asset.source_path = fixture("video.mp4");
    h.session().project.media_library.push(asset);
    let response = answer(&h, "/thumb/7/1500");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(&response.body()[..2], &[0xFF, 0xD8]);
    // The same time is served from the cache.
    assert_eq!(answer(&h, "/thumb/7/1500").body(), response.body());
    assert_eq!(answer(&h, "/thumb/7/not-a-time").status(), StatusCode::OK);
}

#[test]
fn the_waveform_route_serves_little_endian_peak_pairs() {
    let h = harness_with_cache();
    let mut asset = video_asset(7, 5.0);
    asset.source_path = fixture("video.mp4");
    asset.has_audio = true;
    h.session().project.media_library.push(asset);
    let response = answer(&h, "/wave/7");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers()[header::CONTENT_TYPE],
        "application/octet-stream"
    );
    let expected = avcore::detailed_bucket_count(5.0) * 8;
    assert_eq!(response.body().len(), expected);
    let floats: Vec<f32> = response
        .body()
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .collect();
    assert!(floats.chunks_exact(2).all(|p| p[0] <= p[1]));
    assert!(floats.iter().any(|v| *v != 0.0));
}

#[test]
fn the_waveform_route_is_404_without_audio_or_for_unknown_assets() {
    let h = harness_with_cache();
    let mut silent = video_asset(8, 5.0);
    silent.has_audio = false;
    h.session().project.media_library.push(silent);
    for path in ["/wave/8", "/wave/99", "/wave/not-a-number", "/wave"] {
        assert_eq!(answer(&h, path).status(), StatusCode::NOT_FOUND, "{path}");
    }
}
