//! Persistent bounded export queue for the desktop shell.
use avcore::{
    AudioSegment, Canvas, ClipSegment, ExportJob, ExportJobStatus, GpuEncoderPreference,
    PrivacyBlurSegment, Project, RenderControl, RenderOutcome, ShapeSegment, TextSegment,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};
use tauri::{AppHandle, Emitter};

pub const EXPORT_JOBS: &str = "export-jobs";
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJobView {
    pub id: u64,
    pub title: String,
    pub output_path: String,
    pub status: ExportJobStatus,
    pub percent: u8,
    pub elapsed_secs: Option<f64>,
}
#[derive(Serialize, Deserialize)]
struct ExportSettings {
    #[serde(default)]
    gpu_encoder_preference: GpuEncoderPreference,
    #[serde(default = "one")]
    worker_count: usize,
}
fn one() -> usize {
    1
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            gpu_encoder_preference: GpuEncoderPreference::Auto,
            worker_count: 1,
        }
    }
}
pub struct PreparedExport {
    pub track_segments: Vec<Vec<ClipSegment>>,
    pub audio_segments: Vec<AudioSegment>,
    pub text_segments: Vec<TextSegment>,
    pub shape_segments: Vec<ShapeSegment>,
    pub privacy_blur_segments: Vec<PrivacyBlurSegment>,
    pub canvas: Canvas,
    pub target_lufs: f32,
}
pub fn prepare_export(project: &Project) -> Result<PreparedExport, String> {
    let sequence = project.active_sequence();
    let (track_segments, canvas) =
        avcore::resolve_timeline_segments_multi(sequence, &project.media_library)
            .map_err(|e| e.to_string())?;
    let audio_segments = avcore::resolve_audio_segments(sequence, &project.media_library)
        .map_err(|e| e.to_string())?;
    let canvas = avcore::apply_export_aspect_ratio(canvas, sequence.export_settings.aspect_ratio);
    Ok(PreparedExport {
        text_segments: avcore::resolve_text_segments(sequence, canvas.width, canvas.height),
        shape_segments: avcore::resolve_shape_segments(sequence, canvas.width, canvas.height),
        privacy_blur_segments: avcore::resolve_privacy_blur_segments(sequence),
        track_segments,
        audio_segments,
        canvas,
        target_lufs: sequence.export_settings.target_lufs,
    })
}
pub struct ExportQueue {
    jobs: Mutex<Vec<ExportJob>>,
    controls: Mutex<HashMap<u64, Arc<RenderControl>>>,
    started: Mutex<HashMap<u64, Instant>>,
    config_dir: PathBuf,
    settings: Mutex<ExportSettings>,
}
impl ExportQueue {
    pub fn load(config_dir: PathBuf) -> Self {
        let queue_path = avcore::export_queue_path(&config_dir);
        let jobs = avcore::load_export_queue_from_path(&queue_path).unwrap_or_else(|e| {
            tracing::warn!(%e,"failed to load export queue");
            Vec::new()
        });
        let settings = std::fs::read(config_dir.join("export-settings.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            jobs: Mutex::new(jobs),
            controls: Mutex::new(HashMap::new()),
            started: Mutex::new(HashMap::new()),
            config_dir,
            settings: Mutex::new(settings),
        }
    }
    pub fn snapshot(&self) -> Vec<ExportJobView> {
        let starts = self.started.lock().unwrap_or_else(|p| p.into_inner());
        self.jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .map(|j| {
                let percent = match &j.status {
                    ExportJobStatus::Rendering { percent }
                    | ExportJobStatus::Paused { percent } => *percent,
                    _ => 0,
                };
                ExportJobView {
                    id: j.id,
                    title: j.title.clone(),
                    output_path: j.output_path.clone(),
                    status: j.status.clone(),
                    percent,
                    elapsed_secs: starts.get(&j.id).map(|s| s.elapsed().as_secs_f64()),
                }
            })
            .collect()
    }
    pub fn gpu_encoder_preference(&self) -> GpuEncoderPreference {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .gpu_encoder_preference
    }
    pub fn set_gpu_encoder_preference(&self, p: GpuEncoderPreference) -> Result<(), String> {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .gpu_encoder_preference = p;
        self.save_settings()
    }
    pub fn set_worker_count(&self, n: usize) -> Result<(), String> {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .worker_count = n.clamp(1, 8);
        self.save_settings()
    }
    fn save_settings(&self) -> Result<(), String> {
        std::fs::create_dir_all(&self.config_dir).map_err(|e| e.to_string())?;
        let b = serde_json::to_vec(&*self.settings.lock().unwrap_or_else(|p| p.into_inner()))
            .map_err(|e| e.to_string())?;
        std::fs::write(self.config_dir.join("export-settings.json"), b).map_err(|e| e.to_string())
    }
    pub fn enqueue(
        self: &Arc<Self>,
        app: AppHandle,
        title: String,
        output: PathBuf,
        prepared: PreparedExport,
    ) -> u64 {
        let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        let id = jobs.iter().map(|j| j.id).max().unwrap_or(0).wrapping_add(1);
        jobs.push(ExportJob {
            id,
            title,
            segments: prepared.track_segments.first().cloned().unwrap_or_default(),
            text_segments: prepared.text_segments,
            shape_segments: prepared.shape_segments,
            privacy_blur_segments: prepared.privacy_blur_segments,
            track_segments: prepared.track_segments,
            audio_segments: prepared.audio_segments,
            canvas: prepared.canvas,
            target_lufs: prepared.target_lufs,
            output_path: output.display().to_string(),
            status: ExportJobStatus::Queued,
        });
        drop(jobs);
        self.persist();
        self.emit(&app);
        self.pump(app);
        id
    }
    pub fn pause(self: &Arc<Self>, app: AppHandle, id: u64) {
        self.transition(id, |s| match s {
            ExportJobStatus::Queued => Some(ExportJobStatus::Paused { percent: 0 }),
            ExportJobStatus::Rendering { percent } => {
                Some(ExportJobStatus::Paused { percent: *percent })
            }
            _ => None,
        });
        if let Some(c) = self
            .controls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
        {
            c.pause()
        }
        self.persist();
        self.emit(&app)
    }
    pub fn resume(self: &Arc<Self>, app: AppHandle, id: u64) {
        let c = self
            .controls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .cloned();
        self.transition(id, |s| match s {
            ExportJobStatus::Paused { percent } if c.is_some() => {
                Some(ExportJobStatus::Rendering { percent: *percent })
            }
            ExportJobStatus::Paused { .. } => Some(ExportJobStatus::Queued),
            _ => None,
        });
        if let Some(c) = c {
            c.resume()
        }
        self.persist();
        self.emit(&app);
        self.pump(app)
    }
    pub fn cancel(self: &Arc<Self>, app: AppHandle, id: u64) {
        if let Some(c) = self
            .controls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(&id)
            .cloned()
        {
            c.cancel()
        } else {
            self.jobs
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|j| j.id != id)
        }
        self.persist();
        self.emit(&app)
    }
    pub fn remove(self: &Arc<Self>, app: AppHandle, id: u64) {
        if !self
            .controls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .contains_key(&id)
        {
            self.jobs
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .retain(|j| j.id != id);
            self.persist();
            self.emit(&app)
        }
    }
    pub fn match_loudness(&self, app: &AppHandle, target: f32) {
        for j in self
            .jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter_mut()
        {
            if j.status == ExportJobStatus::Queued {
                j.target_lufs = target
            }
        }
        self.persist();
        self.emit(app)
    }
    fn transition(&self, id: u64, f: impl FnOnce(&ExportJobStatus) -> Option<ExportJobStatus>) {
        if let Some(j) = self
            .jobs
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter_mut()
            .find(|j| j.id == id)
        {
            if let Some(s) = f(&j.status) {
                j.status = s
            }
        }
    }
    fn persist(&self) {
        let jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(e) =
            avcore::save_export_queue_to_path(&jobs, &avcore::export_queue_path(&self.config_dir))
        {
            tracing::warn!(%e,"failed to save export queue")
        }
    }
    fn emit(&self, app: &AppHandle) {
        if let Err(e) = app.emit(EXPORT_JOBS, self.snapshot()) {
            tracing::warn!(%e,"failed to emit export jobs")
        }
    }
    fn pump(self: &Arc<Self>, app: AppHandle) {
        loop {
            if self
                .controls
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .len()
                >= self
                    .settings
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .worker_count
            {
                break;
            }
            let job = {
                let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
                let Some(j) = jobs
                    .iter_mut()
                    .find(|j| j.status == ExportJobStatus::Queued)
                else {
                    break;
                };
                j.status = ExportJobStatus::Rendering { percent: 0 };
                j.clone()
            };
            let control = Arc::new(RenderControl::new());
            self.controls
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(job.id, Arc::clone(&control));
            self.started
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(job.id, Instant::now());
            self.persist();
            self.emit(&app);
            let queue = Arc::clone(self);
            let worker_app = app.clone();
            let gpu = self.gpu_encoder_preference();
            std::thread::spawn(move || {
                let tracks = if job.track_segments.is_empty() {
                    vec![job.segments.clone()]
                } else {
                    job.track_segments.clone()
                };
                let mut last = 0;
                let result = avcore::render_export_job_multi_with_audio(
                    &tracks,
                    &job.audio_segments,
                    job.canvas,
                    &PathBuf::from(&job.output_path),
                    job.target_lufs,
                    gpu,
                    &job.text_segments,
                    &job.shape_segments,
                    &job.privacy_blur_segments,
                    control.cancel_flag(),
                    |percent| {
                        if percent != last {
                            last = percent;
                            queue.progress(&worker_app, job.id, percent)
                        }
                        control.wait_if_paused();
                    },
                );
                queue.finish(worker_app, job.id, result)
            });
        }
    }
    fn progress(&self, app: &AppHandle, id: u64, percent: u8) {
        self.transition(id, |s| match s {
            ExportJobStatus::Rendering { .. } => Some(ExportJobStatus::Rendering { percent }),
            ExportJobStatus::Paused { .. } => Some(ExportJobStatus::Paused { percent }),
            _ => None,
        });
        self.emit(app)
    }
    fn finish(
        self: &Arc<Self>,
        app: AppHandle,
        id: u64,
        result: Result<RenderOutcome, avcore::RenderError>,
    ) {
        let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
        match result {
            Ok(RenderOutcome::Completed) => {
                if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                    j.status = ExportJobStatus::Done
                }
            }
            Ok(RenderOutcome::Cancelled) => jobs.retain(|j| j.id != id),
            Err(e) => {
                if let Some(j) = jobs.iter_mut().find(|j| j.id == id) {
                    j.status = ExportJobStatus::Failed {
                        message: e.to_string(),
                    }
                }
            }
        }
        drop(jobs);
        self.controls
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        self.started
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&id);
        self.persist();
        self.emit(&app);
        self.pump(app)
    }
}
