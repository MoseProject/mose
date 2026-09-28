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

use std::sync::{Arc, Mutex, MutexGuard};

use avcore::timeline::{ClipFormatting, ClipInstance, TrackKind};
use avcore::undo::UndoStack;
use avcore::{Project, Sequence, SequenceExportSettings, Timeline};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

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
}

impl AppState {
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
pub fn publish(app: &AppHandle, session: &Session) {
    let state = app.state::<AppState>();
    state.preview.set_content(
        session.project.timeline().clone(),
        session.project.media_library.clone(),
    );
    if let Err(error) = app.emit(SESSION_CHANGED, session.view()) {
        tracing::warn!(%error, "failed to emit session-changed");
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
