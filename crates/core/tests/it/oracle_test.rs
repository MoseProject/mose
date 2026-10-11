// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Checks against an independent reference: the `ffmpeg` and `ffprobe` programs, run as separate
//! processes. The other export tests read their results back through the same libraries that
//! wrote them, which can agree with each other and still be wrong; these ask a second opinion.
//!
//! They need the two programs (on `PATH`, or in `$FFMPEG_DIR/bin`). Without them each test says
//! so and passes, unless `OCA_REQUIRE_ORACLES` is set (CI should set it), which turns that into a
//! failure so the checks cannot be silently skipped.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use avcore::render::{render_export, render_timeline_export, RenderOutcome};
use avcore::timeline::{AudioRole, Track, TrackKind};
use avcore::{probe_media, MediaAsset, MediaKind};
use serde_json::Value;

use crate::timeline_export_test::{clip, sequence_with};

fn tool(name: &str) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FFMPEG_DIR") {
        let candidate = Path::new(&dir).join("bin").join(name);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let probe = Command::new(name).arg("-version").output().ok()?;
    probe.status.success().then(|| PathBuf::from(name))
}

/// Both programs, or `None` after saying the test was skipped (or panicking when required).
fn oracles() -> Option<(PathBuf, PathBuf)> {
    match (tool("ffmpeg"), tool("ffprobe")) {
        (Some(ffmpeg), Some(ffprobe)) => Some((ffmpeg, ffprobe)),
        _ => {
            assert!(
                std::env::var_os("OCA_REQUIRE_ORACLES").is_none(),
                "OCA_REQUIRE_ORACLES is set but ffmpeg and ffprobe were not found"
            );
            eprintln!("skipped: ffmpeg and ffprobe are not available");
            None
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oca-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(name)
}

/// A clip made by `ffmpeg` itself: 640x360 at 30 fps, a 440 Hz tone, `seconds` long, with the
/// video at roughly `video_kbps`.
fn generate(ffmpeg: &Path, name: &str, seconds: u32, video_kbps: u32) -> PathBuf {
    let output = scratch(name);
    let status = Command::new(ffmpeg)
        .args(["-v", "error", "-y"])
        .args(["-f", "lavfi", "-i"])
        .arg(format!("testsrc2=size=640x360:rate=30:duration={seconds}"))
        .args(["-f", "lavfi", "-i"])
        .arg(format!(
            "sine=frequency=440:sample_rate=48000:duration={seconds}"
        ))
        .args(["-af", "volume=0.05"])
        // MPEG-4 part 2 is in every FFmpeg build; libx264 is not in the LGPL ones CI uses.
        .args(["-c:v", "mpeg4", "-pix_fmt", "yuv420p"])
        .arg("-b:v")
        .arg(format!("{video_kbps}k"))
        .args(["-c:a", "aac", "-b:a", "128k", "-shortest"])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success(), "ffmpeg could not make the test clip");
    output
}

fn ffprobe(ffprobe: &Path, file: &Path) -> Value {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "error",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(file)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ffprobe failed on {}",
        file.display()
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn stream<'a>(report: &'a Value, kind: &str) -> &'a Value {
    report["streams"]
        .as_array()
        .unwrap()
        .iter()
        .find(|stream| stream["codec_type"] == kind)
        .unwrap_or_else(|| panic!("no {kind} stream in {report}"))
}

fn number(value: &Value) -> f64 {
    match value {
        Value::String(text) => text.parse().unwrap(),
        other => other.as_f64().unwrap(),
    }
}

fn fraction(text: &str) -> f64 {
    let (num, den) = text.split_once('/').unwrap();
    num.parse::<f64>().unwrap() / den.parse::<f64>().unwrap()
}

