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

/// Event carrying only what changed since the previous one ([`crate::session_sync::Patch`]).
pub const SESSION_PATCH: &str = "session-patch";

pub struct Session {
    pub project: Project,
    pub undo: UndoStack,
    /// Unsaved edits since the last load/save.
    pub dirty: bool,
    /// Bumped whenever the project is replaced (new/open) so import enrichment results for the
    /// previous project are dropped instead of patched into the wrong media library.
    pub project_epoch: u64,
    /// Highest media id handed out in this session. Ids never go back down, so removing the
    /// newest asset cannot let a later import reuse its id while background work for the removed
    /// one is still running.
    pub last_asset_id: u64,
}

impl Session {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            undo: UndoStack::new(),
            dirty: false,
            project_epoch: 0,
            last_asset_id: 0,
        }
    }

    /// A media id no asset in the library has, and none handed out before in this session.
    pub fn allocate_asset_id(&mut self) -> u64 {
        let in_library = self
            .project
            .media_library
            .iter()
            .map(|asset| asset.id)
            .max()
            .unwrap_or(0);
        let id = in_library.max(self.last_asset_id) + 1;
        self.last_asset_id = id;
        id
    }

    /// Replaces the open project, resetting undo history and invalidating in-flight imports.
    pub fn replace_project(&mut self, project: Project) {
        self.project = project;
        self.undo.clear();
        self.dirty = false;
        self.project_epoch = self.project_epoch.wrapping_add(1);
        self.last_asset_id = 0;
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
        self.view_at(0)
    }

    /// The view as the webview's revision `rev` of the session.
    pub fn view_at(&self, rev: u64) -> SessionView<'_> {
        SessionView {
            rev,
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
    /// Revision of the update stream this snapshot is current as of.
    pub rev: u64,
    pub project: &'a Project,
    pub file_path: Option<String>,
    pub can_undo: bool,
    pub can_redo: bool,
    pub dirty: bool,
}

/// Counts the edits made to the open project and remembers when the last one happened, so the
/// auto-save knows both that something changed and that the changes have settled.
pub struct EditClock {
    seq: std::sync::atomic::AtomicU64,
    last: Mutex<std::time::Instant>,
}

impl Default for EditClock {
    fn default() -> Self {
        Self {
            seq: std::sync::atomic::AtomicU64::new(0),
            last: Mutex::new(std::time::Instant::now()),
        }
    }
}

impl EditClock {
    /// Records an edit now.
    pub fn touch(&self) {
        self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        *lock_or_recover(&self.last) = std::time::Instant::now();
    }

    /// How many edits so far.
    pub fn seq(&self) -> u64 {
        self.seq.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The edit count and how long ago the last edit was.
    pub fn snapshot(&self) -> (u64, std::time::Duration) {
        (self.seq(), lock_or_recover(&self.last).elapsed())
    }
}

/// Locks `mutex` even when another thread panicked while holding it. What these locks guard
/// (caches, single-slot buffers) is still a usable value afterwards, and giving up on it would
/// turn one panic into a feature that stays broken until the app restarts.
pub(crate) fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
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
    /// When the project last changed, for the auto-save.
    pub edits: EditClock,
    /// What the webview has been sent, so the next update can be only the difference.
    pub sync: Mutex<crate::session_sync::SyncState>,
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
            edits: EditClock::default(),
            sync: Mutex::new(crate::session_sync::SyncState::default()),
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
/// Every command that mutates [`Session`] ends with this. The first update, and the first after
/// another project was opened, is the whole session; the rest are only what changed.
pub fn publish<R: Runtime>(app: &AppHandle<R>, session: &Session) {
    let state = app.state::<AppState>();
    if session.dirty {
        state.edits.touch();
    }
    let next = Arc::new(session.project.clone());
    state.preview.set_content_shared(Arc::clone(&next));
    // Held across the emit so updates leave in the order their revisions were given.
    let mut sync = lock_or_recover(&state.sync);
    sync.rev += 1;
    let rev = sync.rev;
    let patch = match &sync.published {
        Some(prev) if sync.epoch == session.project_epoch => {
            Some(crate::session_sync::diff(prev, &next))
        }
        _ => None,
    };
    sync.published = Some(next);
    sync.epoch = session.project_epoch;
    match patch {
        Some(ops) => {
            let view = session.view();
            let _ = app.emit(
                SESSION_PATCH,
                crate::session_sync::Patch {
                    base: rev - 1,
                    rev,
                    flags: crate::session_sync::Flags {
                        file_path: view.file_path,
                        can_undo: view.can_undo,
                        can_redo: view.can_redo,
                        dirty: view.dirty,
                    },
                    ops,
                },
            );
        }
        None => {
            let _ = app.emit(SESSION_CHANGED, session.view_at(rev));
        }
    }
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
