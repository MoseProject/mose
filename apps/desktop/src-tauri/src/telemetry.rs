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
    let (tx, mut rx) = mpsc::unbounded_channel::<avcore::TelemetryEvent>();
    let path = telemetry_path();
    std::thread::spawn(move || {
        while let Some(event) = rx.blocking_recv() {
            if let Err(e) = avcore::record_event(&path, &event) {
                tracing::warn!(error = %e, "failed to write telemetry record");
            }
        }
    });
    spawn_resource_sampler(tx.clone(), Arc::clone(&enabled));
    spawn_gpu_sampler(tx, enabled);
}

fn spawn_resource_sampler(tx: UnboundedSender<avcore::TelemetryEvent>, enabled: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut sampler = avcore::ResourceSampler::new();
        loop {
            std::thread::sleep(RESOURCE_TELEMETRY_INTERVAL);
            if !enabled.load(Ordering::Relaxed) {
                continue;
            }
            if tx.send(sampler.sample()).is_err() {
                return;
            }
        }
    });
}

fn spawn_gpu_sampler(tx: UnboundedSender<avcore::TelemetryEvent>, enabled: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let Some(sampler) = avcore::GpuSampler::new() else {
            tracing::info!("no NVML-compatible GPU detected -- GPU usage telemetry disabled");
            return;
        };
        loop {
            std::thread::sleep(RESOURCE_TELEMETRY_INTERVAL);
            if !enabled.load(Ordering::Relaxed) {
                continue;
            }
            let Some(event) = sampler.sample() else {
                continue;
            };
            if tx.send(event).is_err() {
                return;
            }
        }
    });
}