/// Integrated loudness (LUFS) and true peak (dBTP) as `ffmpeg`'s own ebur128 filter reports them.
fn ebur128(ffmpeg: &Path, file: &Path) -> (f64, f64) {
    let output = Command::new(ffmpeg)
        .args(["-nostats", "-hide_banner", "-i"])
        .arg(file)
        .args(["-af", "ebur128=peak=true", "-f", "null", "-"])
        .output()
        .unwrap();
    let log = String::from_utf8_lossy(&output.stderr);
    let summary = log.rsplit("Summary:").next().unwrap();
    let value = |label: &str, unit: &str| -> f64 {
        let line = summary
            .lines()
            .find(|line| line.trim_start().starts_with(label))
            .unwrap_or_else(|| panic!("no `{label}` in the ebur128 summary:\n{summary}"));
        let text = line.split(label).nth(1).unwrap().trim();
        text.trim_end_matches(unit).trim().parse().unwrap()
    };
    (value("I:", "LUFS"), value("Peak:", "dBFS"))
}

fn media(file: &Path, id: u64) -> MediaAsset {
    probe_media(file).unwrap().into_media_asset(
        id,
        file.file_name().unwrap().to_string_lossy().into_owned(),
        file.to_path_buf(),
    )
}

#[test]
fn probing_agrees_with_ffprobe() {
    let Some((ffmpeg, ffprobe_bin)) = oracles() else {
        return;
    };
    let file = generate(&ffmpeg, "probe.mp4", 6, 800);
    let ours = probe_media(&file).unwrap();
    let theirs = ffprobe(&ffprobe_bin, &file);
    let video = stream(&theirs, "video");
    let audio = stream(&theirs, "audio");

    assert!((ours.duration_secs - number(&theirs["format"]["duration"])).abs() < 0.05);
    assert_eq!(
        ours.resolution,
        Some((
            number(&video["width"]) as u32,
            number(&video["height"]) as u32
        ))
    );
    assert!(
        (f64::from(ours.fps.unwrap()) - fraction(video["r_frame_rate"].as_str().unwrap())).abs()
            < 0.01
    );
    assert_eq!(ours.codec, video["codec_name"].as_str().unwrap());
    assert!(ours.has_audio);
    // The probe reads the sample rate only from audio-only files; a video file reports none.
    assert_eq!(ours.sample_rate_khz, None);

    let audio_only = scratch("probe_audio.m4a");
    let status = Command::new(&ffmpeg)
        .args(["-y", "-loglevel", "error", "-i"])
        .arg(&file)
        .args(["-vn", "-c:a", "aac"])
        .arg(&audio_only)
        .status()
        .unwrap();
    assert!(status.success());
    let ours = probe_media(&audio_only).unwrap();
    let theirs = ffprobe(&ffprobe_bin, &audio_only);
    assert_eq!(ours.kind, MediaKind::Audio);
    assert!(
        (f64::from(ours.sample_rate_khz.unwrap()) * 1000.0
            - number(&stream(&theirs, "audio")["sample_rate"]))
        .abs()
            < 1.0
    );
    let _ = std::fs::remove_file(audio_only);
    let _ = std::fs::remove_file(file);
}

#[test]
fn an_export_has_the_streams_and_the_length_of_its_source() {
    let Some((ffmpeg, ffprobe_bin)) = oracles() else {
        return;
    };
    let source = generate(&ffmpeg, "export_src.mp4", 8, 1200);
    let output = scratch("export_out.mp4");
    let outcome = render_export(
        &source,
        &output,
        -14.0,
        8.0,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);

    let before = ffprobe(&ffprobe_bin, &source);
    let after = ffprobe(&ffprobe_bin, &output);
    let (video, audio) = (stream(&after, "video"), stream(&after, "audio"));
    // The video is stream-copied, so it keeps the source codec; only the audio is re-encoded.
    assert_eq!(video["codec_name"], stream(&before, "video")["codec_name"]);
    assert_eq!(audio["codec_name"], "aac");
    assert_eq!(video["width"], stream(&before, "video")["width"]);
    assert_eq!(video["height"], stream(&before, "video")["height"]);
    let (a, b) = (
        number(&before["format"]["duration"]),
        number(&after["format"]["duration"]),
    );
    assert!((a - b).abs() < 0.15, "source {a:.3}s, export {b:.3}s");
    let _ = (std::fs::remove_file(source), std::fs::remove_file(output));
}

