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

//! Whole-file operations that had no test: remux, proxy, shape overlays, privacy blur, and the
//! empty-input guards of the timeline and audio-mix exports.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use avbridge::{
    apply_privacy_blur, apply_shape_overlays, apply_text_overlays, encode_matte_video,
    encode_timeline_export_multi, generate_proxy, mix_audio_timeline, probe, remux_copy, version,
    AudioMixError, Canvas, ClipSegment, EncodeError, EncodeOutcome, GpuEncoderPreference,
    PrivacyBlurError, ProxyError, RemuxError, ShapeSegment, StreamKind, TextOverlayError,
};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("avbridge_media_ops_{name}"))
}

#[test]
fn the_native_library_reports_a_stable_version() {
    assert_eq!(version(), version());
}

#[test]
fn remuxing_copies_the_streams_and_a_missing_input_is_reported() {
    let out = temp("remux.mp4");
    let _ = std::fs::remove_file(&out);
    remux_copy(&fixture("video.mp4"), &out).unwrap();
    let original = probe(&fixture("video.mp4")).unwrap();
    let copy = probe(&out).unwrap();
    assert_eq!(copy.kind, StreamKind::Video);
    assert_eq!(copy.resolution, original.resolution);
    let _ = std::fs::remove_file(&out);

    assert!(matches!(
        remux_copy(&fixture("missing.mp4"), &out),
        Err(RemuxError::OpenInput)
    ));
}

#[test]
fn a_proxy_is_scaled_to_the_target_height() {
    let out = temp("proxy.mp4");
    let _ = std::fs::remove_file(&out);
    generate_proxy(&fixture("video.mp4"), &out, 120).unwrap();
    let proxy = probe(&out).unwrap();
    assert_eq!(proxy.resolution.map(|(_, height)| height), Some(120));
    let _ = std::fs::remove_file(&out);

    assert!(matches!(
        generate_proxy(&fixture("missing.mp4"), &out, 120),
        Err(ProxyError::OpenInput)
    ));
}

