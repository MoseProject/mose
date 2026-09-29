use super::*;

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