#[test]
fn the_export_hits_the_loudness_target_as_ffmpeg_measures_it() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    let source = generate(&ffmpeg, "loud_src.mp4", 12, 800);
    let (source_lufs, _) = ebur128(&ffmpeg, &source);
    for target in [-14.0_f32, -16.0, -23.0] {
        let output = scratch(&format!("loud_out_{}.mp4", target.abs()));
        let outcome = render_export(
            &source,
            &output,
            target,
            12.0,
            &AtomicBool::new(false),
            |_| {},
        )
        .unwrap();
        assert_eq!(outcome, RenderOutcome::Completed);
        let (lufs, peak) = ebur128(&ffmpeg, &output);
        eprintln!("target {target}: source {source_lufs:.1} LUFS -> export {lufs:.2} LUFS, peak {peak:.2}");
        assert!(
            (lufs - f64::from(target)).abs() <= 1.0,
            "asked for {target} LUFS, ffmpeg measures {lufs:.2}"
        );
        assert!(peak <= -0.9, "the export peaks at {peak:.2} dBFS");
        let _ = std::fs::remove_file(output);
    }
    let _ = std::fs::remove_file(source);
}

#[test]
fn a_cut_timeline_is_exactly_as_long_as_its_clips() {
    let Some((ffmpeg, ffprobe_bin)) = oracles() else {
        return;
    };
    let source = generate(&ffmpeg, "cut_src.mp4", 12, 800);
    let asset = media(&source, 1);
    // Two pieces of the source, 1.0 to 3.5 s and 6.0 to 8.0 s, back to back.
    let first = clip(1, 1, 0.0, 1.0, 3.5);
    let second = clip(2, 1, 2.5, 6.0, 8.0);
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![first, second],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let output = scratch("cut_out.mp4");
    let outcome = render_timeline_export(
        &sequence_with(vec![track]),
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);

    let report = ffprobe(&ffprobe_bin, &output);
    let duration = number(&report["format"]["duration"]);
    let frames = Command::new(&ffprobe_bin)
        .args(["-v", "error", "-count_frames", "-select_streams", "v:0"])
        .args(["-show_entries", "stream=nb_read_frames", "-of", "csv=p=0"])
        .arg(&output)
        .output()
        .unwrap();
    let frames: f64 = String::from_utf8_lossy(&frames.stdout)
        .trim()
        .parse()
        .unwrap();
    eprintln!("cut export: {duration:.3} s, {frames} frames");
    assert!(
        (duration - 4.5).abs() <= 0.15,
        "expected 4.5 s of clips, ffprobe says {duration:.3}"
    );
    assert!(
        (frames - 4.5 * 30.0).abs() <= 3.0,
        "expected about 135 frames, counted {frames}"
    );
    let _ = (std::fs::remove_file(source), std::fs::remove_file(output));
}

/// A timeline of one clip taken from `source_in` to `source_out`, exported, and what ffprobe
/// says about each stream's start and end.
fn export_one_clip(
    ffmpeg: &Path,
    ffprobe_bin: &Path,
    source_in: f64,
    source_out: f64,
) -> (f64, f64, f64, f64) {
    let source = generate(ffmpeg, &format!("one_src_{source_in}.mp4"), 12, 800);
    let asset = media(&source, 1);
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 1, 0.0, source_in, source_out)],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let output = scratch(&format!("one_out_{source_in}.mp4"));
    let outcome = render_timeline_export(
        &sequence_with(vec![track]),
        &[asset],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    let report = ffprobe(ffprobe_bin, &output);
    let (video, audio) = (stream(&report, "video"), stream(&report, "audio"));
    let result = (
        number(&video["start_time"]),
        number(&video["start_time"]) + number(&video["duration"]),
        number(&audio["start_time"]),
        number(&audio["start_time"]) + number(&audio["duration"]),
    );
    let _ = (std::fs::remove_file(source), std::fs::remove_file(output));
    result
}

#[test]
fn audio_and_video_start_and_end_together_whatever_the_in_point() {
    let Some((ffmpeg, ffprobe_bin)) = oracles() else {
        return;
    };
    for source_in in [0.0, 1.0, 5.0] {
        let (video_start, video_end, audio_start, audio_end) =
            export_one_clip(&ffmpeg, &ffprobe_bin, source_in, source_in + 3.0);
        eprintln!(
            "in point {source_in}: video {video_start:.3}..{video_end:.3}, audio {audio_start:.3}..{audio_end:.3}"
        );
        assert!(
            (audio_start - video_start).abs() < 0.1,
            "in point {source_in}: audio starts at {audio_start:.3} s, video at {video_start:.3} s"
        );
        assert!(
            (audio_end - video_end).abs() < 0.15,
            "in point {source_in}: audio ends at {audio_end:.3} s, video at {video_end:.3} s"
        );
    }
}

