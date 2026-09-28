// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Export queue for the Tauri shell: resolves the active sequence into the same segment
//! snapshot the egui shell queues (`ui::screens::queue`), then renders it on a plain
//! `std::thread` via `avcore::render_export_job_multi_with_audio`, reporting progress to the
//! webview through the `export-jobs` event.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use avcore::{
    AudioSegment, Canvas, ClipSegment, GpuEncoderPreference, PrivacyBlurSegment, Project,
    RenderOutcome, ShapeSegment, TextSegment,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// Event carrying the full `Vec<ExportJobView>` after any job change.
pub const EXPORT_JOBS: &str = "export-jobs";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExportStatus {
    Rendering,
    Done,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJobView {
    pub id: u64,
    pub title: String,
    pub output_path: String,
    pub status: ExportStatus,
    pub percent: u8,
    pub message: Option<String>,
}

/// Everything a render needs, snapshotted at queue time so later edits never change a job
/// already in flight -- same guarantee `avcore::ExportJob` documents.
pub struct PreparedExport {
    pub track_segments: Vec<Vec<ClipSegment>>,
    pub audio_segments: Vec<AudioSegment>,
    pub text_segments: Vec<TextSegment>,
    pub shape_segments: Vec<ShapeSegment>,
    pub privacy_blur_segments: Vec<PrivacyBlurSegment>,
    pub canvas: Canvas,
    pub target_lufs: f32,
}

/// Resolves the active sequence exactly as the egui queue screen does: segments, then the
/// sequence's aspect ratio applied to the canvas, then overlays sized for that canvas.
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

#[derive(Default)]
pub struct ExportQueue {
    jobs: Mutex<Vec<ExportJobView>>,
    cancels: Mutex<HashMap<u64, Arc<AtomicBool>>>,
}

impl ExportQueue {
    pub fn snapshot(&self) -> Vec<ExportJobView> {
        self.jobs
            .lock()
            .map(|jobs| jobs.clone())
            .unwrap_or_default()
    }

    pub fn cancel(&self, job_id: u64) {
        if let Some(flag) = self
            .cancels
            .lock()
            .ok()
            .and_then(|c| c.get(&job_id).cloned())
        {
            flag.store(true, Ordering::Release);
        }
    }

    /// Queues and immediately starts a render. Returns the new job id.
    pub fn start(
        self: &Arc<Self>,
        app: AppHandle,
        title: String,
        output: PathBuf,
        prepared: PreparedExport,
    ) -> u64 {
        let cancel = Arc::new(AtomicBool::new(false));
        let id = {
            let mut jobs = self.jobs.lock().unwrap_or_else(|p| p.into_inner());
            let id = jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
            jobs.push(ExportJobView {
                id,
                title,
                output_path: output.display().to_string(),
                status: ExportStatus::Rendering,
                percent: 0,
                message: None,
            });
            id
        };
        if let Ok(mut cancels) = self.cancels.lock() {
            cancels.insert(id, Arc::clone(&cancel));
        }
        self.emit(&app);

        let queue = Arc::clone(self);
        std::thread::spawn(move || {
            tracing::info!(job_id = id, output = %output.display(), "export render started");
            let mut last_percent = 0u8;
            let outcome = avcore::render_export_job_multi_with_audio(
                &prepared.track_segments,
                &prepared.audio_segments,
                prepared.canvas,
                &output,
                prepared.target_lufs,
                GpuEncoderPreference::Auto,
                &prepared.text_segments,
                &prepared.shape_segments,
                &prepared.privacy_blur_segments,
                &cancel,
                |percent| {
                    if percent != last_percent {
                        last_percent = percent;
                        queue.update(&app, id, |job| job.percent = percent);
                    }
                },
            );
            queue.update(&app, id, |job| match outcome {
                Ok(RenderOutcome::Completed) => {
                    job.status = ExportStatus::Done;
                    job.percent = 100;
                }
                Ok(RenderOutcome::Cancelled) => job.status = ExportStatus::Cancelled,
                Err(error) => {
                    tracing::warn!(job_id = id, %error, "export render failed");
                    job.status = ExportStatus::Failed;
                    job.message = Some(error.to_string());
                }
            });
            if let Ok(mut cancels) = queue.cancels.lock() {
                cancels.remove(&id);
            }
        });
        id
    }

    fn update(&self, app: &AppHandle, job_id: u64, change: impl FnOnce(&mut ExportJobView)) {
        if let Ok(mut jobs) = self.jobs.lock() {
            if let Some(job) = jobs.iter_mut().find(|j| j.id == job_id) {
                change(job);
            }
        }
        self.emit(app);
    }

    fn emit(&self, app: &AppHandle) {
        if let Err(error) = app.emit(EXPORT_JOBS, self.snapshot()) {
            tracing::warn!(%error, "failed to emit export-jobs");
        }
    }
}
