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

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use avcore::{
    AudioSegment, Canvas, ClipSegment, ExportJob, ExportJobStatus, RenderOutcome, ShapeSegment,
    TextSegment,
};
use eframe::egui;
use tracing::{debug, error, info};

use crate::components;
use crate::i18n::Text;
use crate::theme;

use super::{App, NestedSequenceEvent, RenderEvent};

pub(super) use avcore::RenderControl;

/// A ready-to-queue export whose output path collided with an existing file — held until the
/// user picks Overwrite, Rename, or Cancel in [`App::show_export_conflict_modal`]. Everything
/// [`App::queue_export`] needs is captured here so resolving the conflict is just a matter of
/// picking (or rewriting) `output_path` and calling it.
pub struct PendingExportConflict {
    pub title: String,
    pub track_segments: Vec<Vec<ClipSegment>>,
    pub audio_segments: Vec<AudioSegment>,
    pub text_segments: Vec<TextSegment>,
    pub shape_segments: Vec<ShapeSegment>,
    pub privacy_blur_segments: Vec<avcore::PrivacyBlurSegment>,
    pub canvas: Canvas,
    pub target_lufs: f32,
    pub output_path: PathBuf,
}

/// [`App::export_preview_cache`]'s stored key + result. Compared against the active sequence's
/// own `id`/`timeline.tracks` and `media_library` — not just the whole [`avcore::project::
/// Sequence`] active tab (whose `export_settings`/`name` [`avcore::resolve_timeline_segments_multi`]/
/// [`avcore::resolve_audio_segments`] never read, so comparing those would invalidate the cache
/// on an unrelated edit) and not `timeline.playhead_secs` (which changes continuously during
/// scrubbing/playback and would defeat the cache exactly when it matters most) — plus every
/// *other* sequence too (`sequences`), since a compound clip's rendered content depends on
/// whatever nested `Sequence`'s own timeline it points at, which `tracks`/`media_library` alone
/// can't see edits to. `result` holds `Err(())` rather than the real [`avcore::RenderError`] —
/// the only caller only ever checks `is_ok()`, and `RenderError::ReplaceOutput`'s
/// `std::io::Error` field isn't `Clone`, which a cached-and-returned `Result` needs to be.
pub(super) struct ExportPreviewCache {
    sequence_id: u64,
    tracks: Vec<avcore::timeline::Track>,
    media_library: Vec<avcore::MediaAsset>,
    sequences: Vec<avcore::project::Sequence>,
    result: Result<(Vec<Vec<ClipSegment>>, Vec<AudioSegment>, Canvas), ()>,
}

