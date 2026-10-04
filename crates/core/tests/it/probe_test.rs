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

use avcore::media::MediaKind;
use avcore::probe::{probe_media, ProbeError};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn probes_a_video_stream_as_the_primary_track() {
    let media = probe_media(&fixture("video.mp4")).unwrap();
    assert_eq!(media.kind, MediaKind::Video);
    assert_eq!(media.codec, "mpeg4");
    assert_eq!(media.resolution, Some((320, 240)));
    assert_eq!(media.sample_rate_khz, None);
    assert!(media.has_audio);
    assert!((media.duration_secs - 1.0).abs() < 0.1);
}

#[test]
fn converts_container_bitrate_from_bps_to_mbps() {
    let media = probe_media(&fixture("video.mp4")).unwrap();
    assert!(media.bitrate_mbps > 0.0);
}

#[test]
fn parses_frame_rate() {
    let media = probe_media(&fixture("video.mp4")).unwrap();
    assert!((media.fps.unwrap() - 30.0).abs() < 0.01);
}

#[test]
fn falls_back_to_the_audio_stream_when_there_is_no_video() {
    let media = probe_media(&fixture("audio.m4a")).unwrap();
    assert_eq!(media.kind, MediaKind::Audio);
    assert_eq!(media.codec, "aac");
    assert_eq!(media.resolution, None);
    assert_eq!(media.sample_rate_khz, Some(44.1));
    assert!(media.has_audio);
}

#[test]
fn rejects_a_missing_file() {
    assert!(matches!(
        probe_media(&fixture("does_not_exist.mp4")),
        Err(ProbeError::Open)
    ));
}

#[test]
fn into_media_asset_carries_the_probed_fields_through() {
    let media = probe_media(&fixture("video.mp4")).unwrap();
    let asset = media.into_media_asset(
        7,
        "video.mp4".to_string(),
        PathBuf::from("/videos/video.mp4"),
    );
    assert_eq!(asset.id, 7);
    assert_eq!(asset.file_name, "video.mp4");
    assert_eq!(asset.source_path, PathBuf::from("/videos/video.mp4"));
    assert_eq!(asset.codec, "mpeg4");
    assert!(asset.has_audio);
    assert!(asset.loudness.is_none());
}
