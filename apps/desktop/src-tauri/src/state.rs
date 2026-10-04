// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Backend-owned editing session. The webview never holds authoritative project state: every
//! edit is a command that mutates [`Session`] here and then republishes the whole
//! [`SessionView`] as the `session-changed` event, so undo, persistence and preview all read
//! one source of truth -- the same role `ui::App` plays for the egui shell.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use avcore::timeline::{ClipFormatting, ClipInstance, TrackKind};
use avcore::undo::UndoStack;
use avcore::{Project, Sequence, SequenceExportSettings, Timeline};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::export::ExportQueue;
use crate::prefs::PrefsDocument;
use crate::preview_engine::PreviewEngine;

/// Event carrying a fresh [`SessionView`] after any mutation.
pub const SESSION_CHANGED: &str = "session-changed";

pub struct Session {
    pub project: Project,
    pub undo: UndoStack,
    /// Unsaved edits since the last load/save.
    pub dirty: bool,
    /// Bumped whenever the project is replaced (new/open) so import enrichment results for the
    /// previous project are dropped instead of patched into the wrong media library.
    pub project_epoch: u64,
}

impl Session {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: UndoStack::new(),
            dirty: false,
            project_epoch: 0,
        }
    }

    /// Replaces the open project, resetting undo history and invalidating in-flight imports.
    pub fn replace_project(&mut self, project: Project) {
        self.project = project;
        self.undo.clear();
        self.dirty = false;
        self.project_epoch = self.project_epoch.wrapping_add(1);
    }

    pub fn undo(&mut self) -> bool {
        let current = self.project.active_sequence().clone();
        self.restore(|undo| undo.undo(current))
    }

    pub fn redo(&mut self) -> bool {
        let current = self.project.active_sequence().clone();
        self.restore(|undo| undo.redo(current))
    }

    fn restore(&mut self, step: impl FnOnce(&mut UndoStack) -> Option<Sequence>) -> bool {
        let Some(sequence) = step(&mut self.undo) else {
            return false;
        };
        // The playhead is navigation, not an edit: keep it where the user left it.
        let playhead = self.project.timeline().playhead_secs;
        *self.project.active_sequence_mut() = sequence;
        self.project.timeline_mut().playhead_secs = playhead;
        self.dirty = true;
        true
    }

    pub fn view(&self) -> SessionView<'_> {
        SessionView {
            project: &self.project,
            file_path: self
                .project
                .file_path
                .as_ref()
                .map(|p| p.display().to_string()),
            can_undo: self.undo.can_undo(),
            can_redo: self.undo.can_redo(),
            dirty: self.dirty,
        }
    }
}

/// What the webview renders from. `project` serializes with `avcore`'s own serde derives, so
/// the TypeScript mirror in `src/types.ts` follows the persisted field names exactly.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView<'a> {
    pub project: &'a Project,
    pub file_path: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

pub struct AppState {
    pub session: Mutex<Session>,
    pub prefs: Mutex<PrefsDocument>,
    pub preview: PreviewEngine,
    pub exports: Arc<ExportQueue>,
    /// What `Ctrl+C`/`Ctrl+X` last captured, for `Ctrl+V` -- transient, never serialized, and
    /// not scoped to a project or sequence, same as `ui::App::clipboard_clip`.
    pub clip_clipboard: Mutex<Option<(Vec<ClipInstance>, TrackKind)>>,
    /// What `Ctrl+Shift+C` last captured, for `Ctrl+Shift+V` -- same transience as
    /// `clip_clipboard`, mirroring `ui::App::formatting_clipboard`.
    pub formatting_clipboard: Mutex<Option<ClipFormatting>>,
    /// The asset a background transcription is currently running for, if any -- only one at a
    /// time, mirrors `ui::App::transcribe_state.transcribing_asset_id`.
    pub transcribing_asset_id: Mutex<Option<u64>>,
    /// Single-flight guards for the remaining background AI features, each mirroring the
    /// matching `ui::App::*_state` field of the same name.
    pub scene_cut_detection_clip_id: Mutex<Option<u64>>,
    pub motion_tracking_clip_id: Mutex<Option<u64>>,
    pub auto_reframing_clip_id: Mutex<Option<u64>>,
    pub matte_generating_clip_id: Mutex<Option<u64>>,
    pub privacy_blur_generating_clip_id: Mutex<Option<u64>>,
    pub tts_generating: Mutex<bool>,
    /// The running watch-folder loop's stop flag, if any -- `Some` while a watch is active.
    /// Mirrors `ui::App::watch_folder_state.stop`.
    pub watch_folder_stop: Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
    /// The running YouTube download's cancel flag, if any. Mirrors
    /// `ui::App::youtube_download_state.youtube_download_cancel`.
    pub youtube_download_cancel: Mutex<Option<Arc<std::sync::atomic::AtomicBool>>>,
    /// This launch's error reporter, if ER-01B consent is `AlwaysSend`. `None` is the
    /// consent-disabled state: no report is ever built, validated, queued, or sent. Mirrors
    /// `ui::App::error_reporter`.
    pub error_reporter: Mutex<Option<Arc<dyn avcore::ErrorReporter>>>,
    /// A previous launch's crash, staged for the post-crash review modal, if `main.rs`'s panic
    /// hook wrote one newer than `prefs.last_reviewed_crash_unix`. Mirrors
    /// `ui::App::pending_crash_review`.
    pub pending_crash_review: Mutex<Option<crate::error_reporting::PendingCrashReview>>,
    /// Live toggle for `telemetry.rs`'s resource samplers -- checked each sampling tick, kept in
    /// sync with `prefs.telemetry_enabled` by `commands::prefs::set_prefs`.
    pub telemetry_enabled: Arc<std::sync::atomic::AtomicBool>,
    /// [`avcore::nested_sequence::materialize_nested_sequences`]'s cache for `start_export` --
    /// separate from `PreviewEngine`'s own copy (it lives on that dedicated thread), so a
    /// compound clip's nested content isn't re-rendered on every export of the same sequence.
    pub nested_sequence_cache: Mutex<HashMap<u64, avcore::nested_sequence::NestedSequenceCache>>,
    /// Calls that leave the process or need model files; faked in tests.
    pub services: Arc<dyn crate::services::Services>,
}

