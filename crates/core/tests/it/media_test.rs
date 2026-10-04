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

use std::path::PathBuf;

use avcore::media::{format_timecode, MediaAsset, MediaKind};

#[test]
fn zero_seconds_is_zeroed_mm_ss() {
    assert_eq!(format_timecode(0.0), "00:00");
}

#[test]
fn under_a_minute_pads_seconds() {
    assert_eq!(format_timecode(5.0), "00:05");
}

#[test]
fn drops_the_hour_component_under_an_hour() {
    assert_eq!(format_timecode(65.0), "01:05");
    assert_eq!(format_timecode(3599.0), "59:59");
}

#[test]
fn includes_hour_component_at_and_beyond_an_hour() {
    assert_eq!(format_timecode(3600.0), "1:00:00");
    assert_eq!(format_timecode(3661.0), "1:01:01");
    assert_eq!(format_timecode(3.0 * 3600.0 + 20.0 * 60.0), "3:20:00");
}

#[test]
fn rounds_to_the_nearest_second() {
    assert_eq!(format_timecode(59.6), "01:00");
    assert_eq!(format_timecode(0.4), "00:00");
}

#[test]
fn duration_label_delegates_to_format_timecode() {
    let asset = MediaAsset {
        id: 1,
        file_name: "clip.mp4".to_string(),
        source_path: PathBuf::from("/videos/clip.mp4"),
        kind: MediaKind::Video,
        has_audio: true,
        duration_secs: 134.0,
        codec: "H.264".to_string(),
        source_bitrate_mbps: 42.0,
        resolution: Some((1920, 1080)),
        fps: Some(60.0),
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    };
    assert_eq!(asset.duration_label(), "02:14");
}
