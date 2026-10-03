//! The calls that leave the process or need model files: the GitHub release API, the in-place
//! self-update and restart, and the Piper voice. Commands reach them through
//! [`crate::state::AppState::services`], so tests can swap in a fake and exercise everything
//! around the call (single-flight guards, events, imports) without a network, an installed
//! update or a model. [`SystemServices`] only forwards, and is excluded from coverage with the
//! other process-boundary code (see `spec/architecture/test-coverage-plan.md`).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use avcore::{
    ApplyUpdateError, ApplyUpdateOutcome, LatestRelease, PiperVoiceConfig, TranscribeError,
    TranscribeOutcome, TtsError, UpdateCheckError,
};

pub trait Services: Send + Sync {
    fn fetch_latest_release(&self) -> Result<LatestRelease, UpdateCheckError>;
    fn apply_update(&self, version: &str) -> Result<ApplyUpdateOutcome, ApplyUpdateError>;
    fn restart_application(&self) -> Result<(), ApplyUpdateError>;
    /// Closes the app gracefully, once a restarted copy is running.
    fn exit(&self, code: i32);
    fn synthesize(
        &self,
        model_path: &Path,
        config: &PiperVoiceConfig,
        text: &str,
    ) -> Result<Vec<f32>, TtsError>;
    /// The Whisper model shipped with the app, when this install has one.
    fn bundled_whisper_model(&self) -> Option<PathBuf>;
    fn transcribe(
        &self,
        source: &Path,
        model: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> Result<TranscribeOutcome, TranscribeError>;
}

/// The real implementations.
pub struct SystemServices {
    app: tauri::AppHandle,
}

impl SystemServices {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl Services for SystemServices {
    fn fetch_latest_release(&self) -> Result<LatestRelease, UpdateCheckError> {
        avcore::fetch_latest_release()
    }

    fn apply_update(&self, version: &str) -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
        avcore::apply_update(version)
    }

    fn restart_application(&self) -> Result<(), ApplyUpdateError> {
        avcore::restart_application()
    }

    fn exit(&self, code: i32) {
        self.app.exit(code);
    }

    fn synthesize(
        &self,
        model_path: &Path,
        config: &PiperVoiceConfig,
        text: &str,
    ) -> Result<Vec<f32>, TtsError> {
        avcore::synthesize(model_path, config, text)
    }

    fn bundled_whisper_model(&self) -> Option<PathBuf> {
        avcore::bundled_resource_path(avcore::BundledResource::WhisperBase)
    }

    fn transcribe(
        &self,
        source: &Path,
        model: &Path,
        cancel: &Arc<AtomicBool>,
    ) -> Result<TranscribeOutcome, TranscribeError> {
        avcore::transcribe(source, model, None, cancel, |_percent| {})
    }
}
