//! The calls that leave the process or need model files: the GitHub release API, the in-place
//! self-update and restart, and the Piper voice. Commands reach them through
//! [`crate::state::AppState::services`], so tests can swap in a fake and exercise everything
//! around the call (single-flight guards, events, imports) without a network, an installed
//! update or a model. [`SystemServices`] only forwards, and is excluded from coverage with the
//! other process-boundary code (see `spec/architecture/test-coverage-plan.md`).

use std::path::Path;

use avcore::{
    ApplyUpdateError, ApplyUpdateOutcome, LatestRelease, PiperVoiceConfig, TtsError,
    UpdateCheckError,
};

pub trait Services: Send + Sync {
    fn fetch_latest_release(&self) -> Result<LatestRelease, UpdateCheckError>;
    fn apply_update(&self, version: &str) -> Result<ApplyUpdateOutcome, ApplyUpdateError>;
    fn restart_application(&self) -> Result<(), ApplyUpdateError>;
    fn synthesize(
        &self,
        model_path: &Path,
        config: &PiperVoiceConfig,
        text: &str,
    ) -> Result<Vec<f32>, TtsError>;
}

/// The real implementations.
pub struct SystemServices;

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

    fn synthesize(
        &self,
        model_path: &Path,
        config: &PiperVoiceConfig,
        text: &str,
    ) -> Result<Vec<f32>, TtsError> {
        avcore::synthesize(model_path, config, text)
    }
}
