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

use std::path::Path;

use avbridge::{generate_waveform, WaveformError};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn computes_peaks_for_a_real_audio_file() {
    let peaks = generate_waveform(&fixture("audio.m4a"), 64).unwrap();

    assert_eq!(peaks.len(), 64);
    for (min, max) in &peaks {
        assert!((-1.0..=1.0).contains(min), "min {min} out of range");
        assert!((-1.0..=1.0).contains(max), "max {max} out of range");
        assert!(min <= max, "bucket min {min} > max {max}");
    }
    // Real audio, not silence - at least one bucket must show actual amplitude.
    assert!(peaks.iter().any(|(min, max)| *min < 0.0 || *max > 0.0));
}

#[test]
fn one_bucket_contains_the_global_extrema_of_finer_waveforms() {
    let one_bucket = generate_waveform(&fixture("audio.m4a"), 1).unwrap();
    let fine_buckets = generate_waveform(&fixture("audio.m4a"), 64).unwrap();

    let expected_min = fine_buckets
        .iter()
        .map(|(min, _)| *min)
        .min_by(f32::total_cmp)
        .unwrap();
    let expected_max = fine_buckets
        .iter()
        .map(|(_, max)| *max)
        .max_by(f32::total_cmp)
        .unwrap();

    assert_eq!(one_bucket, vec![(expected_min, expected_max)]);
}

#[test]
fn fails_on_missing_input() {
    let err = generate_waveform(&fixture("does_not_exist.mp4"), 64).unwrap_err();
    assert!(matches!(err, WaveformError::OpenInput));
}

#[test]
fn fails_on_zero_bucket_count() {
    let err = generate_waveform(&fixture("audio.m4a"), 0).unwrap_err();
    assert!(matches!(err, WaveformError::Pipeline));
}