/// A one-second solid-colour clip with a quiet tone.
fn solid(ffmpeg: &Path, name: &str, colour: &str) -> PathBuf {
    let output = scratch(name);
    let status = Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg(format!("color=c={colour}:s=320x180:r=30:d=1"))
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:d=1",
        ])
        .args([
            "-c:v",
            "mpeg4",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&output)
        .status()
        .unwrap();
    assert!(status.success());
    output
}

/// The RGB of the pixel at (`x`, `y`) in frame `frame` of `file`, as `ffmpeg` decodes it.
fn pixel(ffmpeg: &Path, file: &Path, frame: u32, x: u32, y: u32) -> [u8; 3] {
    let output = Command::new(ffmpeg)
        .args(["-v", "error", "-i"])
        .arg(file)
        .arg("-vf")
        .arg(format!(
            "select=eq(n\\,{frame}),format=rgb24,crop=1:1:{x}:{y}"
        ))
        .args(["-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"])
        .output()
        .unwrap();
    assert_eq!(
        output.stdout.len(),
        3,
        "no frame {frame} in the export: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    [output.stdout[0], output.stdout[1], output.stdout[2]]
}

/// A red clip underneath, a green one above with `shape` applied to it.
fn red_under_green(
    ffmpeg: &Path,
    label: &str,
    tweak: impl FnOnce(&mut avcore::timeline::ClipInstance),
) -> PathBuf {
    let red = solid(ffmpeg, &format!("{label}_red.mp4"), "red");
    let green = solid(ffmpeg, &format!("{label}_green.mp4"), "green");
    let mut top = clip(2, 2, 0.0, 0.0, 1.0);
    tweak(&mut top);
    let track = |id: u64, clips| Track {
        id,
        name: format!("V{id}"),
        kind: TrackKind::Video,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let sequence = sequence_with(vec![
        track(1, vec![clip(1, 1, 0.0, 0.0, 1.0)]),
        track(2, vec![top]),
    ]);
    let output = scratch(&format!("{label}_out.mp4"));
    let outcome = render_timeline_export(
        &sequence,
        &[media(&red, 1), media(&green, 2)],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    output
}

fn is_close(got: [u8; 3], want: [u8; 3], tolerance: i32) -> bool {
    got.iter()
        .zip(want)
        .all(|(g, w)| (i32::from(*g) - i32::from(w)).abs() <= tolerance)
}

#[test]
fn a_circle_mask_shows_what_is_underneath_outside_the_circle() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    let out = red_under_green(&ffmpeg, "circle", |c| {
        c.mask_shape = avcore::timeline::MaskShape::Circle;
    });
    let centre = pixel(&ffmpeg, &out, 15, 160, 90);
    let corner = pixel(&ffmpeg, &out, 15, 4, 4);
    // Green (0,128,0) at the centre, red underneath in the corner.
    assert!(is_close(centre, [0, 128, 0], 40), "centre {centre:?}");
    assert!(is_close(corner, [255, 0, 0], 40), "corner {corner:?}");
}

#[test]
fn a_rounded_rectangle_mask_cuts_the_corners_only() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    let out = red_under_green(&ffmpeg, "rounded", |c| {
        c.mask_shape = avcore::timeline::MaskShape::RoundedRect;
        c.mask_corner_radius = 0.5;
    });
    let edge_middle = pixel(&ffmpeg, &out, 15, 160, 4);
    let corner = pixel(&ffmpeg, &out, 15, 3, 3);
    assert!(
        is_close(edge_middle, [0, 128, 0], 40),
        "edge {edge_middle:?}"
    );
    assert!(is_close(corner, [255, 0, 0], 40), "corner {corner:?}");
}

#[test]
fn animated_opacity_blends_the_layer_into_what_is_underneath() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    let out = red_under_green(&ffmpeg, "opacity", |c| {
        c.opacity_keyframes = vec![
            avcore::Keyframe {
                time_fraction: 0.0,
                value: 0.0,
            },
            avcore::Keyframe {
                time_fraction: 1.0,
                value: 1.0,
            },
        ];
    });
    let early = pixel(&ffmpeg, &out, 1, 160, 90);
    let late = pixel(&ffmpeg, &out, 28, 160, 90);
    assert!(
        early[0] > 200 && early[1] < 60,
        "start is still the red layer: {early:?}"
    );
    assert!(
        late[1] > 100 && late[0] < 60,
        "end is the green layer: {late:?}"
    );
}

