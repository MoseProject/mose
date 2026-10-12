use super::*;
use crate::timeline::{TextFontFamily, TextFontStyle};
use avbridge::Canvas;

fn canvas() -> Canvas {
    Canvas {
        width: 1920,
        height: 1080,
        fps_num: 30,
        fps_den: 1,
        bit_rate_bps: 8_000_000,
    }
}

fn segment() -> avbridge::ClipSegment {
    avbridge::ClipSegment {
        source_path: "fixture.mp4".into(),
        source_in_secs: 0.0,
        source_out_secs: 2.0,
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

fn text_segment() -> TextSegment {
    TextSegment {
        start_secs: 0.0,
        duration_secs: 2.0,
        text: "caption".to_string(),
        font_size: 32.0,
        font_family: TextFontFamily::Lato,
        font_style: TextFontStyle::Regular,
        font_weight: None,
        color_rgba: [255, 255, 255, 255],
        background_rgba: [0, 0, 0, 0],
        background_padding: 0.0,
        background_corner_radius: 0.0,
        glyph_byte_range: None,
        pos_x: 0.0,
        pos_y: 0.0,
        opacity_keyframe_expr: String::new(),
        position_keyframe_expr_x: String::new(),
        position_keyframe_expr_y: String::new(),
        scale_keyframe_expr_x: String::new(),
        scale_keyframe_expr_y: String::new(),
        rotation_keyframe_expr_x: String::new(),
        rotation_keyframe_expr_y: String::new(),
        direction: crate::timeline::TextDirection::Auto,
        text_align: crate::timeline::TextAlign::Auto,
    }
}

fn audio_segment() -> avbridge::AudioSegment {
    avbridge::AudioSegment {
        source_path: "fixture.wav".into(),
        source_in_secs: 0.0,
        source_out_secs: 2.0,
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
        pan: 0.0,
    }
}

#[test]
fn render_validation_accepts_a_complete_segment() {
    assert!(validate_segment_list(&[segment()], canvas(), -16.0).is_ok());
}

#[test]
fn render_validation_rejects_invalid_native_boundary_inputs() {
    let cases = [
        ("empty source path", {
            let mut value = segment();
            value.source_path.clear();
            value
        }),
        ("reversed source range", {
            let mut value = segment();
            value.source_out_secs = 0.0;
            value
        }),
        ("nonfinite speed", {
            let mut value = segment();
            value.speed_factor = f32::NAN;
            value
        }),
        ("negative timeline start", {
            let mut value = segment();
            value.timeline_start_secs = -0.1;
            value
        }),
        ("out of range anchor", {
            let mut value = segment();
            value.anchor_x = 1.1;
            value
        }),
    ];

    for (name, value) in cases {
        assert!(
            matches!(
                validate_segment_list(&[value], canvas(), -16.0),
                Err(RenderError::InvalidTimelineSegment)
            ),
            "{name}"
        );
    }
}

#[test]
fn render_validation_rejects_invalid_canvas_and_loudness_target() {
    let mut invalid_canvas = canvas();
    invalid_canvas.fps_den = 0;
    assert!(matches!(
        validate_segment_list(&[segment()], invalid_canvas, -16.0),
        Err(RenderError::InvalidCanvas)
    ));
    assert!(matches!(
        validate_segment_list(&[segment()], canvas(), f32::INFINITY),
        Err(RenderError::InvalidTargetLufs)
    ));
}

#[test]
fn render_validation_rejects_text_that_cannot_be_shaped() {
    let mut invalid = text_segment();
    invalid.text = "a".repeat(crate::text_layout::MAX_TEXT_LAYOUT_BYTES + 1);
    assert!(matches!(
        validate_text_segments(&[invalid], canvas()),
        Err(RenderError::InvalidTextLayout)
    ));
}

#[test]
fn render_validation_rejects_invalid_audio_mix_segments() {
    let mut invalid = audio_segment();
    invalid.voice_cleanup_ceiling_linear = f32::NAN;
    assert!(matches!(
        validate_audio_segments(&[invalid]),
        Err(RenderError::InvalidAudioSegment)
    ));
}

#[test]
fn audio_mix_timeline_duration_respects_smooth_speed_ramps() {
    let mut ramp = segment();
    ramp.source_out_secs = 4.0;
    ramp.speed_factor = 1.0;
    ramp.smooth_speed_ramp_end_factor = 2.0;

    let duration = timeline_duration(&[vec![ramp.clone()]]);
    assert!((duration - clip_segment_duration_secs(&ramp)).abs() < 1e-9);
    assert!(
        duration < 4.0,
        "a ramp from 1x to 2x must shorten the timeline"
    );
}

#[test]
fn render_job_rejects_invalid_segments_before_entering_the_native_encoder() {
    let mut invalid = segment();
    invalid.speed_factor = 0.0;

    assert!(matches!(
        render_export_job(
            &[invalid],
            canvas(),
            std::path::Path::new("not-created.mp4"),
            -16.0,
            avbridge::GpuEncoderPreference::Auto,
            &[],
            &[],
            &[],
            &std::sync::atomic::AtomicBool::new(false),
            |_| {},
        ),
        Err(RenderError::InvalidTimelineSegment)
    ));
}

#[test]
fn fps_to_rational_uses_whole_number_over_one_for_an_integer_fps() {
    assert_eq!(fps_to_rational(30.0), (30, 1));
    assert_eq!(fps_to_rational(60.0), (60, 1));
    assert_eq!(fps_to_rational(24.0), (24, 1));
}

#[test]
fn fps_to_rational_uses_a_fixed_point_fraction_for_a_non_integer_fps() {
    // 29.97 is not within 0.001 of a whole number, so it takes the /1000 fraction path.
    assert_eq!(fps_to_rational(29.97), (29970, 1000));
    assert_eq!(fps_to_rational(23.976), (23976, 1000));
}

#[test]
fn fps_to_rational_treats_a_value_within_0_001_of_whole_as_whole() {
    assert_eq!(fps_to_rational(30.0004), (30, 1));
    assert_eq!(fps_to_rational(29.9996), (30, 1));
}

#[test]
fn fps_to_rational_falls_back_to_30_1_for_zero_or_negative_fps() {
    assert_eq!(fps_to_rational(0.0), (30, 1));
    assert_eq!(fps_to_rational(-5.0), (30, 1));
}

#[test]
fn fps_to_rational_falls_back_to_30_1_for_nonfinite_fps() {
    assert_eq!(fps_to_rational(f32::INFINITY), (30, 1));
    assert_eq!(fps_to_rational(f32::NAN), (30, 1));
}

#[test]
fn every_render_error_has_a_distinct_readable_message() {
    let errors = vec![
        RenderError::OpenInput,
        RenderError::StreamInfo,
        RenderError::AllocOutput,
        RenderError::NewStream,
        RenderError::OpenOutput,
        RenderError::WriteHeader,
        RenderError::WriteFrame,
        RenderError::NoAudioStream,
        RenderError::Decoder,
        RenderError::FilterGraph,
        RenderError::Encoder,
        RenderError::Pipeline,
        RenderError::Unknown(42),
        RenderError::NoVideoStream,
        RenderError::AudioFormatMismatch,
        RenderError::EmptyTimeline,
        RenderError::MissingAsset,
        RenderError::InvalidCanvas,
        RenderError::InvalidTimelineSegment,
        RenderError::InvalidTargetLufs,
        RenderError::InvalidTextLayout,
        RenderError::InvalidAudioSegment,
        RenderError::AudioMix("mix".to_string()),
        RenderError::MediaMux("mux".to_string()),
        RenderError::ReplaceOutput(std::io::Error::other("replace")),
        RenderError::MissingNestedSequence,
        RenderError::CyclicNestedSequence,
        RenderError::ProbeNestedSequence,
    ];
    let messages: std::collections::HashSet<String> =
        errors.iter().map(|error| error.to_string()).collect();
    assert_eq!(messages.len(), errors.len());
    assert!(RenderError::Unknown(42).to_string().contains("42"));
    assert!(RenderError::AudioMix("mix".to_string())
        .to_string()
        .contains("mix"));
    assert!(RenderError::MediaMux("mux".to_string())
        .to_string()
        .contains("mux"));
    assert!(RenderError::ReplaceOutput(std::io::Error::other("replace"))
        .to_string()
        .contains("replace"));
}

#[test]
fn every_avbridge_encode_error_maps_to_its_render_error() {
    use avbridge::EncodeError as E;
    let invalid_path = std::ffi::CString::new("a b").unwrap_err();
    assert!(matches!(
        RenderError::from(E::InvalidPath(invalid_path)),
        RenderError::OpenInput
    ));
    assert!(matches!(
        RenderError::from(E::OpenInput),
        RenderError::OpenInput
    ));
    assert!(matches!(
        RenderError::from(E::StreamInfo),
        RenderError::StreamInfo
    ));
    assert!(matches!(
        RenderError::from(E::AllocOutput),
        RenderError::AllocOutput
    ));
    assert!(matches!(
        RenderError::from(E::NewStream),
        RenderError::NewStream
    ));
    assert!(matches!(
        RenderError::from(E::OpenOutput),
        RenderError::OpenOutput
    ));
    assert!(matches!(
        RenderError::from(E::WriteHeader),
        RenderError::WriteHeader
    ));
    assert!(matches!(
        RenderError::from(E::WriteFrame),
        RenderError::WriteFrame
    ));
    assert!(matches!(
        RenderError::from(E::NoAudioStream),
        RenderError::NoAudioStream
    ));
    assert!(matches!(
        RenderError::from(E::Decoder),
        RenderError::Decoder
    ));
    assert!(matches!(
        RenderError::from(E::FilterGraph),
        RenderError::FilterGraph
    ));
    assert!(matches!(
        RenderError::from(E::Encoder),
        RenderError::Encoder
    ));
    assert!(matches!(
        RenderError::from(E::Pipeline),
        RenderError::Pipeline
    ));
    assert!(matches!(
        RenderError::from(E::NoVideoStream),
        RenderError::NoVideoStream
    ));
    assert!(matches!(
        RenderError::from(E::AudioFormatMismatch),
        RenderError::AudioFormatMismatch
    ));
    assert!(matches!(
        RenderError::from(E::EmptyTimeline),
        RenderError::EmptyTimeline
    ));
    assert!(matches!(
        RenderError::from(E::Unknown(7)),
        RenderError::Unknown(7)
    ));
}

/// How resolving a sequence into export segments grows with a big project. Ignored (it prints
/// timings): `cargo test --release -p core --lib resolve_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn resolve_cost() {
    use crate::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};
    use crate::{MediaAsset, MediaKind, Sequence};
    use std::time::Instant;

    let media: Vec<MediaAsset> = (1..=100)
        .map(|id| MediaAsset {
            id,
            file_name: format!("asset-{id}"),
            source_path: format!("asset-{id}.mp4").into(),
            kind: MediaKind::Video,
            has_audio: true,
            duration_secs: 600.0,
            codec: "h264".into(),
            source_bitrate_mbps: 8.0,
            resolution: Some((1920, 1080)),
            fps: Some(60.0),
            sample_rate_khz: None,
            loudness: None,
            proxy_path: None,
            waveform_peaks: None,
            favorited: false,
            streams: Default::default(),
            hdr_handling: Default::default(),
            audio_stream: None,
        })
        .collect();
    for (track_count, clips_per_track) in [(1usize, 400usize), (10, 400), (40, 400)] {
        let mut next_id = 1;
        let tracks = (0..track_count)
            .map(|t| Track {
                id: t as u64 + 1,
                name: format!("V{t}"),
                kind: TrackKind::Video,
                clips: (0..clips_per_track)
                    .map(|c| {
                        next_id += 1;
                        ClipInstance::new(next_id, 1 + next_id % 100, c as f64 * 5.0, 0.0, 5.0)
                    })
                    .collect(),
                text_clips: vec![],
                shape_clips: vec![],
                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                locked: false,
                mix: Default::default(),
                color_label: None,
            })
            .collect();
        let sequence = Sequence {
            id: 1,
            name: "s".into(),
            timeline: Timeline {
                tracks,
                playhead_secs: 0.0,
                markers: vec![],
                multicam_groups: vec![],
            },
            export_settings: Default::default(),
        };
        let started = Instant::now();
        let video = resolve_timeline_segments_multi(&sequence, &media).unwrap();
        let video_ms = started.elapsed().as_secs_f64() * 1e3;
        let started = Instant::now();
        let audio = resolve_audio_segments(&sequence, &media).unwrap();
        eprintln!(
            "{track_count} tracks x {clips_per_track} clips: video segments {video_ms:.1} ms ({} layers), audio segments {:.1} ms ({} segments)",
            video.0.len(),
            started.elapsed().as_secs_f64() * 1e3,
            audio.len()
        );
    }
}

