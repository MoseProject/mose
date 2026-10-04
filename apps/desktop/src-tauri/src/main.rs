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
//! this crate owns the editing session, the Bush preview, import and export, all through
//! `avcore`. Runs alongside the egui shell (`crates/ui`) while screens migrate; see
//! `spec/architecture/tauri-migration.md`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autosave;
mod bush_adapter;
mod close_guard;
mod commands;
mod dialogs;
mod error_reporting;
mod export;
mod frames;
mod handler;
mod prefs;
mod preview_engine;
mod protocol;
mod services;
mod session_sync;
mod state;
mod telemetry;
#[cfg(test)]
mod testing;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use crate::export::ExportQueue;
use crate::preview_engine::PreviewEngine;
use crate::protocol::ThumbnailCache;
use crate::state::AppState;

fn main() {
    // Must run before any helper process starts so packaged tools win over system installs.
    avcore::configure_bundled_runtime();
    init_logging();
    install_panic_hook();

    tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, |ctx, request, responder| {
            protocol::handle_with(ctx.app_handle(), request.uri().path(), move |response| {
                responder.respond(response)
            });
        })
        .setup(|app| {
            let handle = app.handle().clone();
            app.manage(ThumbnailCache::default());
            let prefs = prefs::PrefsDocument::load();
            let preview = PreviewEngine::spawn(handle.clone());

            let locale_code = if prefs.view.locale == "en" {
                "en"
            } else {
                "pt-BR"
            };
            let error_reporter = error_reporting::seed_error_reporting(
                locale_code,
                prefs.view.error_reporting_consent,
            );
            let pending_crash_review = error_reporting::find_latest_unreviewed_crash(
                &platform_log_dir(),
                prefs.view.last_reviewed_crash_unix,
            );
            let telemetry_enabled = Arc::new(std::sync::atomic::AtomicBool::new(
                prefs.view.telemetry_enabled,
            ));
            telemetry::spawn(Arc::clone(&telemetry_enabled));

            app.manage(AppState::new(
                prefs,
                preview,
                Arc::new(ExportQueue::load(app.path().app_config_dir()?)),
                error_reporter,
                pending_crash_review,
                telemetry_enabled,
                Arc::new(services::SystemServices::new(handle.clone())),
            ));
            autosave::spawn(handle.clone());
            autosave::register_emergency_save(handle.clone());
            autosave::spawn_saver(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::CloseRequested { api, .. } => {
                    if close_guard::guard_close(window.app_handle()) {
                        api.prevent_close();
                    }
                }
                // Switching to another window is a good moment to put the work on disk.
                tauri::WindowEvent::Focused(false) => {
                    let app = window.app_handle().clone();
                    std::thread::spawn(move || {
                        autosave::flush_now(&app);
                    });
                }
                _ => {}
            }
        })
        .invoke_handler(handler::all_commands!())
        .build(tauri::generate_context!())
        .expect("error while building the oca desktop shell")
        .run(|app, event| {
            // However the app ends (the window, the tray, the system, `quit_app`), the last
            // changes are written before the process goes.
            if matches!(
                event,
                tauri::RunEvent::ExitRequested { .. } | tauri::RunEvent::Exit
            ) {
                autosave::flush_now(app);
            }
        });
}

/// Initialises the global tracing subscriber: a rolling daily file in the platform log dir
/// plus, in debug builds, a human-readable layer on stderr. Mirrors `ui::main.rs::init_logging`
/// exactly -- same log directory convention, same `OCA_LOG` filter override.
fn init_logging() {
    let log_dir = platform_log_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "oca.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    std::mem::forget(guard);

    let filter = tracing_subscriber::EnvFilter::try_from_env("OCA_LOG")
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(non_blocking);

    #[cfg(debug_assertions)]
    {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .pretty();
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(stderr_layer)
            .init();
    }
    #[cfg(not(debug_assertions))]
    {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .init();
    }

    tracing::info!(version = env!("CARGO_PKG_VERSION"), "oca desktop starting");
}

/// Platform-appropriate log directory -- mirrors `ui::main.rs::platform_log_dir` exactly, same
/// paths, so both shells' crash files and error-report queues share one directory.
fn platform_log_dir() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join("Library/Logs/oca");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return std::path::PathBuf::from(appdata).join("oca\\logs");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return std::path::PathBuf::from(home).join(".local/share/oca/logs");
        }
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("logs")))
        .unwrap_or_else(|| std::path::PathBuf::from("logs"))
}

/// Installs a panic hook that writes a `crash_<unix_seconds>.txt` file to the log directory
/// before calling the default hook -- mirrors `ui::main.rs::install_panic_hook` exactly (same
/// file format `error_reporting::parse_crash_report`/`find_latest_unreviewed_crash` read), so a
/// crash from either shell surfaces through the same post-crash review.
fn install_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("<unknown>");
        let location = info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "<unknown>".to_owned());
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let report = format!(
            "oca v{ver} crash report\ntimestamp (unix): {timestamp}\nlocation: {location}\nmessage: {msg}\n\nbacktrace:\n{backtrace}\n",
            ver = env!("CARGO_PKG_VERSION"),
        );
        let log_dir = platform_log_dir();
        let _ = std::fs::create_dir_all(&log_dir);
        let crash_path = log_dir.join(format!("crash_{timestamp}.txt"));
        let _ = std::fs::write(&crash_path, report.as_bytes());
        // The crash report is on disk; now try to keep the user's work too.
        autosave::run_emergency_save();
        default_hook(info);
    }));
}