#[test]
fn animated_brightness_brightens_the_picture_over_the_clip() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    let grey = solid(&ffmpeg, "brightness_grey.mp4", "0x606060");
    let mut ramped = clip(1, 1, 0.0, 0.0, 1.0);
    ramped.brightness_keyframes = vec![
        avcore::Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        avcore::Keyframe {
            time_fraction: 1.0,
            value: 0.4,
        },
    ];
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![ramped],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let output = scratch("brightness_out.mp4");
    let outcome = render_timeline_export(
        &sequence_with(vec![track]),
        &[media(&grey, 1)],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    let early = pixel(&ffmpeg, &output, 1, 160, 90);
    let middle = pixel(&ffmpeg, &output, 15, 160, 90);
    let late = pixel(&ffmpeg, &output, 28, 160, 90);
    eprintln!("brightness ramp: {early:?} {middle:?} {late:?}");
    // 0.4 of full scale is about 100 levels by the last frame, and half that in the middle.
    assert!(
        i32::from(late[0]) - i32::from(early[0]) > 80,
        "{early:?} -> {late:?}"
    );
    assert!(
        (i32::from(middle[0]) - i32::from(early[0]) - 50).abs() < 20,
        "{early:?} -> {middle:?}"
    );
}

#[test]
fn an_animated_zoom_in_magnifies_the_picture_about_its_centre() {
    let Some((ffmpeg, _)) = oracles() else {
        return;
    };
    // Red on the left three quarters, green on the right quarter: the boundary is at x = 240 of 320.
    let split = scratch("zoom_split.mp4");
    let status = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
        .arg("color=c=red:s=320x180:r=30:d=1,drawbox=x=240:y=0:w=80:h=180:color=green:t=fill")
        .args([
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:d=1",
        ])
        .args([
            "-c:v",
            "mpeg4",
            "-pix_fmt",
            "yuv420p",
            "-c:a",
            "aac",
            "-shortest",
        ])
        .arg(&split)
        .status()
        .unwrap();
    assert!(status.success());
    let mut zoomed = clip(1, 1, 0.0, 0.0, 1.0);
    zoomed.scale_keyframes = vec![
        avcore::Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        avcore::Keyframe {
            time_fraction: 1.0,
            value: 2.0,
        },
    ];
    let track = Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![zoomed],
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    };
    let output = scratch("zoom_out.mp4");
    let outcome = render_timeline_export(
        &sequence_with(vec![track]),
        &[media(&split, 1)],
        &output,
        -14.0,
        avcore::GpuEncoderPreference::Cpu,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    // The boundary sits at 160 + 80 * zoom: x = 240 at the start, 280 half-way, off the frame at the end.
    let start = pixel(&ffmpeg, &output, 0, 300, 90);
    let early_green = pixel(&ffmpeg, &output, 0, 250, 90);
    let middle_red = pixel(&ffmpeg, &output, 15, 270, 90);
    let middle_green = pixel(&ffmpeg, &output, 15, 292, 90);
    let end = pixel(&ffmpeg, &output, 29, 300, 90);
    assert!(is_close(start, [0, 128, 0], 50), "start {start:?}");
    assert!(
        is_close(early_green, [0, 128, 0], 50),
        "early {early_green:?}"
    );
    assert!(
        is_close(middle_red, [255, 0, 0], 50),
        "middle left {middle_red:?}"
    );
    assert!(
        is_close(middle_green, [0, 128, 0], 50),
        "middle right {middle_green:?}"
    );
    assert!(is_close(end, [255, 0, 0], 50), "end {end:?}");
}
