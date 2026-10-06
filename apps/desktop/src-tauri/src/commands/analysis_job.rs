//! Admission and input identity shared by background analyses.
use crate::state::{AppState, Session};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{AppHandle, Manager, Runtime};

pub(super) struct AnalysisPermit(Arc<AtomicBool>, Option<Box<dyn FnOnce() + Send>>);
impl AnalysisPermit {
    pub fn acquire<R: Runtime>(app: &AppHandle<R>) -> Result<Self, String> {
        let state = app.state::<AppState>();
        let flag = Arc::clone(&state.analysis_running);
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| "analysisBusy".to_string())?;
        let app = app.clone();
        Ok(Self(
            flag,
            Some(Box::new(move || {
                let state = app.state::<AppState>();
                *crate::state::lock_or_recover(&state.scene_cut_detection_clip_id) = None;
                *crate::state::lock_or_recover(&state.motion_tracking_clip_id) = None;
                *crate::state::lock_or_recover(&state.auto_reframing_clip_id) = None;
                *crate::state::lock_or_recover(&state.matte_generating_clip_id) = None;
                *crate::state::lock_or_recover(&state.privacy_blur_generating_clip_id) = None;
                *crate::state::lock_or_recover(&state.transcribing_asset_id) = None;
            })),
        ))
    }
}
impl Drop for AnalysisPermit {
    fn drop(&mut self) {
        if let Some(cleanup) = self.1.take() {
            cleanup();
        }
        self.0.store(false, Ordering::Release);
    }
}

pub(super) struct AnalysisToken {
    epoch: u64,
    sequence_id: u64,
    clip: Option<avcore::timeline::ClipInstance>,
    asset_id: u64,
    source: Option<PathBuf>,
    revision: Option<(u64, std::time::SystemTime)>,
    resolution: Option<(u32, u32)>,
    pub mask_dir: PathBuf,
    settings: avcore::project::SequenceExportSettings,
    pub playhead: f64,
    pub transcript_dir: PathBuf,
}
impl AnalysisToken {
    pub fn capture(state: &AppState, clip_id: Option<u64>, asset_id: Option<u64>) -> Self {
        let session = state.session();
        let project = &session.project;
        let clip = clip_id.and_then(|id| {
            project
                .timeline()
                .tracks
                .iter()
                .flat_map(|t| &t.clips)
                .find(|c| c.id == id)
                .cloned()
        });
        let asset_id = asset_id
            .or_else(|| clip.as_ref().map(|c| c.asset_id))
            .unwrap_or(0);
        let mut token = Self {
            revision: None,
            resolution: project
                .media_library
                .iter()
                .find(|a| a.id == asset_id)
                .and_then(|a| a.resolution),
            mask_dir: avcore::background_removal::mask_cache_dir_for_project(project),
            epoch: session.project_epoch,
            sequence_id: project.active_sequence().id,
            source: project
                .media_library
                .iter()
                .find(|a| a.id == asset_id)
                .map(|a| a.source_path.clone()),
            clip,
            asset_id,
            settings: project.active_sequence().export_settings,
            playhead: project.timeline().playhead_secs,
            transcript_dir: avcore::transcript_cache_dir_for_project(project),
        };
        drop(session);
        token.revision = token.source.as_deref().and_then(source_revision);
        token
    }
    pub fn clip(&self) -> Option<avcore::timeline::ClipInstance> {
        self.clip.clone()
    }
    pub fn source(&self) -> Option<PathBuf> {
        self.source.clone()
    }
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.resolution
    }
    pub fn target_dims(&self, source: (u32, u32)) -> (u32, u32) {
        self.settings.aspect_ratio.dims_or(source)
    }

    fn matches(&self, session: &Session) -> bool {
        let project = &session.project;
        self.source.as_deref().and_then(source_revision) == self.revision
            && session.project_epoch == self.epoch
            && project.active_sequence().id == self.sequence_id
            && project.active_sequence().export_settings == self.settings
            && project
                .media_library
                .iter()
                .find(|a| a.id == self.asset_id)
                .map(|a| &a.source_path)
                == self.source.as_ref()
            && project
                .media_library
                .iter()
                .find(|a| a.id == self.asset_id)
                .and_then(|a| a.resolution)
                == self.resolution
            && self.clip.as_ref().is_none_or(|expected| {
                project
                    .timeline()
                    .tracks
                    .iter()
                    .flat_map(|t| &t.clips)
                    .find(|c| c.id == expected.id)
                    == Some(expected)
            })
    }
    pub fn apply<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        edit: impl FnOnce(&mut avcore::Project),
    ) -> Result<(), String> {
        super::with_session(app, |session| {
            if !self.matches(session) {
                return Err("analysis inputs changed; result discarded".into());
            }
            let before = session.project.active_sequence().clone();
            edit(&mut session.project);
            session.undo.push(before);
            session.dirty = true;
            Ok(())
        })
    }
    pub fn is_current<R: Runtime>(&self, app: &AppHandle<R>) -> bool {
        self.matches(&app.state::<AppState>().session())
    }
}

fn source_revision(source: &std::path::Path) -> Option<(u64, std::time::SystemTime)> {
    let metadata = std::fs::metadata(source).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

#[cfg(test)]
#[path = "analysis_job_test.rs"]
mod tests;