#[test]
fn shape_overlays_are_composited_and_failures_are_reported() {
    let source = fixture("video.mp4");
    let info = probe(&source).unwrap();
    let (width, height) = info.resolution.unwrap();
    let out = temp("shapes.mp4");
    let _ = std::fs::remove_file(&out);
    let identity = ShapeSegment {
        filter_desc: "geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)'".to_string(),
    };

    // No segments is a no-op that writes nothing.
    apply_shape_overlays(&source, &out, &[], width, height, 30, 1).unwrap();
    assert!(!out.exists());

    apply_shape_overlays(
        &source,
        &out,
        std::slice::from_ref(&identity),
        width,
        height,
        30,
        1,
    )
    .unwrap();
    assert_eq!(probe(&out).unwrap().resolution, Some((width, height)));
    let _ = std::fs::remove_file(&out);

    assert!(matches!(
        apply_shape_overlays(
            &fixture("missing.mp4"),
            &out,
            &[identity],
            width,
            height,
            30,
            1
        ),
        Err(TextOverlayError::OpenInput)
    ));
    let broken = ShapeSegment {
        filter_desc: "not_a_filter".to_string(),
    };
    assert!(matches!(
        apply_shape_overlays(&source, &out, &[broken], width, height, 30, 1),
        Err(TextOverlayError::FilterGraph)
    ));
    let nul = ShapeSegment {
        filter_desc: "a\0b".to_string(),
    };
    assert!(matches!(
        apply_shape_overlays(&source, &out, &[nul], width, height, 30, 1),
        Err(TextOverlayError::InvalidPath(_))
    ));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn the_privacy_blur_needs_a_positive_sigma_and_blurs_through_a_matte() {
    let source = fixture("video.mp4");
    let info = probe(&source).unwrap();
    let (width, height) = info.resolution.unwrap();
    let matte = temp("matte.mp4");
    let out = temp("blurred.mp4");
    let _ = std::fs::remove_file(&out);

    // A matte that is white on the left half, one frame per source frame.
    let frame: Vec<u8> = (0..width * height)
        .map(|index| if index % width < width / 2 { 255 } else { 0 })
        .collect();
    encode_matte_video(&vec![frame; 30], width, height, 30, 1, &matte).unwrap();

    assert!(matches!(
        apply_privacy_blur(&source, &out, &matte, 0.0, width, height, 30, 1),
        Err(PrivacyBlurError::NonPositiveSigma)
    ));
    apply_privacy_blur(&source, &out, &matte, 6.0, width, height, 30, 1).unwrap();
    assert_eq!(probe(&out).unwrap().resolution, Some((width, height)));
    let _ = std::fs::remove_file(&out);

    assert!(matches!(
        apply_privacy_blur(
            &fixture("missing.mp4"),
            &out,
            &matte,
            6.0,
            width,
            height,
            30,
            1
        ),
        Err(PrivacyBlurError::OpenInput)
    ));
    let _ = std::fs::remove_file(&matte);
}

#[test]
fn empty_inputs_are_rejected_before_any_native_call() {
    let cancel = AtomicBool::new(false);
    let canvas = Canvas {
        width: 320,
        height: 240,
        fps_num: 30,
        fps_den: 1,
        bit_rate_bps: 500_000,
    };
    let out = temp("never-written.mp4");
    for tracks in [Vec::new(), vec![Vec::new()]] {
        assert!(matches!(
            encode_timeline_export_multi(
                &tracks,
                canvas,
                &out,
                -14.0,
                GpuEncoderPreference::Auto,
                &cancel,
                |_| {},
            ),
            Err(EncodeError::EmptyTimeline)
        ));
    }
    assert!(matches!(
        mix_audio_timeline(&[], 1.0, &out, -14.0, &cancel),
        Err(AudioMixError::NoAudio)
    ));
    assert!(!out.exists());
}

fn clip() -> ClipSegment {
    ClipSegment {
        source_path: fixture("video.mp4"),
        source_in_secs: 0.0,
        source_out_secs: 0.4,
        gain_db: 0.0,
        video_filter: String::new(),
        frozen: false,
        speed_factor: 1.0,
        smooth_speed_ramp_end_factor: 0.0,
        position_x_expr: String::new(),
        position_y_expr: String::new(),
        transition_in: 0,
        transition_duration_secs: 0.3,
        timeline_start_secs: 0.0,
        mask_video_path: String::new(),
        blend_mode: String::new(),
        anchor_x: 0.5,
        anchor_y: 0.5,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
    }
}

#[test]
fn a_single_track_multi_export_delegates_to_the_plain_timeline_export() {
    let out = temp("single-track.mp4");
    let _ = std::fs::remove_file(&out);
    let canvas = Canvas {
        width: 320,
        height: 240,
        fps_num: 30,
        fps_den: 1,
        bit_rate_bps: 500_000,
    };
    let outcome = encode_timeline_export_multi(
        &[vec![clip()]],
        canvas,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);
    assert_eq!(probe(&out).unwrap().resolution, Some((320, 240)));
    let _ = std::fs::remove_file(&out);
}

#[test]
fn text_overlays_without_segments_are_a_no_op() {
    let out = temp("no-text.mp4");
    let _ = std::fs::remove_file(&out);
    apply_text_overlays(&fixture("video.mp4"), &out, &[], 320, 240, 30, 1).unwrap();
    assert!(!out.exists());
}

#[test]
fn extracts_one_audio_stream_as_its_own_file() {
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../core/tests/fixtures/audio_codecs/obs_multitrack.mkv");
    let dir = std::env::temp_dir().join(format!("avbridge-extract-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();

    // Stream 2 of the fixture is the one titled "Mic".
    let out = dir.join("mic.mka");
    avbridge::extract_audio_stream(&source, &out, 2).expect("extracts");
    let info = avbridge::probe(&out).expect("the copy probes");
    assert_eq!(info.audio_stream_count, 1);
    assert_eq!(info.kind, avbridge::StreamKind::Audio);
    assert_eq!(info.codec_name, "aac");
    assert!((info.duration_secs - 0.4).abs() < 0.1, "{}", info.duration_secs);

    // The video stream and out-of-range indexes are refused.
    assert!(avbridge::extract_audio_stream(&source, &dir.join("video.mka"), 0).is_err());
    assert!(avbridge::extract_audio_stream(&source, &dir.join("none.mka"), 9).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}