impl App {
    /// Shows the Overwrite/Rename/Cancel modal when [`App::pending_export_conflict`] is
    /// `Some` — the output path a queued export was about to use already exists on disk.
    /// Overwrite queues it as-is; Rename picks the first free `name (2).mp4`-style sibling via
    /// [`next_available_path`] and queues that instead; Cancel (or Escape) drops the job.
    pub(super) fn show_export_conflict_modal(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_export_conflict.as_ref() else {
            return;
        };
        let locale = self.locale;
        let filename = pending
            .output_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| pending.output_path.display().to_string());
        let renamed_preview = next_available_path(&pending.output_path);
        let renamed_filename = renamed_preview
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| renamed_preview.display().to_string());

        let modal = egui::Modal::new(egui::Id::new("export_conflict_modal"));
        let mut choice: Option<bool> = None; // Some(true) = overwrite, Some(false) = rename
        let mut cancelled = false;
        let response = modal.show(ctx, |ui| {
            ui.set_width(380.0);
            components::modal_title(ui, Text::ExportFileExistsTitle.tr(locale));
            ui.add_space(8.0);
            ui.label(
                Text::ExportFileExistsBody
                    .tr(locale)
                    .replace("{name}", &filename),
            );
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    Text::ExportFileExistsRenamedTo
                        .tr(locale)
                        .replace("{name}", &renamed_filename),
                )
                .size(11.0)
                .color(theme::TEXT_MUTED),
            );
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                cancelled = true;
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if components::primary_button(ui, Text::ExportFileExistsOverwrite.tr(locale))
                    .clicked()
                {
                    choice = Some(true);
                }
                if ui.button(Text::ExportFileExistsRename.tr(locale)).clicked() {
                    choice = Some(false);
                }
                if ui.button(Text::CancelJob.tr(locale)).clicked() {
                    cancelled = true;
                }
            });
        });
        if response.should_close() || cancelled {
            self.pending_export_conflict = None;
            return;
        }
        if let Some(overwrite) = choice {
            let pending = self.pending_export_conflict.take().unwrap();
            let output_path = if overwrite {
                pending.output_path
            } else {
                next_available_path(&pending.output_path)
            };
            self.queue_export(
                pending.title,
                pending.track_segments,
                pending.audio_segments,
                pending.text_segments,
                pending.shape_segments,
                pending.privacy_blur_segments,
                pending.canvas,
                pending.target_lufs,
                output_path.display().to_string(),
            );
        }
    }

    /// Resolves the active sequence's visible video tracks + media library into export segments
    /// (track segments, audio segments, canvas) — what the Fila (export queue) screen's header
    /// needs every frame just to show a file-size estimate and gate the "Adicionar exportação"
    /// button. [`avcore::resolve_timeline_segments_multi`]/[`avcore::resolve_audio_segments`]
    /// rebuild every clip's filter-chain string and re-scan the whole media library from
    /// scratch, so recomputing this every UI frame is real wasted work while nothing on the
    /// timeline actually changed — [`App::export_preview_cache`] reuses the last result whenever
    /// the exact inputs that produced it (see [`ExportPreviewCache`]'s doc comment on what does
    /// and doesn't count) are still the same. See `architecture/performance-and-caching.md` §2.
    pub fn resolved_active_sequence_export_preview(
        &mut self,
    ) -> Result<(Vec<Vec<ClipSegment>>, Vec<AudioSegment>, Canvas), ()> {
        let project = self.active_project();
        let sequence_id = project.active_sequence().id;
        let tracks = project.active_sequence().timeline.tracks.clone();
        let media_library = project.media_library.clone();
        // Every sequence, not just the active one -- a compound clip's rendered content depends
        // on some *other* Sequence's own timeline, which `tracks`/`media_library` above can't
        // see edits to. Cloned wholesale for the equality check; still far cheaper than the
        // filter-chain-string recompute this cache exists to avoid in the first place (same
        // reasoning ExportPreviewCache's own doc comment already gives for tracks/media_library).
        let sequences = project.sequences.clone();

        if let Some(cache) = &self.export_preview_cache {
            if cache.sequence_id == sequence_id
                && cache.tracks == tracks
                && cache.media_library == media_library
                && cache.sequences == sequences
            {
                return cache.result.clone();
            }
        }

        let nested_assets = self.materialize_nested_sequences_for_active_sequence();
        let mut media_library_with_nested = media_library.clone();
        media_library_with_nested.extend(nested_assets);

        let sequence = self.active_project().active_sequence();
        let result = avcore::resolve_timeline_segments_multi(sequence, &media_library_with_nested)
            .and_then(|(track_segments, canvas)| {
                avcore::resolve_audio_segments(sequence, &media_library_with_nested)
                    .map(|audio_segments| (track_segments, audio_segments, canvas))
            })
            .map_err(|_| ());

        self.export_preview_cache = Some(ExportPreviewCache {
            sequence_id,
            tracks,
            media_library,
            sequences,
            result: result.clone(),
        });
        result
    }

    /// Returns the active sequence's compound-clip (nested sequence) synthetic assets — one
    /// [`avcore::MediaAsset`] per nested sequence reachable from its timeline — to merge into a
    /// `media_library` clone before resolving export segments, see `avcore::nested_sequence`'s
    /// own doc comment.
    ///
    /// The actual rendering runs on a background thread (dispatched here, applied by
    /// [`App::pump_nested_sequence_renders`]) rather than blocking this call — the one honest
    /// gap ROADMAP.md P4 item 35 flagged: materializing a nested sequence is a real FFmpeg
    /// re-encode, and doing it synchronously on the UI thread meant the first hit after any edit
    /// to a nested sequence froze the whole app for however long that encode took. This method
    /// instead always returns immediately: the last successfully-materialized result for the
    /// active sequence (empty before the very first render completes), while dispatching a fresh
    /// background render whenever the active sequence's `Project::sequences` snapshot has
    /// changed since the input that produced that cached result (comparing every sequence, not
    /// just the active one's own timeline, since a compound clip's rendered content depends on
    /// whatever *other* sequence it points at — same reasoning `ExportPreviewCache` already
    /// uses) and no render for this sequence id is already in flight. A materialization failure
    /// (a missing or cyclically-nested sequence) is logged and treated as "no nested clips
    /// resolved" — the same clip simply fails `RenderError::MissingAsset` downstream instead, a
    /// clearer error for the eventual caller than this method swallowing the whole resolution.
    pub(super) fn materialize_nested_sequences_for_active_sequence(
        &mut self,
    ) -> Vec<avcore::MediaAsset> {
        let project = self.active_project().clone();
        let sequence_id = project.active_sequence().id;
        let cached = self
            .nested_sequence_render_state
            .nested_sequence_last_result
            .get(&sequence_id)
            .cloned()
            .unwrap_or_default();

        let unchanged = self
            .nested_sequence_render_state
            .nested_sequence_last_input
            .get(&sequence_id)
            == Some(&project.sequences);
        if unchanged
            || self
                .nested_sequence_render_state
                .nested_sequence_rendering_ids
                .contains(&sequence_id)
        {
            return cached;
        }

        let timeline = project.active_sequence().timeline.clone();
        let has_nested_clips = timeline
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .any(|c| c.nested_sequence_id.is_some());
        if !has_nested_clips {
            // Latch the input even in the trivial case, so this cheap scan doesn't repeat every
            // single frame either.
            self.nested_sequence_render_state
                .nested_sequence_last_input
                .insert(sequence_id, project.sequences.clone());
            self.nested_sequence_render_state
                .nested_sequence_last_result
                .remove(&sequence_id);
            return Vec::new();
        }

        self.nested_sequence_render_state
            .nested_sequence_rendering_ids
            .insert(sequence_id);
        let cache_dir = avcore::nested_sequence::cache_dir_for_project(&project);
        let mut cache_snapshot = self.nested_sequence_render_cache.clone();
        let sequences_input = project.sequences.clone();
        let tx = self.nested_sequence_render_state.nested_sequence_tx.clone();
        std::thread::spawn(move || {
            let event = match avcore::nested_sequence::materialize_nested_sequences(
                &project,
                &timeline,
                &cache_dir,
                &mut cache_snapshot,
            ) {
                Ok(assets) => NestedSequenceEvent::Ready {
                    sequence_id,
                    cache: cache_snapshot,
                    assets: assets.into_values().collect(),
                    sequences_input,
                },
                Err(error) => NestedSequenceEvent::Failed {
                    sequence_id,
                    error: error.to_string(),
                    sequences_input,
                },
            };
            let _ = tx.send(event);
        });

        cached
    }

    /// Applies finished background nested-sequence materializations to
    /// `nested_sequence_render_cache`/`nested_sequence_render_state`. Called once per frame from
    /// [`eframe::App::ui`], same as [`App::pump_motion_tracking`].
    pub(super) fn pump_nested_sequence_renders(&mut self) {
        while let Ok(event) = self
            .nested_sequence_render_state
            .nested_sequence_rx
            .try_recv()
        {
            match event {
                NestedSequenceEvent::Ready {
                    sequence_id,
                    cache,
                    assets,
                    sequences_input,
                } => {
                    self.nested_sequence_render_cache = cache;
                    self.nested_sequence_render_state
                        .nested_sequence_last_result
                        .insert(sequence_id, assets);
                    self.nested_sequence_render_state
                        .nested_sequence_last_input
                        .insert(sequence_id, sequences_input);
                    self.nested_sequence_render_state
                        .nested_sequence_rendering_ids
                        .remove(&sequence_id);
                }
                NestedSequenceEvent::Failed {
                    sequence_id,
                    error,
                    sequences_input,
                } => {
                    tracing::warn!(error = %error, "failed to materialize nested sequence(s)");
                    // Latched on failure too (not just success) -- otherwise a persistently
                    // broken nested-sequence reference (a real cycle, a deleted sequence) would
                    // get redispatched to a fresh background thread every single frame forever,
                    // since "unchanged since last input" would never become true.
                    self.nested_sequence_render_state
                        .nested_sequence_last_input
                        .insert(sequence_id, sequences_input);
                    self.nested_sequence_render_state
                        .nested_sequence_rendering_ids
                        .remove(&sequence_id);
                }
            }
        }
    }

    /// Appends a new `Queued` job — what "Adicionar exportação" does, given video, audio, text,
    /// and shape segments already resolved from the active sequence so this job renders the
    /// timeline as it was at the moment it entered the queue, not whatever is edited later.
    /// Picked up by [`App::pump_export_queue`] once a worker slot ([`App::queue_workers`])
    /// frees up.
    pub fn queue_export(
        &mut self,
        title: String,
        track_segments: Vec<Vec<avcore::ClipSegment>>,
        audio_segments: Vec<avcore::AudioSegment>,
        text_segments: Vec<avcore::TextSegment>,
        shape_segments: Vec<avcore::ShapeSegment>,
        privacy_blur_segments: Vec<avcore::PrivacyBlurSegment>,
        canvas: avcore::Canvas,
        target_lufs: f32,
        output_path: String,
    ) {
        let id = self.export_jobs.iter().map(|j| j.id).max().unwrap_or(0) + 1;
        info!(job_id = id, title = %title, output = %output_path, "export job queued");
        // `segments` is kept as the first track for backwards-compatible JSON; `track_segments`
        // carries the full multi-track snapshot that the render worker actually uses.
        let segments = track_segments.first().cloned().unwrap_or_default();
        self.export_jobs.push(ExportJob {
            id,
            title,
            segments,
            text_segments,
            shape_segments,
            privacy_blur_segments,
            track_segments,
            audio_segments,
            canvas,
            target_lufs,
            output_path,
            status: ExportJobStatus::Queued,
        });
        save_queue(&self.export_jobs);
    }

    /// Sets `target_lufs` on every `Queued` job — what the Fila screen's "match loudness
    /// across the batch" buttons do (`ROADMAP.md` P2 item 12, "D3 — series-level loudness
    /// consistency": episode 1 and episode 5 of a series shouldn't sound mismatched back to
    /// back just because their sequences had different export defaults when queued). Only
    /// `Queued` jobs are touched — a `Rendering`/`Paused` job's render worker already captured
    /// its own `target_lufs` at dispatch time ([`App::pump_export_queue`]), so changing the
    /// field on the `ExportJob` afterward wouldn't affect an already-started render anyway; a
    /// `Done`/`Failed` job is finished, changing it would just be misleading.
    pub fn match_loudness_across_queued_jobs(&mut self, target_lufs: f32) {
        for job in &mut self.export_jobs {
            if job.status == ExportJobStatus::Queued {
                job.target_lufs = target_lufs;
            }
        }
        save_queue(&self.export_jobs);
    }

    /// Stops a job: kills its `ffmpeg` process if it's actively rendering, or just removes it
    /// from the list if it hadn't started yet. There's no such thing as cancelling a `Done`/
    /// `Failed` job — callers only wire this to the buttons where it's meaningful.
    pub fn cancel_export_job(&mut self, job_id: u64) {
        match self.active_renders.get(&job_id) {
            Some(control) => control.cancel(),
            None => self.export_jobs.retain(|j| j.id != job_id),
        }
        save_queue(&self.export_jobs);
    }

    /// Pauses a queued job before it starts, or requests a cooperative pause for an active
    /// render at its next frame-progress checkpoint. Completed and failed jobs are unchanged.
    pub fn pause_export_job(&mut self, job_id: u64) {
        let Some(job) = self.export_jobs.iter_mut().find(|job| job.id == job_id) else {
            return;
        };
        let percent = match job.status {
            ExportJobStatus::Queued => 0,
            ExportJobStatus::Rendering { percent } => percent,
            _ => return,
        };
        if let Some(control) = self.active_renders.get(&job_id) {
            control.pause();
        }
        job.status = ExportJobStatus::Paused { percent };
        save_queue(&self.export_jobs);
    }

    /// Resumes an active paused worker, or moves a persisted/pre-start paused job back to the
    /// queue so [`App::pump_export_queue`] can dispatch it normally.
    pub fn resume_export_job(&mut self, job_id: u64) {
        let Some(job) = self.export_jobs.iter_mut().find(|job| job.id == job_id) else {
            return;
        };
        let ExportJobStatus::Paused { percent } = job.status else {
            return;
        };
        if let Some(control) = self.active_renders.get(&job_id) {
            control.resume();
            job.status = ExportJobStatus::Rendering { percent };
        } else {
            job.status = ExportJobStatus::Queued;
        }
        save_queue(&self.export_jobs);
    }

    /// Applies events from render worker threads to `export_jobs`, then — if there's a free
    /// worker slot under `queue_workers` — dispatches the next `Queued` job to a new
    /// background thread. Called once per frame; this is the entire "background export
    /// queue" from the execution plan's Fase 4.
    pub(super) fn pump_export_queue(&mut self) {
        while let Ok(event) = self.render_rx.try_recv() {
            match event {
                RenderEvent::Progress { job_id, percent } => {
                    if let Some(job) = self.export_jobs.iter_mut().find(|j| j.id == job_id) {
                        match &mut job.status {
                            ExportJobStatus::Rendering { percent: current }
                            | ExportJobStatus::Paused { percent: current } => {
                                *current = percent;
                            }
                            _ => {}
                        }
                    }
                }
                RenderEvent::Done {
                    job_id,
                    duration_ms,
                    output_duration_secs,
                } => {
                    if let Some(job) = self.export_jobs.iter_mut().find(|j| j.id == job_id) {
                        info!(job_id, output = %job.output_path, "export job completed");
                        job.status = ExportJobStatus::Done;
                    }
                    self.active_renders.remove(&job_id);
                    self.export_job_started_at.remove(&job_id);
                    save_queue(&self.export_jobs);
                    self.record_telemetry(avcore::TelemetryEvent::ExportCompleted {
                        duration_ms,
                        output_duration_secs,
                        success: true,
                    });
                }
                RenderEvent::Failed {
                    job_id,
                    message,
                    duration_ms,
                    output_duration_secs,
                } => {
                    if let Some(job) = self.export_jobs.iter_mut().find(|j| j.id == job_id) {
                        error!(job_id, output = %job.output_path, error = %message, "export job failed");
                        job.status = ExportJobStatus::Failed {
                            message: message.clone(),
                        };
                    }
                    self.active_renders.remove(&job_id);
                    self.export_job_started_at.remove(&job_id);
                    save_queue(&self.export_jobs);
                    self.record_telemetry(avcore::TelemetryEvent::ExportCompleted {
                        duration_ms,
                        output_duration_secs,
                        success: false,
                    });
                    self.record_telemetry(avcore::TelemetryEvent::Error {
                        context: "export".to_string(),
                        message,
                    });
                    self.report_error(
                        avcore::ErrorCode::ExportEncode,
                        avcore::ErrorSeverity::Error,
                        avcore::Operation::Export,
                        avcore::RecoveryOutcome::Aborted,
                        false,
                    );
                }
                RenderEvent::Cancelled { job_id } => {
                    debug!(job_id, "export job cancelled");
                    self.export_jobs.retain(|j| j.id != job_id);
                    self.active_renders.remove(&job_id);
                    self.export_job_started_at.remove(&job_id);
                    save_queue(&self.export_jobs);
                }
            }
        }

        if self.active_renders.len() >= self.prefs.export_workers as usize {
            return;
        }
        let Some(job) = self
            .export_jobs
            .iter_mut()
            .find(|j| j.status == ExportJobStatus::Queued)
        else {
            return;
        };

        let job_id = job.id;
        let text_segments = job.text_segments.clone();
        let shape_segments = job.shape_segments.clone();
        let privacy_blur_segments = job.privacy_blur_segments.clone();
        let audio_segments = job.audio_segments.clone();
        // Prefer multi-track snapshot; fall back to legacy single-track `segments` field for
        // jobs persisted before multi-track support was added.
        let track_segments: Vec<Vec<avcore::ClipSegment>> = if !job.track_segments.is_empty() {
            job.track_segments.clone()
        } else {
            vec![job.segments.clone()]
        };
        let canvas = job.canvas;
        let output_path = PathBuf::from(&job.output_path);
        let target_lufs = job.target_lufs;
        let gpu_encoder = self.prefs.gpu_encoder;
        job.status = ExportJobStatus::Rendering { percent: 0 };
        self.export_job_started_at.insert(job_id, Instant::now());

        // The exported timeline's own length (footage time, not encode wall-clock time) —
        // track 0 defines a multi-track export's overall duration, same as
        // `resolve_timeline_segments`'s `total_duration_secs`, just re-derived here from the
        // already-resolved `ClipSegment`s rather than the original `ClipInstance`s.
        let output_duration_secs: f64 = track_segments
            .first()
            .map(|track| {
                track
                    .iter()
                    .map(|seg| {
                        (seg.source_out_secs - seg.source_in_secs)
                            / (seg.speed_factor as f64).max(0.0001)
                    })
                    .sum()
            })
            .unwrap_or(0.0);

        let control = Arc::new(RenderControl::new());
        self.active_renders.insert(job_id, Arc::clone(&control));

        info!(job_id, output = %output_path.display(), "export render worker dispatched");
        let tx = self.render_tx.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let outcome = avcore::render_export_job_multi_with_audio(
                &track_segments,
                &audio_segments,
                canvas,
                &output_path,
                target_lufs,
                gpu_encoder,
                &text_segments,
                &shape_segments,
                &privacy_blur_segments,
                control.cancel_flag(),
                |percent| {
                    let _ = tx.send(RenderEvent::Progress { job_id, percent });
                    control.wait_if_paused();
                },
            );
            let duration_ms = started.elapsed().as_millis() as u64;

            let event = match outcome {
                Ok(RenderOutcome::Completed) => RenderEvent::Done {
                    job_id,
                    duration_ms,
                    output_duration_secs,
                },
                Ok(RenderOutcome::Cancelled) => RenderEvent::Cancelled { job_id },
                Err(e) => RenderEvent::Failed {
                    job_id,
                    message: e.to_string(),
                    duration_ms,
                    output_duration_secs,
                },
            };
            let _ = tx.send(event);
        });
    }
}

