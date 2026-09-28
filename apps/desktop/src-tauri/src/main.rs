// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! `desktop` -- the Tauri shell for oca. The webview (React/TypeScript in `../src`) renders;
//! this crate owns the editing session, GStreamer preview, import and export, all through
//! `avcore`. Runs alongside the egui shell (`crates/ui`) while screens migrate; see
//! `spec/architecture/tauri-migration.md`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autosave;
mod commands;
mod export;
mod frames;
mod prefs;
mod preview_engine;
mod protocol;
mod state;

use std::sync::{Arc, Mutex};

use tauri::Manager;

use crate::export::ExportQueue;
use crate::preview_engine::PreviewEngine;
use crate::protocol::ThumbnailCache;
use crate::state::{blank_project, AppState, Session};

fn main() {
    // Must run before GStreamer initializes so packaged runtimes win over system installs.
    avcore::configure_bundled_runtime();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_env("OCA_LOG")
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            protocol::handle(ctx.app_handle(), request.uri().path(), responder);
        })
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(ThumbnailCache::default());
            let prefs = prefs::PrefsDocument::load();
            let preview = PreviewEngine::spawn(handle.clone());
            preview.set_hardware_decode(prefs.view.preview_hardware_decode);
            app.manage(AppState {
                session: Mutex::new(Session::new(blank_project(
                    "Untitled".to_string(),
                    "Sequence 1".to_string(),
                ))),
                prefs: Mutex::new(prefs),
                preview,
                exports: Arc::new(ExportQueue::load(app.path().app_config_dir()?)),
                clip_clipboard: Mutex::new(None),
                formatting_clipboard: Mutex::new(None),
            });
            autosave::spawn(handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_session,
            commands::new_project,
            commands::open_project,
            commands::open_project_path,
            commands::save_project,
            commands::undo,
            commands::redo,
            commands::preview_seek,
            commands::preview_step,
            commands::preview_set_playing,
            commands::preview_set_hardware_decode,
            commands::set_export_settings,
            commands::export::pick_export_path,
            commands::export::start_export,
            commands::export::pause_export,
            commands::export::resume_export,
            commands::export::cancel_export,
            commands::export::remove_export,
            commands::export::match_export_loudness,
            commands::export::set_gpu_encoder_preference,
            commands::export::set_export_worker_count,
            commands::export::get_gpu_encoder_preference,
            commands::export::export_active_sequence_srt,
            commands::export::list_exports,
            commands::prefs::get_prefs,
            commands::prefs::set_prefs,
            commands::prefs::get_recent_projects,
            commands::prefs::remove_recent_project,
            commands::prefs::restore_autosave,
            commands::prefs::discard_autosave,
            commands::media::import_media_dialog,
            commands::media::import_media,
            commands::media::toggle_favorite,
            commands::markers::add_marker,
            commands::markers::remove_marker,
            commands::markers::update_marker,
            commands::sequences::add_sequence,
            commands::sequences::select_sequence,
            commands::sequences::duplicate_sequence,
            commands::sequences::rename_sequence,
            commands::sequences::remove_sequence,
            commands::sequences::move_sequence,
            commands::timeline::add_asset_to_timeline,
            commands::timeline::move_clip,
            commands::timeline::move_clip_group,
            commands::timeline::trim_clip,
            commands::timeline::roll_edit_clip,
            commands::timeline::roll_edit_from_start_edge,
            commands::timeline::slip_clip,
            commands::timeline::slide_clip,
            commands::timeline::split_at_playhead,
            commands::timeline::delete_clips,
            commands::timeline::update_clip,
            commands::timeline::add_track,
            commands::timeline::set_track_flags,
            commands::timeline::merge_clips_into_composite,
            commands::timeline::copy_clip,
            commands::timeline::cut_clip,
            commands::timeline::paste_clip,
            commands::timeline::copy_clip_formatting,
            commands::timeline::paste_clip_formatting,
            commands::overlays::overlay_catalog,
            commands::overlays::add_text_clip,
            commands::overlays::add_shape_clip,
            commands::overlays::move_overlay_clip,
            commands::overlays::trim_overlay_clip,
            commands::overlays::update_text_clip,
            commands::overlays::update_shape_clip,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the oca desktop shell");
}
