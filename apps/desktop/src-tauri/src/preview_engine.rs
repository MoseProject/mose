// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Playback coordinator handle for the Tauri shell.
//!
//! A webview has no per-frame UI loop on the Rust side, so this module owns the handle the
//! commands talk to: a dedicated thread (the Bush playback loop in [`crate::bush_adapter`])
//! receives timeline snapshots and transport commands over a channel. Decoded frames are
//! JPEG-encoded into a one-slot [`LatestFrame`] served by the `oca://` protocol; the webview
//! learns a new one exists from the throttled [`PREVIEW_TICK`] event.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use avcore::{MediaAsset, Project};
use serde::Serialize;
use tauri::{AppHandle, Runtime};

/// Event carrying a [`PreviewTick`].
pub const PREVIEW_TICK: &str = "preview-tick";

/// Upper bound on tick events per second while playing -- enough for a smooth playhead
/// without flooding the IPC bridge.
pub(crate) const TICK_INTERVAL: Duration = Duration::from_millis(16);
/// How long the engine parks waiting for a command while nothing is playing.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_millis(8);
/// Fallback fps for frame stepping when the clip's asset has none (same guess as `avcore::render`).
pub(crate) const FALLBACK_FPS: f64 = 30.0;

/// The most recent encoded preview frame, shared with the `oca://` protocol handler.
pub type LatestFrame = Arc<Mutex<Option<Arc<Vec<u8>>>>>;

pub(crate) enum Command {
    Content { project: Box<Project> },
    Seek(f64),
    Step(i64),
    Playing(bool),
}

/// Snapshot of transport state for the webview.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewTick {
    /// Increments every time [`LatestFrame`] changes.
    pub frame_seq: u64,
    pub playhead_secs: f64,
    pub playing: bool,
    /// A clip covers the timeline (whether or not it decoded).
    pub has_clip: bool,
    /// The engine has produced a frame for the current scene.
    pub available: bool,
    pub error: Option<String>,
    pub peak_l: f32,
    pub peak_r: f32,
    /// Momentary loudness in LUFS while playing; `None` when silent or stopped.
    pub momentary_lufs: Option<f32>,
}

pub struct PreviewEngine {
    tx: Sender<Command>,
    latest: LatestFrame,
    playhead_bits: Arc<AtomicU64>,
}

impl PreviewEngine {
    pub fn spawn<R: Runtime>(app: AppHandle<R>) -> Self {
        Self::spawn_with(app, |rx, app, latest, playhead| {
            crate::bush_adapter::run(rx, app, latest, playhead)
        })
    }

    /// [`spawn`] with the playback loop passed in, so tests can run it with a fake audio output.
    pub(crate) fn spawn_with<R: Runtime>(
        app: AppHandle<R>,
        run: impl FnOnce(mpsc::Receiver<Command>, AppHandle<R>, LatestFrame, Arc<AtomicU64>)
            + Send
            + 'static,
    ) -> Self {
        let (tx, rx) = mpsc::channel();
        let latest: LatestFrame = Arc::new(Mutex::new(None));
        let playhead_bits = Arc::new(AtomicU64::new(0f64.to_bits()));
        let engine_latest = Arc::clone(&latest);
        let engine_playhead = Arc::clone(&playhead_bits);
        std::thread::Builder::new()
            .name("oca-preview-engine".into())
            .spawn(move || run(rx, app, engine_latest, engine_playhead))
            .expect("spawning the preview engine thread");
        Self {
            tx,
            latest,
            playhead_bits,
        }
    }

    /// An engine with no playback thread behind it: the receiver hands back every command a
    /// test's code under test sent, so assertions can read them in order.
    #[cfg(test)]
    pub(crate) fn detached() -> (Self, mpsc::Receiver<Command>) {
        let (tx, rx) = mpsc::channel();
        let engine = Self {
            tx,
            latest: Arc::new(Mutex::new(None)),
            playhead_bits: Arc::new(AtomicU64::new(0f64.to_bits())),
        };
        (engine, rx)
    }

    /// Pretends the engine produced `bytes` as its latest preview frame.
    #[cfg(test)]
    pub(crate) fn set_latest_frame(&self, bytes: Vec<u8>) {
        *self.latest.lock().unwrap() = Some(Arc::new(bytes));
    }

    /// Pretends playback reached `secs`, which is what edits read as the live playhead.
    #[cfg(test)]
    pub(crate) fn set_playhead(&self, secs: f64) {
        self.playhead_bits.store(secs.to_bits(), Ordering::Release);
    }

    pub fn set_content(&self, project: Project) {
        let _ = self.tx.send(Command::Content {
            project: Box::new(project),
        });
    }

    pub fn seek(&self, position_secs: f64) {
        let _ = self.tx.send(Command::Seek(position_secs));
    }

    pub fn step(&self, delta_frames: i64) {
        let _ = self.tx.send(Command::Step(delta_frames));
    }

    pub fn set_playing(&self, playing: bool) {
        let _ = self.tx.send(Command::Playing(playing));
    }

    /// The live playhead. Authoritative over `Timeline::playhead_secs` in the session, which
    /// only catches up when an edit or save needs it.
    pub fn playhead(&self) -> f64 {
        f64::from_bits(self.playhead_bits.load(Ordering::Acquire))
    }

    pub fn latest_frame(&self) -> Option<Arc<Vec<u8>>> {
        self.latest
            .lock()
            .ok()
            .and_then(|frame| frame.as_ref().map(Arc::clone))
    }
}

/// The source a preview should decode for `asset`: its editing proxy when one exists, else the
/// original -- `None` if neither is on disk (moved/deleted media).
pub(crate) fn preview_source_path(asset: &MediaAsset) -> Option<PathBuf> {
    // The proxy is a cache and can be cleaned away while the original is still there, so a proxy
    // that is not on disk falls back to the original instead of hiding the clip.
    asset
        .proxy_path
        .iter()
        .chain(std::iter::once(&asset.source_path))
        .find(|path| path.exists())
        .cloned()
}

#[cfg(test)]
#[path = "preview_engine_test.rs"]
mod tests;