/// Finds the first `<stem> (2)<ext>`, `<stem> (3)<ext>`, ... sibling of `path` that doesn't
/// already exist. `path` itself is assumed to exist (that's why the caller is renaming).
pub(super) use avcore::next_available_path;

/// Returns the platform-appropriate path for the compressed oca export queue, next to
/// `prefs.oc`.
fn queue_path() -> std::path::PathBuf {
    super::prefs_path()
        .parent()
        .map(avcore::export_queue_path)
        .unwrap_or_else(|| PathBuf::from("queue.ocqueue"))
}

/// Returns the old JSON queue path used before `.ocqueue`. It is read only when the binary file
/// does not exist, then removed after a successful one-time migration.
fn legacy_queue_path() -> std::path::PathBuf {
    super::prefs_path()
        .parent()
        .map(|d| d.join("queue.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("queue.json"))
}

/// Saves `jobs` to `queue.ocqueue`. Queue mutations are infrequent and the metadata is tiny,
/// so this completes synchronously: unlike the old detached writer thread, a newer snapshot
/// cannot be overwritten by an older thread that happens to finish later. The temporary file
/// is created beside the destination and atomically persisted over it.
pub(super) fn save_queue(jobs: &[ExportJob]) {
    let path = queue_path();
    if let Err(storage_error) = save_queue_to_path(jobs, &path) {
        error!(path = %path.display(), error = %storage_error, "failed to save export queue");
    }
}

pub(super) fn save_queue_to_path(
    jobs: &[ExportJob],
    path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    avcore::save_export_queue_to_path(jobs, path).map_err(|error| error.into())
}

pub(super) fn persisted_queue_snapshot(jobs: &[ExportJob]) -> Vec<ExportJob> {
    avcore::persisted_export_queue_snapshot(jobs)
}

/// Loads `queue.ocqueue`, falling back to a one-time migration from the old `queue.json` only
/// when the binary file does not exist. A corrupt binary file never silently resurrects an
/// older JSON snapshot. `Rendering` jobs are reset to `Queued`; `Paused` jobs stay paused at
/// 0% until explicitly resumed, because workers and partial output do not survive the process.
pub(super) fn load_queue() -> Vec<ExportJob> {
    load_queue_from_paths(&queue_path(), &legacy_queue_path())
}

pub(super) fn load_queue_from_paths(path: &Path, legacy_path: &Path) -> Vec<ExportJob> {
    match std::fs::read(path) {
        Ok(_) => match avcore::load_export_queue_from_path(path) {
            Ok(jobs) => jobs,
            Err(storage_error) => {
                error!(path = %path.display(), error = %storage_error, "failed to load export queue");
                Vec::new()
            }
        },
        Err(read_error) if read_error.kind() == std::io::ErrorKind::NotFound => {
            migrate_legacy_queue(path, legacy_path)
        }
        Err(read_error) => {
            error!(path = %path.display(), error = %read_error, "failed to read export queue");
            Vec::new()
        }
    }
}

fn migrate_legacy_queue(path: &Path, legacy_path: &Path) -> Vec<ExportJob> {
    let Ok(bytes) = std::fs::read(legacy_path) else {
        return Vec::new();
    };
    let Ok(jobs) = serde_json::from_slice::<Vec<ExportJob>>(&bytes) else {
        error!(path = %legacy_path.display(), "failed to parse legacy export queue");
        return Vec::new();
    };
    let jobs = normalize_loaded_queue(jobs);
    match save_queue_to_path(&jobs, path) {
        Ok(()) => {
            if let Err(remove_error) = std::fs::remove_file(legacy_path) {
                error!(path = %legacy_path.display(), error = %remove_error, "failed to remove migrated legacy export queue");
            }
            info!(legacy = %legacy_path.display(), path = %path.display(), "export queue migrated to .ocqueue");
        }
        Err(storage_error) => {
            error!(path = %path.display(), error = %storage_error, "failed to migrate export queue");
        }
    }
    jobs
}

pub(super) fn normalize_loaded_queue(mut jobs: Vec<ExportJob>) -> Vec<ExportJob> {
    avcore::normalize_loaded_export_queue(jobs)
}

#[cfg(test)]
#[path = "export/export_test.rs"]
mod tests;
