// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! The `oca://` URI scheme: binary image data the webview loads with plain `<img>` tags
//! instead of pushing bytes through JSON IPC.
//!
//! - `/frame/<seq>` -- the latest preview frame (`seq` is only a cache buster).
//! - `/thumb/<asset_id>` -- a media-library poster frame, extracted once and cached.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime, UriSchemeResponder};

use crate::frames::{encode_jpeg, THUMBNAIL_MAX_DIM};
use crate::state::AppState;

pub const SCHEME: &str = "oca";

/// Where in a clip the poster frame comes from -- past the typical black lead-in of a capture.
const THUMBNAIL_AT_SECS: f64 = 1.0;

#[derive(Default)]
pub struct ThumbnailCache {
    entries: Mutex<HashMap<PathBuf, Option<Arc<Vec<u8>>>>>,
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, path: &str, responder: UriSchemeResponder) {
    handle_with(app, path, move |response| responder.respond(response));
}

/// [`handle`] with the answer delivered through `respond`, so tests can read it back.
pub(crate) fn handle_with<R: Runtime>(
    app: &AppHandle<R>,
    path: &str,
    respond: impl FnOnce(Response<Vec<u8>>) + Send + 'static,
) {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some("frame"), _) => {
            let frame = app.state::<AppState>().preview.latest_frame();
            respond(jpeg_or_not_found(frame));
        }
        (Some("thumb"), Some(id)) => {
            let Ok(asset_id) = id.parse::<u64>() else {
                respond(not_found());
                return;
            };
            let source = app
                .state::<AppState>()
                .session()
                .project
                .media_library
                .iter()
                .find(|asset| asset.id == asset_id)
                .filter(|asset| asset.kind == avcore::MediaKind::Video)
                .map(|asset| {
                    asset
                        .proxy_path
                        .clone()
                        .unwrap_or_else(|| asset.source_path.clone())
                });
            let Some(source) = source else {
                respond(not_found());
                return;
            };
            let app = app.clone();
            // Frame sampling decodes video; keep it off the webview's IPC thread.
            std::thread::spawn(move || {
                let cache = app.state::<ThumbnailCache>();
                respond(jpeg_or_not_found(cache.get_or_extract(&source)));
            });
        }
        _ => respond(not_found()),
    }
}

impl ThumbnailCache {
    fn get_or_extract(&self, source: &Path) -> Option<Arc<Vec<u8>>> {
        if let Some(cached) = self.entries.lock().ok()?.get(source) {
            return cached.clone();
        }
        let extracted = extract_thumbnail(source).map(Arc::new);
        if let Ok(mut entries) = self.entries.lock() {
            entries.insert(source.to_path_buf(), extracted.clone());
        }
        extracted
    }
}

fn extract_thumbnail(path: &Path) -> Option<Vec<u8>> {
    if !path.exists() {
        return None;
    }
    let sampler = avcore::FrameSampler::open(path).ok()?;
    let frame = sampler.sample(THUMBNAIL_AT_SECS, Duration::from_millis(800))?;
    encode_jpeg(&frame, THUMBNAIL_MAX_DIM)
}

fn jpeg_or_not_found(bytes: Option<Arc<Vec<u8>>>) -> Response<Vec<u8>> {
    match bytes {
        Some(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, "image/jpeg")
            .header(header::CACHE_CONTROL, "no-store")
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(bytes.as_ref().clone())
            .unwrap_or_else(|_| not_found()),
        None => not_found(),
    }
}

fn not_found() -> Response<Vec<u8>> {
    let mut response = Response::new(Vec::new());
    *response.status_mut() = StatusCode::NOT_FOUND;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../crates/core/tests/fixtures/video.mp4")
    }

    #[test]
    fn a_missing_image_is_a_404_without_a_body() {
        let response = jpeg_or_not_found(None);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(response.body().is_empty());
        assert_eq!(not_found().status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn an_image_is_served_as_an_uncached_jpeg_open_to_the_webview() {
        let response = jpeg_or_not_found(Some(Arc::new(vec![0xFF, 0xD8, 0xFF])));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/jpeg");
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        assert_eq!(response.headers()[header::ACCESS_CONTROL_ALLOW_ORIGIN], "*");
        assert_eq!(response.body(), &vec![0xFF, 0xD8, 0xFF]);
    }

    #[test]
    fn thumbnails_are_extracted_once_and_failures_are_remembered() {
        let cache = ThumbnailCache::default();

        let jpeg = cache
            .get_or_extract(&fixture())
            .expect("a thumbnail of the fixture");
        assert_eq!(
            &jpeg[..2],
            &[0xFF, 0xD8],
            "a JPEG starts with its SOI marker"
        );
        // The second request returns the very same bytes from the cache.
        let again = cache.get_or_extract(&fixture()).unwrap();
        assert!(Arc::ptr_eq(&jpeg, &again));

        let missing = Path::new("definitely-missing.mp4");
        assert!(cache.get_or_extract(missing).is_none());
        assert!(
            cache.get_or_extract(missing).is_none(),
            "the miss is cached too"
        );
        assert!(cache.entries.lock().unwrap().contains_key(missing));
    }

    #[test]
    fn a_file_that_is_not_media_has_no_thumbnail() {
        let path =
            std::env::temp_dir().join(format!("oca_desktop_thumb_{}.bin", std::process::id()));
        std::fs::write(&path, b"not a video").unwrap();
        assert!(extract_thumbnail(&path).is_none());
        let _ = std::fs::remove_file(&path);
        assert!(extract_thumbnail(Path::new("missing.mp4")).is_none());
    }
}

#[cfg(test)]
#[path = "protocol_test.rs"]
mod handler_tests;
