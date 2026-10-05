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
use std::sync::atomic::{AtomicBool, Ordering};

use avbridge::{
    encode_export, encode_timeline_export, encode_timeline_export_multi, measure_loudness_json,
    probe, Canvas, ClipSegment, EncodeError, EncodeOutcome, GpuEncoderPreference, StreamKind,
};

const CANVAS: Canvas = Canvas {
    width: 320,
    height: 240,
    fps_num: 30,
    fps_den: 1,
    bit_rate_bps: 500_000,
};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn encodes_and_normalizes_a_real_file() {
    let out = std::env::temp_dir().join("avbridge_test_encode_ok.mp4");
    let cancel = AtomicBool::new(false);
    let mut progress_calls = 0;

    let outcome = encode_export(&fixture("video.mp4"), &out, -14.0, &cancel, |_secs| {
        progress_calls += 1;
    })
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);
    assert!(progress_calls > 0);

    // Output must be a valid, probeable file with both streams intact.
    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.codec_name, "mpeg4");
    assert_eq!(info.resolution, Some((320, 240)));

    let _ = std::fs::remove_file(&out);
}

#[test]
fn cancelling_mid_render_leaves_no_valid_file() {
    let out = std::env::temp_dir().join("avbridge_test_encode_cancelled.mp4");
    let cancel = AtomicBool::new(false);
    let mut calls = 0;

    let outcome = encode_export(&fixture("video.mp4"), &out, -14.0, &cancel, |_secs| {
        calls += 1;
        if calls >= 3 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Cancelled);
    // A cancelled render never writes a trailer - the output isn't a valid media file.
    assert!(probe(&out).is_err());

    let _ = std::fs::remove_file(&out);
}

#[test]
fn fails_on_missing_input() {
    let out = std::env::temp_dir().join("avbridge_test_encode_missing.mp4");
    let cancel = AtomicBool::new(false);

    let err =
        encode_export(&fixture("does_not_exist.mp4"), &out, -14.0, &cancel, |_| {}).unwrap_err();

    assert!(matches!(err, EncodeError::OpenInput));
}

#[test]
fn fails_on_audio_only_input_without_error_for_video_field() {
    // audio.m4a has no video stream, but does have audio - encode should still succeed
    // (video passthrough is simply a no-op when there's nothing to copy).
    let out = std::env::temp_dir().join("avbridge_test_encode_audio_only.mp4");
    let cancel = AtomicBool::new(false);

    let outcome = encode_export(&fixture("audio.m4a"), &out, -14.0, &cancel, |_| {}).unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Audio);

    let _ = std::fs::remove_file(&out);
}

fn clip(
    source_in_secs: f64,
    source_out_secs: f64,
    gain_db: f32,
    video_filter: &str,
) -> ClipSegment {
    ClipSegment {
        source_path: fixture("video.mp4"),
        source_in_secs,
        source_out_secs,
        gain_db,
        video_filter: video_filter.to_string(),
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
    }
}

#[test]
fn concatenates_two_segments_with_different_filters_into_one_export() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_ok.mp4");
    let cancel = AtomicBool::new(false);
    let mut progress_calls = 0;

    let segments = [clip(0.0, 0.35, 0.0, ""), clip(0.35, 0.7, 6.0, "hflip")];
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| progress_calls += 1,
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);
    assert!(progress_calls > 0);

    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.resolution, Some((320, 240)));
    // Trimmed to ~0.7s total (0.35 + 0.35) — loose bound since seek lands on the nearest
    // keyframe, not exactly at each segment's requested in-point.
    assert!(info.duration_secs > 0.0 && info.duration_secs < 2.0);

    let _ = std::fs::remove_file(&out);
}

/// Real, executable coverage for `ClipSegment::blend_mode` — no automated test exercised the
/// multi-track overlay path with a blend mode set before this (only manually traced by
/// reading `timeline_export_multi.c`). Two tracks, background + one overlay segment with
/// `blend_mode: "multiply"` set: this is real proof the `blend=all_mode=multiply` filtergraph
/// stage `init_overlay_graph` builds in place of `overlay` for that layer parses and encodes
/// successfully against a real (if older-than-this-repo's-normal-minimum) FFmpeg build, not
/// just something read off `ffmpeg -h filter=blend`'s docs.
#[test]
fn multi_track_export_with_a_blend_mode_set_produces_a_valid_file() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_multi_blend.mp4");
    let cancel = AtomicBool::new(false);

    let background = vec![clip(0.0, 0.5, 0.0, "")];
    let overlay = vec![ClipSegment {
        blend_mode: "multiply".to_string(),
        ..clip(0.0, 0.5, 0.0, "")
    }];
    let tracks = [background, overlay];

    let outcome = encode_timeline_export_multi(
        &tracks,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| {},
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);

    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.resolution, Some((320, 240)));
    assert!(info.duration_secs > 0.0 && info.duration_secs < 2.0);

    let _ = std::fs::remove_file(&out);
}

