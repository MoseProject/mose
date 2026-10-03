//! Text-to-speech -- background-thread Piper synthesis, mirroring
//! `ui::App::spawn_generate_tts` exactly (same voice-config-next-to-model-path convention, same
//! post-generation import into the media library), no core changes needed.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::services::Services;
use crate::state::AppState;

pub const TTS_EVENT: &str = "tts-event";

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum TtsResult {
    Done,
    Failed { message: String },
}

/// Synthesizes `text` on a background thread via `prefs.tts_model_path` (the release-bundled
/// pt-BR voice, or a configured override) -- what the media library's "Texto-pra-fala" button
/// does. On success the resulting WAV is imported into the media library the same way any other
/// file drop is. Errors (already running, blank text, no voice configured) surface
/// synchronously; the outcome also arrives via [`TTS_EVENT`].
#[tauri::command]
pub fn generate_tts<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    let text = text.trim().to_string();
    let state = app.state::<AppState>();
    {
        let mut running = state
            .tts_generating
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if *running {
            return Err("a text-to-speech generation is already running".to_string());
        }
        *running = true;
    }
    let model_path = state.prefs().view.tts_model_path.clone();
    if text.is_empty() || model_path.is_empty() {
        *state
            .tts_generating
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = false;
        return Err("blank text or no voice model configured".to_string());
    }
    let model_path = PathBuf::from(model_path);
    let out_dir = tts_output_dir();
    let services = state.services.clone();

    let worker_app = app.clone();
    std::thread::spawn(move || {
        let result = generate_tts_one(services.as_ref(), &model_path, &text, &out_dir);
        {
            let state = worker_app.state::<AppState>();
            *state
                .tts_generating
                .lock()
                .unwrap_or_else(|p| p.into_inner()) = false;
        }
        let event = match result {
            Ok(wav_path) => {
                crate::commands::media::import_media(
                    worker_app.clone(),
                    vec![wav_path.display().to_string()],
                );
                TtsResult::Done
            }
            Err(message) => {
                tracing::error!(%message, "text-to-speech generation failed");
                TtsResult::Failed { message }
            }
        };
        let _ = worker_app.emit(TTS_EVENT, event);
    });
    Ok(())
}

fn tts_output_dir() -> PathBuf {
    avcore::app_paths::prefs_path()
        .parent()
        .map(|d| d.join("tts_output"))
        .unwrap_or_else(|| PathBuf::from("tts_output"))
}

fn generate_tts_one(
    services: &dyn Services,
    model_path: &Path,
    text: &str,
    out_dir: &Path,
) -> Result<PathBuf, String> {
    let config_path = PathBuf::from(format!("{}.json", model_path.display()));
    let config = avcore::load_voice_config(&config_path).map_err(|e| e.to_string())?;
    let samples = services
        .synthesize(model_path, &config, text)
        .map_err(|e| e.to_string())?;

    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let out_path = out_dir.join(format!("tts_{timestamp}.wav"));
    avcore::write_wav(&out_path, config.audio.sample_rate, &samples).map_err(|e| e.to_string())?;
    Ok(out_path)
}

#[cfg(test)]
#[path = "tts_test.rs"]
mod tests;
