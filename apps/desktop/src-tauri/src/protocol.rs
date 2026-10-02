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
use tauri::{AppHandle, Manager, UriSchemeResponder};

use crate::frames::{encode_jpeg, THUMBNAIL_MAX_DIM};
use crate::state::AppState;

pub const SCHEME: &str = "oca";

/// Where in a clip the poster frame comes from -- past the typical black lead-in of a capture.
const THUMBNAIL_AT_SECS: f64 = 1.0;

#[derive(Default)]
pub struct ThumbnailCache {
    entries: Mutex<HashMap<PathBuf, Option<Arc<Vec<u8>>>>>,
}

pub fn handle(app: &AppHandle, path: &str, responder: UriSchemeResponder) {
    let mut parts = path.trim_start_matches('/').splitn(2, '/');
    match (parts.next(), parts.next()) {
        (Some("frame"), _) => {
            let frame = app.state::<AppState>().preview.latest_frame();
            responder.respond(jpeg_or_not_found(frame));
        }
        (Some("thumb"), Some(id)) => {
            let Ok(asset_id) = id.parse::<u64>() else {
                responder.respond(not_found());
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
                responder.respond(not_found());
                return;
            };
            let app = app.clone();
            // Frame sampling blocks on GStreamer preroll; keep it off the webview's IPC thread.
            std::thread::spawn(move || {
                let cache = app.state::<ThumbnailCache>();
                responder.respond(jpeg_or_not_found(cache.get_or_extract(&source)));
            });
        }
        _ => responder.respond(not_found()),
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
    let sampler = avcore::FrameSampler::open(path, Duration::from_millis(20)).ok()?;
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