/// A non-default anchor changes the `pad` stage's offset expression that
/// `avbridge_encode_timeline_export`'s canvas-conforming step builds -- combined with a real
/// `rotate=...` stage (the shape `core::keyframe::rotation_filter_angle_expr` actually emits),
/// this exercises the exact filtergraph anchor is meant to support: rotation pivoting around a
/// point other than the frame's own center. The specific pivot math (verified against real
/// `ffmpeg` CLI output on synthetic test images: byte-identical to the old fixed `(ow-iw)/2`
/// centering at anchor 0.5/0.5, and the anchor point staying fixed under rotation at other
/// anchor values) lives in `core`; this test only confirms the resulting filtergraph string
/// actually encodes without error, not the pivot's exact pixel position (probing that would
/// need a fixture with a recognizable off-center feature, not the plain color-bars fixture
/// this suite uses elsewhere).
#[test]
fn export_with_a_non_default_anchor_and_rotation_produces_a_valid_file() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_anchor_rotation.mp4");
    let cancel = AtomicBool::new(false);

    let segment = ClipSegment {
        video_filter: "rotate=angle='0.3':ow=rotw('0.3'):oh=roth('0.3')".to_string(),
        anchor_x: 0.2,
        anchor_y: 0.8,
        ..clip(0.0, 0.5, 0.0, "")
    };
    let segments = [segment];

    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| {},
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);

    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.resolution, Some((320, 240)));
    assert!(info.duration_secs > 0.0 && info.duration_secs < 2.0);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn freezes_a_segment_into_a_held_frame_export() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_frozen.mp4");
    let cancel = AtomicBool::new(false);
    let mut progress_calls = 0;

    let mut frozen_clip = clip(0.1, 0.6, 0.0, "");
    frozen_clip.frozen = true;
    let segments = [frozen_clip];
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| progress_calls += 1,
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);
    assert!(progress_calls > 0);

    // Still spans the segment's whole trimmed duration (~0.5s) even though only one source
    // frame is ever decoded for it.
    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert!(info.duration_secs > 0.3 && info.duration_secs < 1.0);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn smooth_speed_ramp_produces_a_real_export_with_the_predicted_duration() {
    // A real avfilter_graph_config succeeding on the new setpts=(K/TB)*log(...) expression is
    // proof the expression syntax is valid against the real linked FFmpeg build, not just that
    // the C compiles (P4 item 29's "smooth continuous curve" follow-up — see bridge.h's own
    // doc comment on ClipSegment::smooth_speed_ramp_end_factor for the derivation).
    let out = std::env::temp_dir().join("avbridge_test_smooth_speed_ramp.mp4");
    let cancel = AtomicBool::new(false);

    let mut ramped = clip(0.0, 2.0, 0.0, "");
    ramped.speed_factor = 0.5;
    ramped.smooth_speed_ramp_end_factor = 2.0;
    let segments = [ramped];
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    // Closed form: (D/(v1-v0)) * ln(v1/v0), D=2.0, v0=0.5, v1=2.0.
    let expected_secs = (2.0 / (2.0 - 0.5)) * (2.0f64 / 0.5).ln();
    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    // Loose bound — seek lands on the nearest keyframe, and the encoder's own frame-duration
    // rounding, same "loose bound" reasoning concatenates_two_segments_... above already uses.
    assert!(
        (info.duration_secs - expected_secs).abs() < 1.0,
        "expected ~{expected_secs}s, got {}s",
        info.duration_secs
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn rejects_an_empty_timeline() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_empty.mp4");
    let cancel = AtomicBool::new(false);

    let err = encode_timeline_export(
        &[],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, EncodeError::EmptyTimeline));
}

#[test]
fn fails_on_a_segment_with_a_missing_source() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_missing.mp4");
    let cancel = AtomicBool::new(false);
    let mut missing = clip(0.0, 0.5, 0.0, "");
    missing.source_path = fixture("does_not_exist.mp4");

    let err = encode_timeline_export(
        &[missing],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, EncodeError::OpenInput));
}

