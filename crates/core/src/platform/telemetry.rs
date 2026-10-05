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

//! Structured runtime telemetry — `request.md`'s Fase 7 "Telemetria de runtime": local,
//! append-only JSON-lines records of usage events (import/export duration, preview frame
//! time, error events). Stays on-device only — nothing here ever leaves the machine or talks
//! to a network, matching `request.md`'s explicit "fica no dispositivo por padrão" wording —
//! `ui` decides *where* the file lives (see `App::telemetry_path`, next to the platform log
//! directory Fase 6's `tracing` file appender already writes to) and whether the user has
//! opted out; this module only knows how to append one record given an already-resolved path.
//!
//! **CPU/RAM resource sampling** ([`ResourceUsage`][TelemetryEvent::ResourceUsage],
//! [`ResourceSampler`]) is wired in via `sysinfo`. **GPU usage** ([`GpuUsage`]
//! [TelemetryEvent::GpuUsage], [`GpuSampler`]) is now also covered, NVIDIA-only, via the NVIDIA
//! Management Library (`nvml-wrapper`) — `sysinfo` itself has no cross-platform GPU reader.
//! `Nvml::init()` dynamically loads `libnvidia-ml.so`/`nvml.dll` at runtime, so a machine with
//! no NVIDIA GPU/driver at all (AMD, Intel-only, or no discrete GPU) just gets `Err` back —
//! never a build-time requirement, and never a panic. [`GpuSampler::new`] turns that `Err` into
//! `None`, the same "sampling this is unavailable here, skip it" contract `ui`'s telemetry
//! background thread already has to handle for every other best-effort sample.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use sysinfo::System;

/// Above this size, [`record_event`] rotates the file to a `.1`-suffixed backup (replacing any
/// previous one) before appending further — bounds total disk usage to roughly this size times
/// two (the live file plus one backup) for an install that runs for a very long time. Simple
/// size-based rotation rather than the daily rotation Fase 6's `tracing` log uses, since this
/// crate has no date/time dependency to compute calendar boundaries with, and a plain size cap
/// needs none.
const ROTATE_AT_BYTES: u64 = 10 * 1024 * 1024;

/// One structured usage or error event, appended as a single JSON-lines record by
/// [`record_event`]. `#[serde(tag = "event")]` makes each line self-describing without a
/// separate schema/wrapper type to keep in sync.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum TelemetryEvent {
    /// A file finished importing — `duration_ms` covers `App::spawn_import`'s whole
    /// background-thread pass for that file (probe, then loudness/proxy/waveform
    /// enrichment), not just one sub-step.
    ImportCompleted { duration_ms: u64 },
    /// An export job's render worker finished, successfully or not. `duration_ms` is
    /// wall-clock encode time; `output_duration_secs` is the exported timeline's own length —
    /// a different quantity (encoding speed vs. footage length), both useful to have side by
    /// side when looking at real-world render throughput later.
    ExportCompleted {
        duration_ms: u64,
        output_duration_secs: f64,
        success: bool,
    },
    /// A sampled preview frame time (milliseconds), taken periodically while the preview is
    /// playing — not a record of every single render frame, which would be far too high a
    /// volume for a plain JSON-lines file.
    PreviewFrameTime { frame_time_ms: f32 },
    /// A recoverable error already surfaced to the user or logged via `tracing::error!` —
    /// `context` is a short machine-readable tag (e.g. `"import"`, `"export"`), not a full
    /// sentence, so records group cleanly if this file is ever aggregated across sessions.
    Error { context: String, message: String },
    /// A periodic CPU/RAM sample, taken independently of any import/export/preview activity —
    /// see [`sample_resource_usage`]. `cpu_percent` is the whole system's usage (all cores
    /// averaged, `sysinfo`'s own convention), not just this process's, since `request.md`'s
    /// wording ("uso de CPU/RAM... numa sessão de 2h") is about overall system load during a
    /// session, not this process's isolated footprint.
    ResourceUsage {
        cpu_percent: f32,
        ram_used_mb: u64,
        ram_total_mb: u64,
    },
    /// A periodic GPU utilization/VRAM sample from the first NVML-visible device — see
    /// [`GpuSampler`]. Only ever emitted on a machine with a working NVIDIA driver; there is no
    /// "unavailable" variant of this event, [`GpuSampler::sample`] just returns `None` instead
    /// (same convention [`ResourceUsage`][TelemetryEvent::ResourceUsage] would follow if CPU/RAM
    /// sampling could ever fail, which in practice it doesn't).
    GpuUsage {
        gpu_percent: u32,
        vram_used_mb: u64,
        vram_total_mb: u64,
    },
}

#[derive(Debug)]
pub enum TelemetryError {
    Io(std::io::Error),
    Serialize(serde_json::Error),
}

impl std::fmt::Display for TelemetryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TelemetryError::Io(e) => write!(f, "failed to write telemetry record: {e}"),
            TelemetryError::Serialize(e) => write!(f, "failed to serialize telemetry record: {e}"),
        }
    }
}

impl std::error::Error for TelemetryError {}

/// One line of `telemetry.jsonl` — a Unix-epoch-seconds timestamp plus the event itself,
/// flattened so the JSON stays flat (`{"timestamp": ..., "event": "import_completed", ...}`)
/// rather than nesting the event under its own key.
#[derive(Serialize)]
struct TelemetryRecord<'a> {
    timestamp: u64,
    #[serde(flatten)]
    event: &'a TelemetryEvent,
}

