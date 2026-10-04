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

use std::fs;

use avcore::{record_event, GpuSampler, ResourceSampler, TelemetryEvent};

#[test]
fn record_event_appends_one_json_line_per_call() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_append");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("telemetry.jsonl");

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 120 }).unwrap();
    record_event(
        &path,
        &TelemetryEvent::ExportCompleted {
            duration_ms: 4500,
            output_duration_secs: 30.0,
            success: true,
        },
    )
    .unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"event\":\"import_completed\""));
    assert!(lines[0].contains("\"duration_ms\":120"));
    assert!(lines[1].contains("\"event\":\"export_completed\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_includes_a_timestamp() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_timestamp");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("telemetry.jsonl");

    record_event(
        &path,
        &TelemetryEvent::Error {
            context: "import".to_string(),
            message: "boom".to_string(),
        },
    )
    .unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"timestamp\":"));
    assert!(contents.contains("\"context\":\"import\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_creates_missing_parent_directories() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_missing_parent");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("nested").join("telemetry.jsonl");
    assert!(!dir.exists());

    record_event(
        &path,
        &TelemetryEvent::PreviewFrameTime {
            frame_time_ms: 16.7,
        },
    )
    .unwrap();

    assert!(path.exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_rotates_an_oversized_file_to_a_backup() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_rotation");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    let backup_path = dir.join("telemetry.jsonl.1");

    // One byte past the private ROTATE_AT_BYTES threshold (10 MiB) - rotation itself never
    // parses the file's contents, so filler bytes are fine.
    let oversized = vec![b'x'; 10 * 1024 * 1024 + 1];
    fs::write(&path, &oversized).unwrap();

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 1 }).unwrap();

    assert!(backup_path.exists());
    assert_eq!(
        fs::metadata(&backup_path).unwrap().len(),
        oversized.len() as u64
    );
    let new_contents = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = new_contents.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"event\":\"import_completed\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_does_not_rotate_a_file_under_the_threshold() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_no_rotation");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    let backup_path = dir.join("telemetry.jsonl.1");
    fs::write(&path, b"pre-existing line\n").unwrap();

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 1 }).unwrap();

    assert!(!backup_path.exists());
    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents.lines().count(), 2);
    assert!(contents.starts_with("pre-existing line"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn resource_sampler_reports_nonzero_ram_on_a_real_machine() {
    let mut sampler = ResourceSampler::new();
    let TelemetryEvent::ResourceUsage {
        cpu_percent,
        ram_used_mb,
        ram_total_mb,
    } = sampler.sample()
    else {
        panic!("ResourceSampler::sample must return ResourceUsage");
    };

    // Real system RAM on any dev/CI machine this runs on is well above zero, and used can
    // never exceed total.
    assert!(ram_total_mb > 0);
    assert!(ram_used_mb <= ram_total_mb);
    assert!(cpu_percent >= 0.0);
}

#[test]
fn resource_sampler_event_serializes_as_resource_usage() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_resource_usage");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("telemetry.jsonl");

    record_event(
        &path,
        &TelemetryEvent::ResourceUsage {
            cpu_percent: 12.5,
            ram_used_mb: 2048,
            ram_total_mb: 16384,
        },
    )
    .unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"event\":\"resource_usage\""));
    assert!(contents.contains("\"cpu_percent\":12.5"));
    assert!(contents.contains("\"ram_used_mb\":2048"));
    assert!(contents.contains("\"ram_total_mb\":16384"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn gpu_sampler_returns_none_or_a_consistent_sample() {
    // This can't assert a specific outcome -- the real one depends on whether the machine
    // running the test has a working NVIDIA driver, which a dev machine may or may not (this
    // sandbox's own CI/dev container does not, confirmed via a throwaway scratch crate: no
    // panic, no build-time failure, just Nvml::init() returning Err here). Either branch is a
    // real, meaningful assertion about GpuSampler's own no-panic, no-nonsense-values contract.
    match GpuSampler::new() {
        None => {}
        Some(sampler) => {
            if let Some(TelemetryEvent::GpuUsage {
                gpu_percent,
                vram_used_mb,
                vram_total_mb,
            }) = sampler.sample()
            {
                assert!(gpu_percent <= 100);
                assert!(vram_used_mb <= vram_total_mb);
            }
        }
    }
}

#[test]
fn gpu_usage_event_serializes_correctly() {
    let dir = std::env::temp_dir().join("oca_telemetry_test_gpu_usage");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("telemetry.jsonl");

    record_event(
        &path,
        &TelemetryEvent::GpuUsage {
            gpu_percent: 42,
            vram_used_mb: 2048,
            vram_total_mb: 8192,
        },
    )
    .unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    assert!(contents.contains("\"event\":\"gpu_usage\""));
    assert!(contents.contains("\"gpu_percent\":42"));
    assert!(contents.contains("\"vram_used_mb\":2048"));
    assert!(contents.contains("\"vram_total_mb\":8192"));

    let _ = fs::remove_dir_all(&dir);
}
