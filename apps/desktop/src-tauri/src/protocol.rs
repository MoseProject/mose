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
//! - `/thumb/<asset_id>/<millis>` -- the frame of an asset at that source time (timeline strips).
//! - `/wave/<asset_id>` -- the asset's detailed waveform: little-endian `f32` (min, max) pairs,
//!   `detailed_bucket_count(duration)` of them, computed on first request and cached.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Manager, Runtime};

use crate::frames::{encode_jpeg, THUMBNAIL_MAX_DIM};
use crate::state::AppState;

pub const SCHEME: &str = "oca";

/// Where in a clip the poster frame comes from -- past the typical black lead-in of a capture.
const THUMBNAIL_AT_SECS: f64 = 1.0;

/// Timeline frame strips ask for many distinct times; past this many cached tiles the cache is
/// dropped wholesale (a tile is ~10 KB, so this bounds it near 15 MB) and refills as needed.
const MAX_TILE_ENTRIES: usize = 1500;

type Bytes = Option<Arc<Vec<u8>>>;

#[derive(Default)]
pub struct ThumbnailCache {
    entries: Mutex<HashMap<PathBuf, Bytes>>,
    tiles: Mutex<HashMap<(PathBuf, u64), Bytes>>,
    waves: Mutex<HashMap<PathBuf, Arc<OnceLock<Bytes>>>>,
}

/// Answers one `oca://` request through `respond`; `main` hands it the scheme responder.
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
        (Some("wave"), Some(id)) => {
            let Some(asset) = id.parse::<u64>().ok().and_then(|id| find_asset(app, id)) else {
                respond(not_found());
                return;
            };
            if !asset.has_audio {
                respond(not_found());
                return;
            }
            let app = app.clone();
            // A full audio decode: minutes on a multi-hour recording, never on the IPC thread.
            std::thread::spawn(move || {
                let cache = app.state::<ThumbnailCache>();
                let bytes = cache.get_or_wave(&asset.source_path, asset.duration_secs);
                respond(bytes_or_not_found(bytes, "application/octet-stream"));
            });
        }
        (Some("thumb"), Some(rest)) => {
            let mut ids = rest.splitn(2, '/');
            let Some(asset_id) = ids.next().and_then(|id| id.parse::<u64>().ok()) else {
                respond(not_found());
                return;
            };
            let at_millis = ids.next().and_then(|ms| ms.parse::<u64>().ok());
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
                let bytes = match at_millis {
                    Some(millis) => cache.get_or_extract_at(&source, millis),
                    None => cache.get_or_extract(&source),
                };
                respond(jpeg_or_not_found(bytes));
            });
        }
        _ => respond(not_found()),
    }
}

fn find_asset<R: Runtime>(app: &AppHandle<R>, asset_id: u64) -> Option<avcore::MediaAsset> {
    app.state::<AppState>()
        .session()
        .project
        .media_library
        .iter()
        .find(|asset| asset.id == asset_id)
        .cloned()
}

impl ThumbnailCache {
    fn get_or_extract_at(&self, source: &Path, millis: u64) -> Bytes {
        let key = (source.to_path_buf(), millis);
        if let Some(cached) = self.tiles.lock().ok()?.get(&key) {
            return cached.clone();
        }
        let extracted = extract_frame_at(source, millis as f64 / 1000.0).map(Arc::new);
        if let Ok(mut tiles) = self.tiles.lock() {
            if tiles.len() >= MAX_TILE_ENTRIES {
                tiles.clear();
            }
            tiles.insert(key, extracted.clone());
        }
        extracted
    }

    /// The detailed waveform of `source` as little-endian `f32` (min, max) pairs. Concurrent
    /// requests for one file share a single decode pass.
    fn get_or_wave(&self, source: &Path, duration_secs: f64) -> Bytes {
        let cell = {
            let mut waves = self.waves.lock().ok()?;
            waves.entry(source.to_path_buf()).or_default().clone()
        };
        cell.get_or_init(|| {
            let peaks = avcore::generate_detailed_waveform(source, duration_secs)
                .inspect_err(|error| tracing::warn!(%error, "failed to compute waveform"))
                .ok()?;
            let mut bytes = Vec::with_capacity(peaks.len() * 8);
            for (min, max) in peaks {
                bytes.extend_from_slice(&min.to_le_bytes());
                bytes.extend_from_slice(&max.to_le_bytes());
            }
            Some(Arc::new(bytes))
        })
        .clone()
    }

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
    extract_frame_at(path, THUMBNAIL_AT_SECS)
}

fn extract_frame_at(path: &Path, at_secs: f64) -> Option<Vec<u8>> {
    if !path.exists() {
        return None;
    }
    let sampler = avcore::FrameSampler::open(path).ok()?;
    let frame = sampler.sample(at_secs, Duration::from_millis(800))?;
    encode_jpeg(&frame, THUMBNAIL_MAX_DIM)
}

fn jpeg_or_not_found(bytes: Option<Arc<Vec<u8>>>) -> Response<Vec<u8>> {
    bytes_or_not_found(bytes, "image/jpeg")
}

fn bytes_or_not_found(bytes: Option<Arc<Vec<u8>>>, content_type: &str) -> Response<Vec<u8>> {
    match bytes {
        Some(bytes) => Response::builder()
            .header(header::CONTENT_TYPE, content_type)
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