#[test]
fn cancelling_mid_timeline_export_leaves_no_valid_file() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_cancelled.mp4");
    let cancel = AtomicBool::new(false);
    let mut calls = 0;

    let segments = [clip(0.0, 0.35, 0.0, ""), clip(0.35, 0.7, 0.0, "")];
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Auto,
        &cancel,
        |_secs| {
            calls += 1;
            if calls >= 3 {
                cancel.store(true, Ordering::Relaxed);
            }
        },
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Cancelled);
    assert!(probe(&out).is_err());

    let _ = std::fs::remove_file(&out);
}

/// Every [`GpuEncoderPreference`] variant must still produce a valid export — on a dev/CI
/// machine without the requested vendor's GPU/driver, `open_video_encoder` (see gpu_encoder.c)
/// falls through to the CPU (libopenh264) encoder rather than failing the whole export. This is
/// the only way the hardware-encoder code paths get exercised at all in this environment: forcing
/// e.g. `Nvenc` here deterministically hits the fallback branch (no NVIDIA GPU present), which is
/// still a real assertion — it proves the fallback logic actually works end-to-end, not just that
/// `Auto`/`Cpu` do.
#[test]
fn every_gpu_encoder_preference_falls_back_to_a_working_export() {
    for (name, preference) in [
        ("auto", GpuEncoderPreference::Auto),
        ("cpu", GpuEncoderPreference::Cpu),
        ("nvenc", GpuEncoderPreference::Nvenc),
        ("quicksync", GpuEncoderPreference::QuickSync),
        ("amf", GpuEncoderPreference::Amf),
        ("vaapi", GpuEncoderPreference::Vaapi),
    ] {
        let out = std::env::temp_dir().join(format!("avbridge_test_gpu_encoder_{name}.mp4"));
        let cancel = AtomicBool::new(false);

        let segments = [clip(0.0, 0.35, 0.0, "")];
        let outcome =
            encode_timeline_export(&segments, CANVAS, &out, -14.0, preference, &cancel, |_| {})
                .unwrap();

        assert_eq!(
            outcome,
            EncodeOutcome::Completed,
            "preference {name:?} failed"
        );

        let info = probe(&out).unwrap();
        assert_eq!(info.kind, StreamKind::Video);

        let _ = std::fs::remove_file(&out);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn videotoolbox_encodes_a_real_timeline_on_macos() {
    let out = std::env::temp_dir().join("avbridge_test_videotoolbox_timeline.mp4");
    let cancel = AtomicBool::new(false);
    let segments = [clip(0.0, 0.35, 0.0, "")];

    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::VideoToolbox,
        &cancel,
        |_| {},
    )
    .unwrap();

    assert_eq!(outcome, EncodeOutcome::Completed);
    let info = probe(&out).unwrap();
    assert_eq!(info.kind, StreamKind::Video);
    assert_eq!(info.codec_name, "h264");

    let _ = std::fs::remove_file(&out);
}

/// Number of video frames in `path`, by decoding it.
fn frame_count(path: &std::path::Path) -> usize {
    let (mut session, _) = avbridge::DecodeSession::open(path).unwrap();
    let cancel = AtomicBool::new(false);
    let mut frames = 0;
    while let avbridge::Advance::Frame(_) = session.advance(&cancel).unwrap() {
        frames += 1;
    }
    frames
}

/// The export decodes with frame threading, which holds back a few frames until the decoder is
/// flushed. Segments that run to the end of their file must still come out whole: no frames lost
/// at the tail of each one.
#[test]
fn segments_that_run_to_the_end_of_their_file_keep_their_last_frames() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_tails.mp4");
    let cancel = AtomicBool::new(false);
    let source = fixture("video.mp4");
    let per_segment = frame_count(&source);

    // Past the end of the file: each segment is cut by its input ending, not by the out point.
    let segments: Vec<ClipSegment> = (0..4).map(|_| clip(0.0, 100.0, 0.0, "")).collect();
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let exported = frame_count(&out);
    // Frame rate conversion may add or drop a frame at a join; a lost decoder backlog costs
    // several frames per segment.
    assert!(
        exported + 4 >= 4 * per_segment,
        "exported {exported} frames of {} expected",
        4 * per_segment
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn multi_track_segments_that_run_to_the_end_of_their_file_keep_their_last_frames() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_multi_tails.mp4");
    let cancel = AtomicBool::new(false);
    let source = fixture("video.mp4");
    let per_segment = frame_count(&source);

    // A background of four whole-file segments plus a short overlay on a second track.
    let background: Vec<ClipSegment> = (0..4).map(|_| clip(0.0, 100.0, 0.0, "")).collect();
    let overlay = vec![clip(0.0, 0.2, 0.0, "")];
    let outcome = encode_timeline_export_multi(
        &[background, overlay],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let exported = frame_count(&out);
    // The multi-track compositor already drops frames where whole-file segments join (92 of 120
    // with a single-threaded decoder too); a lost decoder backlog on top of that gives about 32.
    assert!(
        exported >= 80,
        "exported {exported} frames of {}",
        4 * per_segment
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn audio_is_encoded_beside_the_video_and_follows_each_segments_gain() {
    // The audio graph and encoder run on their own thread: the export must still end with all of
    // its audio, mastered to the target, and never deadlock on the queue between the two.
    let out = std::env::temp_dir().join("avbridge_test_audio_worker.mp4");
    let cancel = AtomicBool::new(false);
    let segments = [
        clip(0.0, 0.5, 0.0, ""),
        clip(0.0, 0.5, -12.0, ""),
        clip(0.0, 0.5, 0.0, ""),
    ];
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let report = measure_loudness_json(&out).expect("the export has an audio stream");
    let integrated: f64 = report
        .split("\"input_i\"")
        .nth(1)
        .and_then(|rest| rest.split('"').nth(1))
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("no input_i in {report}"));
    assert!(
        integrated > -40.0 && integrated < -6.0,
        "integrated loudness {integrated} LUFS: audio missing or not mastered"
    );

    let _ = std::fs::remove_file(&out);
}

#[test]
fn each_segment_contributes_exactly_its_duration_in_frames() {
    // The canvas conform stage (`fps`) keeps its newest frame until the next one arrives: without
    // a flush at the end of each segment, every cut lost one frame, and the video fell a frame
    // further behind the audio at each of them.
    let out = std::env::temp_dir().join("avbridge_test_timeline_exact.mp4");
    let cancel = AtomicBool::new(false);
    let segments: Vec<ClipSegment> = (0..3).map(|_| clip(0.0, 1.0, 0.0, "")).collect();
    let outcome = encode_timeline_export(
        &segments,
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let per_second = (CANVAS.fps_num / CANVAS.fps_den) as usize;
    assert_eq!(frame_count(&out), 3 * per_second);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn multi_track_segments_contribute_exactly_their_duration_in_frames() {
    let out = std::env::temp_dir().join("avbridge_test_timeline_multi_exact.mp4");
    let cancel = AtomicBool::new(false);
    let background: Vec<ClipSegment> = (0..3).map(|_| clip(0.0, 1.0, 0.0, "")).collect();
    let overlay = vec![clip(0.0, 0.2, 0.0, "")];
    let outcome = encode_timeline_export_multi(
        &[background, overlay],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let per_second = (CANVAS.fps_num / CANVAS.fps_den) as usize;
    assert_eq!(frame_count(&out), 3 * per_second);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn an_overlay_covering_a_whole_segment_keeps_every_frame() {
    // The layers of an overlay graph were stamped with a frame counter in the source's time
    // base (1/15360 s for this file), so `fps` saw a whole second of video inside a millisecond
    // and dropped nearly all of it: this export had no video frames at all.
    let out = std::env::temp_dir().join("avbridge_test_timeline_overlay_exact.mp4");
    let cancel = AtomicBool::new(false);
    let background = vec![clip(0.0, 1.0, 0.0, "")];
    let overlay = vec![clip(0.0, 1.0, 0.0, "")];
    let outcome = encode_timeline_export_multi(
        &[background, overlay],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);

    let per_second = (CANVAS.fps_num / CANVAS.fps_den) as usize;
    assert_eq!(frame_count(&out), per_second);

    let _ = std::fs::remove_file(&out);
}

#[test]
fn multi_track_export_honours_a_segments_anchor() {
    // The single- and multi-track exports build a segment's filter graph with the same function
    // now; the multi-track one used to centre the clip whatever its anchor was.
    let out = std::env::temp_dir().join("avbridge_test_multi_anchor.mp4");
    let cancel = AtomicBool::new(false);
    let segment = ClipSegment {
        video_filter: "rotate=angle='0.3':ow=rotw('0.3'):oh=roth('0.3')".to_string(),
        anchor_x: 0.2,
        anchor_y: 0.8,
        ..clip(0.0, 1.0, 0.0, "")
    };
    let outcome = encode_timeline_export_multi(
        &[vec![segment]],
        CANVAS,
        &out,
        -14.0,
        GpuEncoderPreference::Cpu,
        &cancel,
        |_secs| {},
    )
    .unwrap();
    assert_eq!(outcome, EncodeOutcome::Completed);
    let per_second = (CANVAS.fps_num / CANVAS.fps_den) as usize;
    assert_eq!(frame_count(&out), per_second);

    let _ = std::fs::remove_file(&out);
}
