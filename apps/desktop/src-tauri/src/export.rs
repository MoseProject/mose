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
use tauri::{AppHandle, Emitter, Runtime};

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
    /// Once the job has run: the encoder it used and how long it took against the length of the
    /// video, so a slow export can say whether it ran on the GPU or the CPU.
    pub report: Option<ExportReport>,
}
/// Where and how fast an export ran.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportReport {
    /// `h264_nvenc`, `libopenh264`, ... or `None` when none was opened.
    pub encoder: Option<String>,
    pub took_secs: f64,
    /// Length of the exported video; the speed is this over `took_secs`.
    pub video_secs: f64,
}
#[derive(Serialize, Deserialize)]
struct ExportSettings {
    #[serde(default)]
    gpu_encoder_preference: GpuEncoderPreference,
    #[serde(default = "one")]
    worker_count: usize,
    /// Master the export audio to the sequence's target loudness. Off leaves the mix as is.
    #[serde(default = "yes")]
    normalize_audio: bool,
}
fn yes() -> bool {
    true
}
fn one() -> usize {
    1
}
impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            gpu_encoder_preference: GpuEncoderPreference::Auto,
            worker_count: 1,
            normalize_audio: true,
        }
    }
}
/// What the export dialog asks beyond the sequence's own settings: a video bitrate and a
/// picture height (smaller or larger than the sequence's: a larger one scales the picture up). Both `None` keeps the source-matched defaults.
#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportOptions {
    pub bitrate_mbps: Option<f32>,
    pub max_height: Option<u32>,
    /// `Some(false)` exports this one without normalizing its loudness; `None` follows the app
    /// setting.
    pub normalize: Option<bool>,
}

impl ExportOptions {
    pub const MIN_BITRATE_MBPS: f32 = 0.5;
    pub const MAX_BITRATE_MBPS: f32 = 200.0;

    /// Rejects a bitrate or height the encoder cannot honour.
    pub fn validate(&self) -> Result<(), String> {
        if let Some(mbps) = self.bitrate_mbps {
            if !mbps.is_finite()
                || !(Self::MIN_BITRATE_MBPS..=Self::MAX_BITRATE_MBPS).contains(&mbps)
            {
                return Err("invalidBitrate".to_string());
            }
        }
        if self
            .max_height
            .is_some_and(|height| !(144..=4320).contains(&height))
        {
            return Err("invalidResolution".to_string());
        }
        Ok(())
    }

