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

use avcore::waveform::{
    detailed_bucket_count, generate_detailed_waveform, generate_waveform, WAVEFORM_BUCKET_COUNT,
    WAVEFORM_DETAIL_MAX_BUCKETS,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn computes_a_full_resolution_peak_table_for_a_real_audio_file() {
    let peaks = generate_waveform(&fixture("audio.m4a")).unwrap();

    assert_eq!(peaks.len(), WAVEFORM_BUCKET_COUNT);
    assert!(peaks.iter().any(|(min, max)| *min < 0.0 || *max > 0.0));
}

#[test]
fn computes_peaks_for_a_video_with_an_audio_track() {
    let peaks = generate_waveform(&fixture("video.mp4")).unwrap();

    assert_eq!(peaks.len(), WAVEFORM_BUCKET_COUNT);
    // The fixture has a synthesized 1000Hz tone — at least one bucket must show amplitude.
    assert!(peaks.iter().any(|(min, max)| *min < 0.0 || *max > 0.0));
}

#[test]
fn all_peaks_are_in_the_normalized_range() {
    let peaks = generate_waveform(&fixture("audio.m4a")).unwrap();

    for (min, max) in &peaks {
        assert!((-1.0..=1.0).contains(min), "min {min} out of [-1,1]");
        assert!((-1.0..=1.0).contains(max), "max {max} out of [-1,1]");
        assert!(min <= max, "bucket min {min} > max {max}");
    }
}

#[test]
fn errors_on_a_missing_source() {
    let result = generate_waveform(&fixture("does_not_exist.mp4"));
    assert!(result.is_err());
}

#[test]
fn the_detail_bucket_count_scales_with_duration_within_its_bounds() {
    assert_eq!(detailed_bucket_count(0.0), WAVEFORM_BUCKET_COUNT);
    assert_eq!(detailed_bucket_count(10.0), WAVEFORM_BUCKET_COUNT);
    assert_eq!(detailed_bucket_count(600.0), 30_000);
    assert_eq!(detailed_bucket_count(9_900.0), 495_000);
    assert_eq!(detailed_bucket_count(1.0e9), WAVEFORM_DETAIL_MAX_BUCKETS);
}

#[test]
fn a_detailed_waveform_of_a_real_file_has_the_requested_resolution() {
    let peaks = generate_detailed_waveform(&fixture("video.mp4"), 5.0).unwrap();

    assert_eq!(peaks.len(), detailed_bucket_count(5.0));
    assert!(peaks.iter().any(|(min, max)| *min < 0.0 || *max > 0.0));
}
