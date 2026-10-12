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

use avbridge::{probe, ProbeError, StreamKind};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn probes_a_video_file() {
    let info = probe(&fixture("video.mp4")).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.codec_name, "mpeg4");
    assert_eq!(info.resolution, Some((320, 240)));
    assert!((info.fps.unwrap() - 30.0).abs() < 0.01);
    assert!((info.duration_secs - 1.0).abs() < 0.1);
    assert!(info.bit_rate.unwrap() > 0);
    assert_eq!(info.sample_rate_hz, None);
    assert!(info.has_audio);
}

#[test]
fn falls_back_to_audio_when_there_is_no_video_stream() {
    let info = probe(&fixture("audio.m4a")).unwrap();
    assert_eq!(info.kind, StreamKind::Audio);
    assert_eq!(info.codec_name, "aac");
    assert_eq!(info.resolution, None);
    assert_eq!(info.fps, None);
    assert_eq!(info.sample_rate_hz, Some(44100));
    assert!(info.has_audio);
}

#[test]
fn missing_file_returns_open_error() {
    let err = probe(&fixture("does_not_exist.mp4")).unwrap_err();
    assert!(matches!(err, ProbeError::Open));
}

#[test]
fn non_media_file_returns_open_error() {
    let err = probe(&fixture("garbage.bin")).unwrap_err();
    assert!(matches!(err, ProbeError::Open));
}

fn core_fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures")
        .join(name)
}

// FFmpeg's own numbers: AVCOL_SPC_BT709 1, AVCOL_SPC_BT2020_NCL 9, AVCOL_PRI_BT2020 9,
// AVCOL_TRC_BT709 1, AVCOL_TRC_SMPTE2084 16.
#[test]
fn reports_the_colour_description_of_sdr_and_hdr_video() {
    let sdr = probe(&core_fixture("color/hd709_tagged.mkv")).unwrap();
    let color = sdr.color.expect("a video stream has a colour description");
    assert_eq!(color.bit_depth, 8);
    assert_eq!(color.space, 1);
    assert!(!color.full_range);

    let hdr = probe(&core_fixture("color/hdr10_pq.mkv")).unwrap();
    let color = hdr.color.unwrap();
    assert_eq!(color.bit_depth, 10);
    assert_eq!(color.space, 9);
    assert_eq!(color.primaries, 9);
    assert_eq!(color.transfer, 16);

    let full = probe(&core_fixture("color/hd709_fullrange.mkv")).unwrap();
    assert!(full.color.unwrap().full_range);
}

#[test]
fn an_audio_only_file_has_no_colour_description() {
    assert_eq!(probe(&fixture("audio.m4a")).unwrap().color, None);
}

#[test]
fn lists_every_audio_stream_with_its_title() {
    let info = probe(&core_fixture("audio_codecs/obs_multitrack.mkv")).unwrap();
    assert_eq!(info.audio_stream_count, 3);
    let titles: Vec<&str> = info
        .audio_streams
        .iter()
        .map(|s| s.title.as_str())
        .collect();
    assert_eq!(titles, ["Game", "Mic", "Chat"]);
    assert_eq!(info.audio_streams[0].language, "por");
    assert_eq!(info.audio_streams[0].stream_index, 1);
    assert!(info
        .audio_streams
        .iter()
        .all(|s| s.channels == 2 && s.codec_name == "aac"));
}

#[test]
fn a_file_with_one_audio_stream_lists_one() {
    let info = probe(&fixture("video.mp4")).unwrap();
    assert_eq!(info.audio_stream_count, 1);
    assert_eq!(info.audio_streams.len(), 1);
}
