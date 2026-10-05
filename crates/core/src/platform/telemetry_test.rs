use std::fs;

use super::*;

fn test_dir(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oca_telemetry_test_{tag}_{}", std::process::id()))
}

#[test]
fn record_event_creates_missing_parent_directories_and_writes_one_json_line() {
    let dir = test_dir("creates_parents");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("nested").join("telemetry.jsonl");

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 42 }).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("\"event\":\"import_completed\""));
    assert!(lines[0].contains("\"duration_ms\":42"));
    assert!(lines[0].contains("\"timestamp\""));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_appends_rather_than_truncating() {
    let dir = test_dir("appends");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 1 }).unwrap();
    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 2 }).unwrap();

    let contents = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = contents.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(lines[0].contains("\"duration_ms\":1"));
    assert!(lines[1].contains("\"duration_ms\":2"));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_serializes_every_event_variant_without_error() {
    let dir = test_dir("all_variants");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");

    let events = [
        TelemetryEvent::ImportCompleted { duration_ms: 1 },
        TelemetryEvent::ExportCompleted {
            duration_ms: 2,
            output_duration_secs: 3.5,
            success: true,
        },
        TelemetryEvent::PreviewFrameTime {
            frame_time_ms: 16.6,
        },
        TelemetryEvent::Error {
            context: "import".to_string(),
            message: "boom".to_string(),
        },
        TelemetryEvent::ResourceUsage {
            cpu_percent: 12.3,
            ram_used_mb: 100,
            ram_total_mb: 200,
        },
        TelemetryEvent::GpuUsage {
            gpu_percent: 50,
            vram_used_mb: 1,
            vram_total_mb: 2,
        },
    ];
    for event in &events {
        record_event(&path, event).unwrap();
    }

    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents.lines().count(), events.len());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rotate_if_oversized_is_a_no_op_for_a_missing_file() {
    let dir = test_dir("rotate_missing");
    let _ = fs::remove_dir_all(&dir);
    let path = dir.join("does_not_exist.jsonl");

    // Must not panic even though nothing exists at `path` or its parent.
    rotate_if_oversized(&path);
    assert!(!path.exists());
}

#[test]
fn rotate_if_oversized_leaves_a_small_file_alone() {
    let dir = test_dir("rotate_small");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    fs::write(&path, b"small").unwrap();

    rotate_if_oversized(&path);

    assert!(path.exists());
    assert!(!dir.join("telemetry.jsonl.1").exists());

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rotate_if_oversized_moves_an_oversized_file_to_a_dot_one_backup() {
    let dir = test_dir("rotate_big");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    fs::write(&path, vec![b'x'; ROTATE_AT_BYTES as usize]).unwrap();

    rotate_if_oversized(&path);

    assert!(!path.exists());
    let backup = dir.join("telemetry.jsonl.1");
    assert!(backup.exists());
    assert_eq!(fs::metadata(&backup).unwrap().len(), ROTATE_AT_BYTES);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn rotate_if_oversized_overwrites_a_previous_backup() {
    let dir = test_dir("rotate_overwrite");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    let backup = dir.join("telemetry.jsonl.1");
    fs::write(&backup, b"stale backup").unwrap();
    fs::write(&path, vec![b'x'; ROTATE_AT_BYTES as usize]).unwrap();

    rotate_if_oversized(&path);

    assert_eq!(fs::metadata(&backup).unwrap().len(), ROTATE_AT_BYTES);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn record_event_rotates_before_appending_once_the_file_is_oversized() {
    let dir = test_dir("record_rotates");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("telemetry.jsonl");
    fs::write(&path, vec![b'x'; ROTATE_AT_BYTES as usize]).unwrap();

    record_event(&path, &TelemetryEvent::ImportCompleted { duration_ms: 7 }).unwrap();

    // The oversized content moved to the backup; the live file now holds only the new record.
    assert!(dir.join("telemetry.jsonl.1").exists());
    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(contents.lines().count(), 1);
    assert!(contents.contains("\"duration_ms\":7"));

    let _ = fs::remove_dir_all(&dir);
}