impl AppState {
    /// The state the app boots with: a blank project, empty clipboards and no background job
    /// running. `main` and the test harness differ only in the pieces passed in.
    pub fn new(
        prefs: PrefsDocument,
        preview: PreviewEngine,
        exports: Arc<ExportQueue>,
        error_reporter: Option<Arc<dyn avcore::ErrorReporter>>,
        pending_crash_review: Option<crate::error_reporting::PendingCrashReview>,
        telemetry_enabled: Arc<std::sync::atomic::AtomicBool>,
        services: Arc<dyn crate::services::Services>,
    ) -> Self {
        Self {
            session: Mutex::new(Session::new(blank_project(
                "Untitled".to_string(),
                "Sequence 1".to_string(),
            ))),
            prefs: Mutex::new(prefs),
            preview,
            exports,
            clip_clipboard: Mutex::new(None),
            formatting_clipboard: Mutex::new(None),
            transcribing_asset_id: Mutex::new(None),
            scene_cut_detection_clip_id: Mutex::new(None),
            motion_tracking_clip_id: Mutex::new(None),
            auto_reframing_clip_id: Mutex::new(None),
            matte_generating_clip_id: Mutex::new(None),
            privacy_blur_generating_clip_id: Mutex::new(None),
            tts_generating: Mutex::new(false),
            watch_folder_stop: Mutex::new(None),
            youtube_download_cancel: Mutex::new(None),
            error_reporter: Mutex::new(error_reporter),
            pending_crash_review: Mutex::new(pending_crash_review),
            telemetry_enabled,
            nested_sequence_cache: Mutex::new(std::collections::HashMap::new()),
            services,
        }
    }

    pub fn session(&self) -> MutexGuard<'_, Session> {
        // A poisoned lock means a command panicked mid-edit; the data is still the last
        // consistent snapshot any other command left, so keep serving it.
        self.session
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn prefs(&self) -> MutexGuard<'_, PrefsDocument> {
        self.prefs
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

/// Emits the current session to the webview and hands the timeline to the preview engine.
/// Every command that mutates [`Session`] ends with this.
pub fn publish<R: Runtime>(app: &AppHandle<R>, session: &Session) {
    let state = app.state::<AppState>();
    state.preview.set_content(session.project.clone());
    let _ = app.emit(SESSION_CHANGED, session.view());
}

/// An empty, never-saved project -- what "Novo projeto" builds in the egui shell.
pub fn blank_project(name: String, sequence_name: String) -> Project {
    Project {
        id: 1,
        name,
        last_edited: avcore::Recency::HoursAgo(0),
        summary: String::new(),
        media_library: Vec::new(),
        sequences: vec![Sequence {
            id: 1,
            name: sequence_name,
            timeline: Timeline {
                tracks: Vec::new(),
                playhead_secs: 0.0,
                markers: Vec::new(),
                multicam_groups: Vec::new(),
            },
            export_settings: SequenceExportSettings::default(),
        }],
        active_sequence: 0,
        file_path: None,
        panel_layout: None,
        smart_bins: Vec::new(),
        recent_asset_ids: Vec::new(),
    }
}