/// Exports `fixture` (a flat colour clip under `tests/fixtures/color`) with `change` applied to
/// its asset first, and returns the centre pixel of the exported file, decoded the way the
/// preview decodes it.
fn exported_centre_pixel(fixture: &str, change: impl FnOnce(&mut crate::MediaAsset)) -> [u8; 3] {
    exported_centre_pixel_on_tracks(fixture, 1, change)
}

/// [`exported_centre_pixel`] with the same clip stacked on `tracks` video tracks, which makes the
/// export composite layers instead of rendering one track.
fn exported_centre_pixel_on_tracks(
    fixture: &str,
    tracks: u64,
    change: impl FnOnce(&mut crate::MediaAsset),
) -> [u8; 3] {
    let output = exported_file_on_tracks(fixture, tracks, change);
    let frame = crate::FrameSampler::open(&output)
        .expect("opens the export")
        .sample(0.0, std::time::Duration::from_secs(5))
        .expect("a frame");
    let _ = std::fs::remove_file(&output);
    let at = ((frame.height / 2 * frame.width + frame.width / 2) * 4) as usize;
    [frame.rgba[at], frame.rgba[at + 1], frame.rgba[at + 2]]
}

/// The exported file of `fixture`, for the caller to read and delete.
fn exported_file(fixture: &str) -> std::path::PathBuf {
    exported_file_on_tracks(fixture, 1, |_| {})
}

