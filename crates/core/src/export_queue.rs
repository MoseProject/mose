//! UI-independent export queue persistence and cooperative render controls.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};

use crate::{from_ocqueue_bytes, to_ocqueue_bytes, ExportJob, ExportJobStatus};

/// Cooperative controls shared by a queue and one render worker.
pub struct RenderControl {
    cancel: AtomicBool,
    paused: AtomicBool,
    wait_lock: Mutex<()>,
    wake: Condvar,
}

impl Default for RenderControl {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderControl {
    pub fn new() -> Self {
        Self {
            cancel: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            wait_lock: Mutex::new(()),
            wake: Condvar::new(),
        }
    }

    fn lock_wait_state(&self) -> MutexGuard<'_, ()> {
        self.wait_lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn pause(&self) {
        let _guard = self.lock_wait_state();
        self.paused.store(true, Ordering::Release);
    }

    pub fn resume(&self) {
        let _guard = self.lock_wait_state();
        self.paused.store(false, Ordering::Release);
        self.wake.notify_all();
    }

    pub fn cancel(&self) {
        let _guard = self.lock_wait_state();
        self.cancel.store(true, Ordering::Release);
        self.paused.store(false, Ordering::Release);
        self.wake.notify_all();
    }

    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }

    /// Waits until resumed or cancelled. Returns false if cancellation was requested.
    pub fn wait_if_paused(&self) -> bool {
        let mut guard = self.lock_wait_state();
        while self.paused.load(Ordering::Acquire) && !self.cancel.load(Ordering::Acquire) {
            guard = self
                .wake
                .wait(guard)
                .unwrap_or_else(|poison| poison.into_inner());
        }
        !self.cancel.load(Ordering::Acquire)
    }
}

/// Finds the first `<stem> (2)<ext>`, `<stem> (3)<ext>`, ... non-existing sibling.
pub fn next_available_path(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy())
        .unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy());
    for number in 2.. {
        let name = match &ext {
            Some(ext) => format!("{stem} ({number}).{ext}"),
            None => format!("{stem} ({number})"),
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("an unbounded range always yields a path")
}

/// Standard export queue location beneath an application configuration directory.
pub fn export_queue_path(config_dir: &Path) -> PathBuf {
    config_dir.join("queue.ocqueue")
}

/// Normalizes work that could not survive process exit into resumable queue state.
pub fn normalize_loaded_export_queue(mut jobs: Vec<ExportJob>) -> Vec<ExportJob> {
    for job in &mut jobs {
        match job.status {
            ExportJobStatus::Rendering { .. } => job.status = ExportJobStatus::Queued,
            ExportJobStatus::Paused { .. } => job.status = ExportJobStatus::Paused { percent: 0 },
            _ => {}
        }
    }
    jobs
}

/// Snapshot suitable for persistence: active work cannot be resumed after process exit.
pub fn persisted_export_queue_snapshot(jobs: &[ExportJob]) -> Vec<ExportJob> {
    normalize_loaded_export_queue(jobs.to_vec())
}

/// Atomically persists queue jobs using the shared `.ocqueue` framing.
pub fn save_export_queue_to_path(
    jobs: &[ExportJob],
    path: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bytes = to_ocqueue_bytes(&persisted_export_queue_snapshot(jobs))?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(&bytes)?;
    temporary.as_file().sync_all()?;
    temporary.persist(path)?;
    Ok(())
}

/// Loads a queue persisted with [`save_export_queue_to_path`]. Missing queues are empty.
pub fn load_export_queue_from_path(
    path: &Path,
) -> Result<Vec<ExportJob>, Box<dyn std::error::Error + Send + Sync>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(normalize_loaded_export_queue(from_ocqueue_bytes(&bytes)?)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(Box::new(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Canvas, ExportJob};

    fn job(status: ExportJobStatus) -> ExportJob {
        ExportJob {
            id: 1,
            title: String::new(),
            segments: Vec::new(),
            text_segments: Vec::new(),
            shape_segments: Vec::new(),
            privacy_blur_segments: Vec::new(),
            track_segments: Vec::new(),
            audio_segments: Vec::new(),
            canvas: Canvas {
                width: 1,
                height: 1,
                fps_num: 1,
                fps_den: 1,
                bit_rate_bps: 1,
            },
            target_lufs: -14.0,
            output_path: String::new(),
            status,
        }
    }

    #[test]
    fn normalizes_interrupted_work_to_resumable_state() {
        let jobs = normalize_loaded_export_queue(vec![
            job(ExportJobStatus::Rendering { percent: 42 }),
            job(ExportJobStatus::Paused { percent: 42 }),
        ]);
        assert_eq!(jobs[0].status, ExportJobStatus::Queued);
        assert_eq!(jobs[1].status, ExportJobStatus::Paused { percent: 0 });
    }
}