/// Appends one [`TelemetryEvent`] as a single JSON line to `telemetry_path`, creating the file
/// (and any missing parent directories) if it doesn't exist yet, rotating it first via
/// [`rotate_if_oversized`] if it's grown past [`ROTATE_AT_BYTES`]. Never truncates or rewrites
/// existing lines otherwise — still a plain append-only log within one rotation.
pub fn record_event(telemetry_path: &Path, event: &TelemetryEvent) -> Result<(), TelemetryError> {
    if let Some(parent) = telemetry_path.parent() {
        std::fs::create_dir_all(parent).map_err(TelemetryError::Io)?;
    }
    rotate_if_oversized(telemetry_path);
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let line = serde_json::to_string(&TelemetryRecord { timestamp, event })
        .map_err(TelemetryError::Serialize)?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(telemetry_path)
        .map_err(TelemetryError::Io)?;
    writeln!(file, "{line}").map_err(TelemetryError::Io)?;
    Ok(())
}

/// Renames `telemetry_path` to a `.1`-suffixed backup (replacing any previous one via
/// `fs::rename`'s own overwrite-on-rename behavior) if it exists and is at least
/// [`ROTATE_AT_BYTES`] large. Best-effort: a missing file, or a rename that fails (e.g. no
/// permission), is silently treated as "nothing to rotate" — [`record_event`] must still be
/// able to append a record either way, rotation is a bonus, not a precondition.
fn rotate_if_oversized(telemetry_path: &Path) {
    let Ok(metadata) = std::fs::metadata(telemetry_path) else {
        return;
    };
    if metadata.len() < ROTATE_AT_BYTES {
        return;
    }
    let mut backup_name = telemetry_path.as_os_str().to_os_string();
    backup_name.push(".1");
    let _ = std::fs::rename(telemetry_path, PathBuf::from(backup_name));
}

/// Wraps a `sysinfo::System`, kept alive across calls since CPU-usage percentages are a delta
/// between two refreshes (`sysinfo`'s own model) rather than an instantaneous read — a fresh
/// one-shot `System` would report `0.0` every single time. Call [`ResourceSampler::sample`]
/// repeatedly (e.g. once per telemetry-sampling interval on a long-lived background thread)
/// rather than constructing a new sampler each time.
pub struct ResourceSampler {
    sys: System,
}

impl ResourceSampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        Self { sys }
    }

    /// Refreshes and returns a [`TelemetryEvent::ResourceUsage`] sample. The very first call
    /// right after [`ResourceSampler::new`] may read `0.0` CPU usage — `sysinfo` needs a real
    /// elapsed interval between two refreshes to compute a delta — harmless for a periodic
    /// sampler, since only the first sample in a session is ever affected.
    pub fn sample(&mut self) -> TelemetryEvent {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        TelemetryEvent::ResourceUsage {
            cpu_percent: self.sys.global_cpu_usage(),
            ram_used_mb: self.sys.used_memory() / (1024 * 1024),
            ram_total_mb: self.sys.total_memory() / (1024 * 1024),
        }
    }
}

impl Default for ResourceSampler {
    fn default() -> Self {
        Self::new()
    }
}

/// NVIDIA-only GPU utilization/VRAM sampler, via the NVIDIA Management Library
/// (`nvml-wrapper`). Unlike [`ResourceSampler`] (always constructible — `sysinfo` always has
/// *some* CPU/RAM to read), [`GpuSampler::new`] can fail outright: no NVIDIA GPU, no driver, or
/// the library itself missing all report as `None` here rather than a panic or a build-time
/// requirement (`Nvml::init()` dynamically loads the library at runtime). Holds the initialized
/// `Nvml` handle, not a `Device` — `Device<'nvml>` borrows from `Nvml`, so [`GpuSampler::sample`]
/// looks its device handle up fresh each call (a cheap index lookup, not a re-init) rather than
/// this struct being self-referential.
pub struct GpuSampler {
    nvml: nvml_wrapper::Nvml,
}

impl GpuSampler {
    /// Returns `None` when no NVML-compatible GPU is available at all on this machine.
    pub fn new() -> Option<Self> {
        nvml_wrapper::Nvml::init().ok().map(|nvml| Self { nvml })
    }

    /// Samples utilization and VRAM usage from the first NVML-visible device (index 0) — this
    /// codebase targets a single-editor desktop workstation, not a multi-GPU render farm, so
    /// there is no per-device selection to thread through. Returns `None` on any NVML query
    /// failure (e.g. a transient driver hiccup); a periodic sampler can just skip that tick, the
    /// same "best-effort, not required" contract [`ResourceSampler::sample`] has for CPU/RAM.
    pub fn sample(&self) -> Option<TelemetryEvent> {
        let device = self.nvml.device_by_index(0).ok()?;
        let utilization = device.utilization_rates().ok()?;
        let memory = device.memory_info().ok()?;
        Some(TelemetryEvent::GpuUsage {
            gpu_percent: utilization.gpu,
            vram_used_mb: memory.used / (1024 * 1024),
            vram_total_mb: memory.total / (1024 * 1024),
        })
    }
}

#[cfg(test)]
#[path = "telemetry_test.rs"]
mod tests;