fn exported_file_on_tracks(
    fixture: &str,
    tracks: u64,
    change: impl FnOnce(&mut crate::MediaAsset),
) -> std::path::PathBuf {
    use crate::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};
    use crate::Sequence;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/color")
        .join(fixture);
    let mut asset = crate::probe_media(&source)
        .expect("probes")
        .into_media_asset(1, fixture.into(), source);
    change(&mut asset);
    let sequence = Sequence {
        id: 1,
        name: "s".into(),
        timeline: Timeline {
            tracks: (1..=tracks)
                .map(|id| Track {
                    id,
                    name: format!("V{id}"),
                    kind: TrackKind::Video,
                    clips: vec![ClipInstance::new(id, 1, 0.0, 0.0, 0.4)],
                    text_clips: vec![],
                    shape_clips: vec![],
                    visible: true,
                    audio_role: AudioRole::Unspecified,
                    muted: false,
                    solo: false,
                    locked: false,
                    mix: Default::default(),
                    color_label: None,
                })
                .collect(),
            playhead_secs: 0.0,
            markers: vec![],
            multicam_groups: vec![],
        },
        export_settings: Default::default(),
    };
    let (track_segments, canvas) = resolve_timeline_segments_multi(&sequence, &[asset]).unwrap();
    static EXPORTS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let output = std::env::temp_dir().join(format!(
        "oca-export-color-{}-{}.mp4",
        std::process::id(),
        EXPORTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let outcome = render_export_job_multi(
        &track_segments,
        canvas,
        &output,
        -14.0,
        avbridge::GpuEncoderPreference::Cpu,
        &[],
        &[],
        &[],
        &AtomicBool::new(false),
        |_| {},
    )
    .expect("exports");
    assert!(matches!(outcome, RenderOutcome::Completed));

    output
}

fn assert_pixel_near(pixel: [u8; 3], expected: [i32; 3], tolerance: i32, what: &str) {
    for channel in 0..3 {
        assert!(
            (i32::from(pixel[channel]) - expected[channel]).abs() <= tolerance,
            "{what}: the export shows {pixel:?}, the colour is {expected:?}"
        );
    }
}

/// Every colour fixture holds the sRGB colour 200, 60, 40 (the HDR one as PQ, BT.2020).
#[test]
fn an_exported_hd_clip_keeps_its_colour() {
    let pixel = exported_centre_pixel("hd709_untagged.mkv", |_| {});
    assert_pixel_near(pixel, [200, 60, 40], 8, "an SDR clip");
}

#[test]
fn an_exported_hdr_clip_is_tone_mapped_to_the_same_sdr_colour() {
    let pixel = exported_centre_pixel("hdr10_pq.mkv", |_| {});
    assert_pixel_near(pixel, [200, 60, 40], 10, "an HDR clip");
}

#[test]
fn exported_sd_and_full_range_clips_keep_their_colour_in_rec709() {
    // 601 SD and full-range HD are converted to the Rec.709 limited range the export is tagged
    // with: without it the player would read their YUV with the wrong matrix or range.
    assert_pixel_near(
        exported_centre_pixel("sd601_untagged.mkv", |_| {}),
        [200, 60, 40],
        8,
        "an SD 601 clip",
    );
    assert_pixel_near(
        exported_centre_pixel("hd709_fullrange.mkv", |_| {}),
        [200, 60, 40],
        8,
        "a full-range clip",
    );
}

#[test]
fn an_export_says_it_is_rec709_limited_range() {
    for fixture in ["sd601_untagged.mkv", "hd709_untagged.mkv", "hdr10_pq.mkv"] {
        let output = exported_file(fixture);
        let info = avbridge::probe(&output).expect("the export probes");
        let _ = std::fs::remove_file(&output);
        let color = info.color.expect("a video export");
        assert_eq!(color.space, 1, "{fixture}: the matrix (AVCOL_SPC_BT709)");
        assert_eq!(color.primaries, 1, "{fixture}: the primaries");
        assert_eq!(color.transfer, 1, "{fixture}: the transfer");
        assert!(!color.full_range, "{fixture}: limited range");
        assert_eq!(color.bit_depth, 8, "{fixture}");
    }
}

#[test]
fn hdr_clips_are_tone_mapped_inside_a_multi_layer_export_too() {
    let pixel = exported_centre_pixel_on_tracks("hdr10_pq.mkv", 2, |_| {});
    assert_pixel_near(pixel, [200, 60, 40], 12, "stacked HDR layers");
}

#[test]
fn a_clip_set_to_treat_as_sdr_keeps_the_old_washed_out_look() {
    let pixel = exported_centre_pixel("hdr10_pq.mkv", |asset| {
        asset.hdr_handling = crate::HdrHandling::TreatAsSdr;
    });
    // Not converted: the PQ code values read as SDR, far from the colour (the escape hatch).
    assert!(
        (i32::from(pixel[0]) - 200).abs() > 40,
        "treated as SDR it should look different, got {pixel:?}"
    );
}

/// A sequence with one video track holding the colour fixture's clip, for the mix tests.
fn mix_fixture(mix: crate::timeline::TrackMix) -> (crate::Sequence, Vec<crate::MediaAsset>) {
    use crate::timeline::{AudioRole, ClipInstance, Timeline, Track, TrackKind};
    let source = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/color/hd709_untagged.mkv");
    let asset = crate::probe_media(&source)
        .expect("probes")
        .into_media_asset(1, "clip.mkv".into(), source);
    let mut clip = ClipInstance::new(1, 1, 0.0, 0.0, 0.4);
    clip.gain_db = -2.0;
    let sequence = crate::Sequence {
        id: 1,
        name: "s".into(),
        timeline: Timeline {
            tracks: vec![Track {
                id: 1,
                name: "V1".into(),
                kind: TrackKind::Video,
                clips: vec![clip],
                text_clips: vec![],
                shape_clips: vec![],
                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                locked: false,
                color_label: None,
                mix,
            }],
            playhead_secs: 0.0,
            markers: vec![],
            multicam_groups: vec![],
        },
        export_settings: Default::default(),
    };
    (sequence, vec![asset])
}

#[test]
fn a_neutral_track_keeps_the_one_pass_audio_path() {
    let (sequence, media) = mix_fixture(crate::timeline::TrackMix::default());
    assert!(resolve_audio_segments(&sequence, &media)
        .unwrap()
        .is_empty());
}

#[test]
fn a_fader_or_pan_sends_the_track_through_the_mix_with_its_level_added_to_the_clips() {
    let (sequence, media) = mix_fixture(crate::timeline::TrackMix {
        volume_db: -6.0,
        pan: 0.5,
    });
    let segments = resolve_audio_segments(&sequence, &media).unwrap();
    assert_eq!(segments.len(), 1, "the track is mixed, not copied");
    assert!(
        (segments[0].gain_db - (-8.0)).abs() < 1e-6,
        "-2 dB clip and -6 dB fader"
    );
    assert_eq!(segments[0].pan, 0.5);

    // A pan alone also needs the mix.
    let (sequence, media) = mix_fixture(crate::timeline::TrackMix {
        volume_db: 0.0,
        pan: -1.0,
    });
    assert_eq!(resolve_audio_segments(&sequence, &media).unwrap().len(), 1);
}

#[test]
fn the_fader_multiplies_a_keyframed_gain() {
    let (mut sequence, media) = mix_fixture(crate::timeline::TrackMix {
        volume_db: -6.0,
        pan: 0.0,
    });
    sequence.timeline.tracks[0].clips[0].gain_keyframes = vec![
        crate::Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        crate::Keyframe {
            time_fraction: 1.0,
            value: -12.0,
        },
    ];
    let segments = resolve_audio_segments(&sequence, &media).unwrap();
    let expr = &segments[0].gain_keyframe_expr;
    assert!(
        expr.starts_with('(') && expr.contains(")*0.501187"),
        "{expr}"
    );
}
