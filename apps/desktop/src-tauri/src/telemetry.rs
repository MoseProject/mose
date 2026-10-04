//! Local-only telemetry -- mirrors `ui::App`'s `telemetry.rs` (CPU/RAM/GPU resource sampling,
//! `avcore::record_event`), minus `PreviewFrameTime` sampling (egui's per-frame render-loop hook
//! has no equivalent in the webview's own preview-tick mechanism). Never sent anywhere; separate
//! from `error_reporting`'s remote consent.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::{self, UnboundedSender};

const RESOURCE_TELEMETRY_INTERVAL: Duration = Duration::from_secs(30);

fn telemetry_path() -> PathBuf {
    crate::platform_log_dir().join("telemetry.jsonl")
}

/// Spawns the writer thread plus CPU/RAM and GPU samplers -- called once at startup. `enabled`
/// is checked each sampling tick (not just once), so toggling the Preferences checkbox takes
/// effect on the next tick without a restart.
pub fn spawn(enabled: Arc<AtomicBool>) {
    let gpu = avcore::GpuSampler::new().map(|gpu| move || gpu.sample());
    spawn_to(telemetry_path(), enabled, RESOURCE_TELEMETRY_INTERVAL, gpu);
}

/// Writes a resource (and, when an NVML GPU is present, GPU) sample to `path` every `interval`
/// while `enabled` is set. Split from [`spawn`] so tests can use a scratch file and a short
/// interval.
pub(crate) fn spawn_to(
    path: PathBuf,
    enabled: Arc<AtomicBool>,
    interval: Duration,
    gpu: Option<impl FnMut() -> Option<avcore::TelemetryEvent> + Send + 'static>,
) {
    let (tx, mut rx) = mpsc::unbounded_channel::<avcore::TelemetryEvent>();
    std::thread::spawn(move || {
        while let Some(event) = rx.blocking_recv() {
            if let Err(e) = avcore::record_event(&path, &event) {
                tracing::warn!(error = %e, "failed to write telemetry record");
            }
        }
    });
    let mut resources = avcore::ResourceSampler::new();
    spawn_sampler(tx.clone(), Arc::clone(&enabled), interval, move || {
        Some(resources.sample())
    });
    match gpu {
        Some(gpu) => spawn_sampler(tx, enabled, interval, gpu),
        None => {
            tracing::info!("no NVML-compatible GPU detected -- GPU usage telemetry disabled");
        }
    }
}

/// Calls `sample` every `interval` while `enabled`, forwarding what it returns until the
/// receiving end goes away.
pub(crate) fn spawn_sampler(
    tx: UnboundedSender<avcore::TelemetryEvent>,
    enabled: Arc<AtomicBool>,
    interval: Duration,
    mut sample: impl FnMut() -> Option<avcore::TelemetryEvent> + Send + 'static,
) {
    std::thread::spawn(move || loop {
        std::thread::sleep(interval);
        if !enabled.load(Ordering::Relaxed) {
            continue;
        }
        let Some(event) = sample() else {
            continue;
        };
        if tx.send(event).is_err() {
            return;
        }
    });
}

#[cfg(test)]
#[path = "telemetry_test.rs"]
mod tests;
