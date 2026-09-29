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
fn fps_to_rational_never_panics_on_extreme_input() {
    let _ = fps_to_rational(f32::INFINITY);
    let _ = fps_to_rational(f32::NAN);
}
