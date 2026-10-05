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

//! The post-processing passes of a single-track export (shape overlays, privacy blur), the
//! single-track resolver and the early validation of `render_export_job`.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use avcore::project::Sequence;
use avcore::render::{
    render_export_job, render_export_job_multi_with_audio, resolve_timeline_segments,
    PrivacyBlurSegment, RenderError, RenderOutcome,
};
use avcore::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};
use avcore::{probe_media, MediaAsset};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn temp(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("avcore_render_passes_{name}"))
}

fn video_asset(id: u64) -> MediaAsset {
    let path = fixture("video.mp4");
    probe_media(&path)
        .unwrap()
        .into_media_asset(id, "video.mp4".to_string(), path)
}

fn track(kind: TrackKind, clips: Vec<ClipInstance>) -> Track {
    Track {
        id: 1,
        name: "T".to_string(),
        kind,
        clips,
        text_clips: vec![],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    }
}

fn sequence(tracks: Vec<Track>) -> Sequence {
    Sequence {
        id: 1,
        name: "Sequence 1".to_string(),
        timeline: Timeline {
            tracks,
            playhead_secs: 0.0,
            markers: Vec::new(),
            multicam_groups: Vec::new(),
        },
        export_settings: Default::default(),
    }
}

fn segment() -> avbridge::ClipSegment {
    avbridge::ClipSegment {
        source_path: fixture("video.mp4"),
        source_in_secs: 0.0,
        source_out_secs: 0.5,
        gain_db: 0.0,
        video_filter: String::new(),
        frozen: false,
        speed_factor: 1.0,
        smooth_speed_ramp_end_factor: 0.0,
        position_x_expr: String::new(),
        position_y_expr: String::new(),
        transition_in: 0,
        transition_duration_secs: 0.0,
        timeline_start_secs: 0.0,
        mask_video_path: String::new(),
        blend_mode: String::new(),
        anchor_x: 0.5,
        anchor_y: 0.5,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
    }
}

fn canvas() -> avbridge::Canvas {
    avbridge::Canvas {
        width: 320,
        height: 240,
        fps_num: 30,
        fps_den: 1,
        bit_rate_bps: 500_000,
    }
}

#[test]
fn the_single_track_resolver_orders_clips_and_derives_the_canvas() {
    let asset = video_asset(1);
    let late = ClipInstance::new(2, 1, 1.0, 0.3, 0.6);
    let early = ClipInstance::new(1, 1, 0.0, 0.0, 0.3);
    let sequence = sequence(vec![track(TrackKind::Video, vec![late, early])]);

    let (segments, canvas) =
        resolve_timeline_segments(&sequence, std::slice::from_ref(&asset)).unwrap();

    assert_eq!(segments.len(), 2);
    assert_eq!(segments[0].timeline_start_secs, 0.0);
    assert_eq!(segments[1].timeline_start_secs, 1.0);
    assert_eq!((canvas.width, canvas.height), asset.resolution.unwrap());
    assert!(canvas.fps_num > 0 && canvas.fps_den > 0);
    assert_eq!(
        canvas.bit_rate_bps,
        (f64::from(asset.source_bitrate_mbps) * 1_000_000.0) as i64
    );
}

#[test]
fn the_single_track_resolver_reports_missing_pieces() {
    let asset = video_asset(1);
    let clip = || ClipInstance::new(1, 1, 0.0, 0.0, 0.3);

    // Audio tracks and empty video tracks do not count.
    let audio_only_timeline = sequence(vec![track(TrackKind::Audio, vec![clip()])]);
    assert!(matches!(
        resolve_timeline_segments(&audio_only_timeline, std::slice::from_ref(&asset)),
        Err(RenderError::EmptyTimeline)
    ));
    let empty_track = sequence(vec![track(TrackKind::Video, vec![])]);
    assert!(matches!(
        resolve_timeline_segments(&empty_track, std::slice::from_ref(&asset)),
        Err(RenderError::EmptyTimeline)
    ));
    let video = sequence(vec![track(TrackKind::Video, vec![clip()])]);
    assert!(matches!(
        resolve_timeline_segments(&video, &[]),
        Err(RenderError::MissingAsset)
    ));
    let mut audio_only = asset;
    audio_only.resolution = None;
    assert!(matches!(
        resolve_timeline_segments(&video, &[audio_only]),
        Err(RenderError::NoVideoStream)
    ));
}

#[test]
fn a_job_is_validated_before_the_encoder_runs() {
    let out = temp("never.mp4");
    let cancel = AtomicBool::new(false);
    let run = |segments: &[avbridge::ClipSegment], lufs: f32| {
        render_export_job(
            segments,
            canvas(),
            &out,
            lufs,
            avbridge::GpuEncoderPreference::Cpu,
            &[],
            &[],
            &[],
            &cancel,
            |_| {},
        )
    };
    assert!(matches!(run(&[], -14.0), Err(RenderError::EmptyTimeline)));
    assert!(matches!(
        run(&[segment()], f32::NAN),
        Err(RenderError::InvalidTargetLufs)
    ));
    assert!(!out.exists());
}

