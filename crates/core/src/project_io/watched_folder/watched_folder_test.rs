// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 2 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <http://www.gnu.org/licenses/>.

use std::path::Path;
use std::time::{Duration, Instant};

use super::*;

#[test]
fn recognizes_every_extension_case_insensitively() {
    for ext in VIDEO_EXTENSIONS {
        assert!(is_video_file(Path::new(&format!("clip.{ext}"))));
        assert!(is_video_file(Path::new(&format!(
            "clip.{}",
            ext.to_uppercase()
        ))));
    }
}

#[test]
fn rejects_non_video_extensions_and_extensionless_paths() {
    assert!(!is_video_file(Path::new("notes.txt")));
    assert!(!is_video_file(Path::new("clip")));
    assert!(!is_video_file(Path::new("clip.")));
}

#[test]
fn output_path_joins_the_subfolder_and_keeps_the_file_name() {
    let watch_dir = Path::new("E:/records/teste");
    let source = Path::new("E:/records/teste/gameplay_01.mp4");
    let out = output_path_for(watch_dir, "processed", source);
    assert_eq!(out, Path::new("E:/records/teste/processed/gameplay_01.mp4"));
}

#[test]
fn stability_tracker_is_unstable_on_first_observation() {
    let mut tracker = StabilityTracker::new();
    let now = Instant::now();
    let stable = tracker.poll(Path::new("a.mp4"), 1000, now, Duration::from_secs(6));
    assert!(!stable);
}

#[test]
fn stability_tracker_becomes_stable_once_the_window_elapses_at_the_same_size() {
    let mut tracker = StabilityTracker::new();
    let t0 = Instant::now();
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t0, Duration::from_secs(6)));

    // Same size, not enough time elapsed yet.
    let t1 = t0 + Duration::from_secs(3);
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t1, Duration::from_secs(6)));

    // Same size, window elapsed.
    let t2 = t0 + Duration::from_secs(7);
    assert!(tracker.poll(Path::new("a.mp4"), 1000, t2, Duration::from_secs(6)));
}

#[test]
fn stability_tracker_resets_the_clock_when_size_changes() {
    let mut tracker = StabilityTracker::new();
    let t0 = Instant::now();
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t0, Duration::from_secs(6)));

    // Still growing at t0+7s (past the window) but with a different size -> resets, not stable.
    let t1 = t0 + Duration::from_secs(7);
    assert!(!tracker.poll(Path::new("a.mp4"), 2000, t1, Duration::from_secs(6)));

    // Now stays at 2000 for another full window.
    let t2 = t1 + Duration::from_secs(6);
    assert!(tracker.poll(Path::new("a.mp4"), 2000, t2, Duration::from_secs(6)));
}

#[test]
fn stability_tracker_tracks_multiple_files_independently() {
    let mut tracker = StabilityTracker::new();
    let t0 = Instant::now();
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t0, Duration::from_secs(6)));
    assert!(!tracker.poll(Path::new("b.mp4"), 5000, t0, Duration::from_secs(6)));

    let t1 = t0 + Duration::from_secs(7);
    assert!(tracker.poll(Path::new("a.mp4"), 1000, t1, Duration::from_secs(6)));
    // b.mp4 changed size at t1, so it's not stable even though a.mp4 now is.
    assert!(!tracker.poll(Path::new("b.mp4"), 5500, t1, Duration::from_secs(6)));
}

#[test]
fn forget_clears_a_files_tracked_state() {
    let mut tracker = StabilityTracker::new();
    let t0 = Instant::now();
    let t1 = t0 + Duration::from_secs(7);
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t0, Duration::from_secs(6)));
    assert!(tracker.poll(Path::new("a.mp4"), 1000, t1, Duration::from_secs(6)));

    tracker.forget(Path::new("a.mp4"));

    // Re-polling after forgetting starts the clock over, even at the same size/time.
    assert!(!tracker.poll(Path::new("a.mp4"), 1000, t1, Duration::from_secs(6)));
}

fn fixture(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn every_process_error_has_a_message() {
    use crate::loudness::LoudnessError;
    use crate::probe::ProbeError;
    use crate::render::RenderError;
    let missing = || LoudnessError::Bridge(avbridge::LoudnessError::OpenInput);
    assert!(ProcessError::MeasureBefore(missing())
        .to_string()
        .contains("source loudness"));
    assert!(ProcessError::Probe(ProbeError::Open)
        .to_string()
        .contains("probe"));
    assert!(ProcessError::Render(RenderError::EmptyTimeline)
        .to_string()
        .contains("render"));
    assert!(ProcessError::MeasureAfter(missing())
        .to_string()
        .contains("output loudness"));
}

#[test]
fn a_watched_file_is_normalised_into_the_output_folder() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("nested").join("normalised.mp4");
    let mut last = 0;
    let (before, after) = process_watched_file(
        &fixture("video.mp4"),
        &output,
        -14.0,
        &AtomicBool::new(false),
        |percent| last = percent,
    )
    .unwrap()
    .expect("not cancelled");
    assert!(output.exists());
    assert_eq!(last, 100);
    assert!(before.integrated_lufs.is_finite() && after.integrated_lufs.is_finite());
}

#[test]
fn processing_can_be_cancelled_and_bad_sources_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let output = dir.path().join("out.mp4");
    let cancelled = process_watched_file(
        &fixture("video.mp4"),
        &output,
        -14.0,
        &AtomicBool::new(true),
        |_| {},
    )
    .unwrap();
    assert!(cancelled.is_none());

    let missing = process_watched_file(
        &dir.path().join("missing.mp4"),
        &output,
        -14.0,
        &AtomicBool::new(false),
        |_| {},
    );
    assert!(matches!(missing, Err(ProcessError::MeasureBefore(_))));
}