    /// `canvas` with the options applied. A lower height scales the width along (kept even for
    /// the encoder) and, unless a bitrate was asked for, the bitrate with the pixel count.
    pub fn apply(&self, canvas: Canvas) -> Canvas {
        let mut out = canvas;
        if let Some(height) = self.max_height {
            if height != canvas.height {
                let width = (u64::from(canvas.width) * u64::from(height) / u64::from(canvas.height))
                    as u32
                    & !1;
                out.width = width.max(2);
                out.height = height & !1;
                let ratio = f64::from(out.width) * f64::from(out.height)
                    / (f64::from(canvas.width) * f64::from(canvas.height));
                out.bit_rate_bps = ((canvas.bit_rate_bps as f64) * ratio) as i64;
            }
        }
        if let Some(mbps) = self.bitrate_mbps {
            out.bit_rate_bps = (f64::from(mbps) * 1_000_000.0) as i64;
        }
        out
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
    pub allow_copy: bool,
    pub normalize: bool,
}
#[cfg(test)]
pub fn prepare_export(project: &Project) -> Result<PreparedExport, String> {
    prepare_export_with(project, ExportOptions::default())
}
pub fn prepare_export_with(
    project: &Project,
    options: ExportOptions,
) -> Result<PreparedExport, String> {
    options.validate()?;
    let sequence = project.active_sequence();
    let (track_segments, canvas) =
        avcore::resolve_timeline_segments_multi(sequence, &project.media_library)
            .map_err(|e| e.to_string())?;
    let audio_segments = avcore::resolve_audio_segments(sequence, &project.media_library)
        .map_err(|e| e.to_string())?;
    let base = avcore::apply_export_aspect_ratio(canvas, sequence.export_settings.aspect_ratio);
    let canvas = options.apply(base);
    let unchanged = canvas.width == base.width
        && canvas.height == base.height
        && canvas.bit_rate_bps == base.bit_rate_bps;
    Ok(PreparedExport {
        text_segments: avcore::resolve_text_segments(sequence, canvas.width, canvas.height),
        shape_segments: avcore::resolve_shape_segments(sequence, canvas.width, canvas.height),
        privacy_blur_segments: avcore::resolve_privacy_blur_segments(sequence),
        track_segments,
        audio_segments,
        canvas,
        target_lufs: sequence.export_settings.target_lufs,
        allow_copy: unchanged,
        normalize: options.normalize.unwrap_or(true),
    })
}
pub struct ExportQueue {
    jobs: Mutex<Vec<ExportJob>>,
    controls: Mutex<HashMap<u64, Arc<RenderControl>>>,
    started: Mutex<HashMap<u64, Instant>>,
    reports: Mutex<HashMap<u64, ExportReport>>,
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
            reports: Mutex::new(HashMap::new()),
            config_dir,
            settings: Mutex::new(settings),
        }
    }
    pub fn snapshot(&self) -> Vec<ExportJobView> {
        let starts = self.started.lock().unwrap_or_else(|p| p.into_inner());
        let reports = self.reports.lock().unwrap_or_else(|p| p.into_inner());
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
                    report: reports.get(&j.id).cloned(),
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
    pub fn normalize_audio(&self) -> bool {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .normalize_audio
    }
    pub fn set_normalize_audio(&self, on: bool) -> Result<(), String> {
        self.settings
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .normalize_audio = on;
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
        avcore::write_atomic(&self.config_dir.join("export-settings.json"), &b)
            .map_err(|e| e.to_string())
    }
    pub fn enqueue<R: Runtime>(
        self: &Arc<Self>,
        app: AppHandle<R>,
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
            allow_copy: prepared.allow_copy,
            normalize: prepared.normalize,
            output_path: output.display().to_string(),
            status: ExportJobStatus::Queued,
        });
        drop(jobs);
        self.persist();
        self.emit(&app);
        self.pump(app);
        id
    }
    pub fn pause<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>, id: u64) {
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
    pub fn resume<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>, id: u64) {
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
    pub fn cancel<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>, id: u64) {
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
    pub fn remove<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>, id: u64) {
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
    pub fn match_loudness<R: Runtime>(&self, app: &AppHandle<R>, target: f32) {
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
    fn emit<R: Runtime>(&self, app: &AppHandle<R>) {
        let _ = app.emit(EXPORT_JOBS, self.snapshot());
    }
    fn pump<R: Runtime>(self: &Arc<Self>, app: AppHandle<R>) {
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
            let target_lufs = if self.normalize_audio() && job.normalize {
                job.target_lufs
            } else {
                avcore::NO_NORMALIZATION
            };
            std::thread::spawn(move || {
                let tracks = if job.track_segments.is_empty() {
                    vec![job.segments.clone()]
                } else {
                    job.track_segments.clone()
                };
                let mut last = 0;
                let started_at = Instant::now();
                let result = avcore::render_export_job_multi_with_options(
                    &tracks,
                    &job.audio_segments,
                    job.canvas,
                    &PathBuf::from(&job.output_path),
                    target_lufs,
                    gpu,
                    &job.text_segments,
                    &job.shape_segments,
                    &job.privacy_blur_segments,
                    job.allow_copy,
                    control.cancel_flag(),
                    |percent| {
                        if percent != last {
                            last = percent;
                            queue.progress(&worker_app, job.id, percent)
                        }
                        control.wait_if_paused();
                    },
                );
                // The export ran on this thread, so this is the encoder it opened.
                let report = ExportReport {
                    encoder: avcore::last_video_encoder(),
                    took_secs: started_at.elapsed().as_secs_f64(),
                    video_secs: tracks
                        .first()
                        .map_or(0.0, |track| track.iter().map(segment_output_secs).sum()),
                };
                queue.finish(worker_app, job.id, result, report)
            });
        }
    }
    fn progress<R: Runtime>(&self, app: &AppHandle<R>, id: u64, percent: u8) {
        self.transition(id, |s| match s {
            ExportJobStatus::Rendering { .. } => Some(ExportJobStatus::Rendering { percent }),
            ExportJobStatus::Paused { .. } => Some(ExportJobStatus::Paused { percent }),
            _ => None,
        });
        self.emit(app)
    }
    fn finish<R: Runtime>(
        self: &Arc<Self>,
        app: AppHandle<R>,
        id: u64,
        result: Result<RenderOutcome, avcore::RenderError>,
        report: ExportReport,
    ) {
        if matches!(result, Ok(RenderOutcome::Completed)) {
            self.reports
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(id, report);
        }
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

#[cfg(test)]
#[path = "export/export_test.rs"]
mod tests;

/// How long one clip segment plays in the export: its source range over its speed.
fn segment_output_secs(segment: &ClipSegment) -> f64 {
    let speed = if segment.speed_factor > 0.0 {
        f64::from(segment.speed_factor)
    } else {
        1.0
    };
    (segment.source_out_secs - segment.source_in_secs) / speed
}