#[test]
fn overlay_and_privacy_passes_run_after_the_encode_and_failures_are_skipped() {
    let out = temp("passes.mp4");
    let matte = temp("matte.mp4");
    let _ = std::fs::remove_file(&out);
    // A 320x240 matte, white on the left half.
    let frame: Vec<u8> = (0..320 * 240)
        .map(|index| if index % 320 < 160 { 255 } else { 0 })
        .collect();
    avcore::encode_matte_video(&vec![frame; 30], 320, 240, 30, 1, &matte).unwrap();

    let identity = avbridge::ShapeSegment {
        filter_desc: "geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)'".to_string(),
    };
    let privacy = |mask_path: &Path, duration_secs: f64| PrivacyBlurSegment {
        mask_path: mask_path.display().to_string(),
        blur_sigma: 5.0,
        start_secs: 0.0,
        duration_secs,
    };
    let segments = vec![
        // Blurs through the matte.
        privacy(&matte, 0.5),
        // A matte that does not exist and a zero-length window are skipped.
        privacy(Path::new("missing-matte.mp4"), 0.5),
        privacy(&matte, 0.0),
    ];
    let outcome = render_export_job(
        &[segment()],
        canvas(),
        &out,
        -14.0,
        avbridge::GpuEncoderPreference::Cpu,
        &[],
        &[identity],
        &segments,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    let info = probe_media(&out).unwrap();
    assert_eq!(info.resolution, Some((320, 240)));
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&matte);
}

#[test]
fn a_matte_with_the_wrong_resolution_is_skipped() {
    let out = temp("wrong-matte.mp4");
    let matte = temp("small-matte.mp4");
    let _ = std::fs::remove_file(&out);
    avcore::encode_matte_video(&vec![vec![255u8; 64 * 48]; 10], 64, 48, 30, 1, &matte).unwrap();
    let segments = vec![PrivacyBlurSegment {
        mask_path: matte.display().to_string(),
        blur_sigma: 5.0,
        start_secs: 0.0,
        duration_secs: 0.3,
    }];
    let outcome = render_export_job(
        &[segment()],
        canvas(),
        &out,
        -14.0,
        avbridge::GpuEncoderPreference::Cpu,
        &[],
        &[],
        &segments,
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    let _ = std::fs::remove_file(&out);
    let _ = std::fs::remove_file(&matte);
}

fn audio_segment(path: PathBuf) -> avbridge::AudioSegment {
    avbridge::AudioSegment {
        source_path: path,
        source_in_secs: 0.0,
        source_out_secs: 0.4,
        timeline_start_secs: 0.0,
        gain_db: 0.0,
        speed_factor: 1.0,
        gain_keyframe_expr: String::new(),
        duck_role: 0,
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
    }
}

fn export_with_audio(
    out: &Path,
    audio: &[avbridge::AudioSegment],
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(u8),
) -> Result<RenderOutcome, RenderError> {
    render_export_job_multi_with_audio(
        &[vec![segment()]],
        audio,
        canvas(),
        out,
        -14.0,
        avbridge::GpuEncoderPreference::Cpu,
        &[],
        &[],
        &[],
        cancel,
        &mut on_progress,
    )
}

#[test]
fn an_audio_mix_pass_replaces_the_soundtrack_of_a_single_track_export() {
    let out = temp("with-audio.mp4");
    let _ = std::fs::remove_file(&out);
    let outcome = export_with_audio(
        &out,
        &[audio_segment(fixture("audio.m4a"))],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Completed);
    assert!(probe_media(&out).unwrap().resolution.is_some());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn a_missing_audio_source_and_a_cancel_during_the_mix_are_reported() {
    let out = temp("audio-failure.mp4");
    let _ = std::fs::remove_file(&out);
    let error = export_with_audio(
        &out,
        &[audio_segment(PathBuf::from("missing.m4a"))],
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap_err();
    assert!(matches!(error, RenderError::AudioMix(_)));
    let _ = std::fs::remove_file(&out);

    // Cancelling once the video is complete stops the mix and removes the export.
    let cancel = AtomicBool::new(false);
    let outcome = export_with_audio(
        &out,
        &[audio_segment(fixture("audio.m4a"))],
        &cancel,
        |percent| {
            if percent == 100 {
                cancel.store(true, std::sync::atomic::Ordering::Release);
            }
        },
    )
    .unwrap();
    assert_eq!(outcome, RenderOutcome::Cancelled);
    assert!(!out.exists());
}
