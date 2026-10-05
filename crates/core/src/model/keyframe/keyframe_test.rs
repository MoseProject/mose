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

use super::*;

#[test]
fn evaluate_keyframes_returns_default_when_empty() {
    let keyframes: Vec<Keyframe<f32>> = vec![];
    assert_eq!(evaluate_keyframes(&keyframes, 0.5, 1.0), 1.0);
}

#[test]
fn evaluate_keyframes_returns_the_constant_value_for_a_single_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.7,
        value: 2.0,
    }];
    assert_eq!(evaluate_keyframes(&keyframes, 0.0, 1.0), 2.0);
    assert_eq!(evaluate_keyframes(&keyframes, 1.0, 1.0), 2.0);
}

#[test]
fn evaluate_keyframes_holds_before_the_first_and_after_the_last() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.25,
            value: 10.0,
        },
        Keyframe {
            time_fraction: 0.75,
            value: 20.0,
        },
    ];
    assert_eq!(evaluate_keyframes(&keyframes, 0.0, 0.0), 10.0);
    assert_eq!(evaluate_keyframes(&keyframes, 1.0, 0.0), 20.0);
}

#[test]
fn evaluate_keyframes_interpolates_linearly_at_the_midpoint() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 10.0,
        },
    ];
    assert_eq!(evaluate_keyframes(&keyframes, 0.5, 0.0), 5.0);
}

#[test]
fn evaluate_keyframes_picks_the_right_segment_across_three_or_more_points() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 0.5,
            value: 10.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    assert_eq!(evaluate_keyframes(&keyframes, 0.25, 0.0), 5.0);
    assert_eq!(evaluate_keyframes(&keyframes, 0.75, 0.0), 5.0);
}

#[test]
fn position_lerp_interpolates_both_components() {
    let a = Position { x: 0.0, y: 0.0 };
    let b = Position { x: 1.0, y: 2.0 };
    assert_eq!(a.lerp(b, 0.5), Position { x: 0.5, y: 1.0 });
}

#[test]
fn split_keyframes_at_is_empty_for_an_empty_input() {
    let keyframes: Vec<Keyframe<f32>> = vec![];
    let (first, second) = split_keyframes_at(&keyframes, 0.5, 1.0);
    assert!(first.is_empty());
    assert!(second.is_empty());
}

#[test]
fn split_keyframes_at_rescales_and_inserts_a_matching_boundary_point() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 10.0,
        },
    ];
    let boundary = evaluate_keyframes(&keyframes, 0.5, 0.0);
    let (first, second) = split_keyframes_at(&keyframes, 0.5, 0.0);

    assert_eq!(
        first,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: 0.0
            },
            Keyframe {
                time_fraction: 1.0,
                value: boundary
            },
        ]
    );
    assert_eq!(
        second,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: boundary
            },
            Keyframe {
                time_fraction: 1.0,
                value: 10.0
            },
        ]
    );
}

#[test]
fn split_keyframes_at_drops_keyframes_that_land_on_the_other_side() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.1,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 0.9,
            value: 9.0,
        },
    ];
    let (first, second) = split_keyframes_at(&keyframes, 0.5, 0.0);
    // Only the 0.1 keyframe (rescaled to 0.1/0.5 = 0.2) plus the synthetic boundary land in
    // the first half; only the 0.9 one (rescaled to (0.9-0.5)/0.5 = 0.8) plus the boundary
    // land in the second.
    assert_eq!(first.len(), 2);
    assert_eq!(first[0].time_fraction, 0.2);
    assert_eq!(second.len(), 2);
    assert!((second[1].time_fraction - 0.8).abs() < 1e-6);
}

#[test]
fn smooth_speed_ramp_duration_secs_matches_plain_division_for_a_constant_speed() {
    // v0 == v1 degenerates to the same formula the old constant-speed path uses.
    let d = smooth_speed_ramp_duration_secs(10.0, 2.0, 2.0);
    assert!((d - 5.0).abs() < 1e-6);
}

#[test]
fn smooth_speed_ramp_duration_secs_matches_numerical_integration() {
    // ∫₀^D 1/(v0 + (v1-v0)/D * t) dt, integrated numerically at fine resolution, should match
    // the closed-form log expression to a tight tolerance.
    let (d, v0, v1) = (8.0, 0.5, 2.0);
    let steps = 200_000;
    let dt = d / steps as f64;
    let mut numeric = 0.0;
    for i in 0..steps {
        let t = (i as f64 + 0.5) * dt;
        let speed = v0 as f64 + (v1 as f64 - v0 as f64) * t / d;
        numeric += dt / speed;
    }
    let closed_form = smooth_speed_ramp_duration_secs(d, v0, v1);
    assert!(
        (closed_form - numeric).abs() < 1e-4,
        "closed form {closed_form} vs numeric {numeric}"
    );
}

#[test]
fn smooth_speed_ramp_duration_secs_is_shorter_for_a_speed_up_ramp() {
    // Speeding up (v1 > v0) should always finish faster than staying at the start speed the
    // whole time, and slower than staying at the end speed the whole time.
    let d = 6.0;
    let ramped = smooth_speed_ramp_duration_secs(d, 1.0, 3.0);
    assert!(ramped < d / 1.0);
    assert!(ramped > d / 3.0);
}

#[test]
fn smooth_speed_ramp_source_secs_at_is_the_true_inverse_of_the_duration_formula() {
    // Round-trip: convert a source time to elapsed timeline time, then back — should return
    // (approximately) the original source time, across both a speed-up and slow-down ramp.
    for (v0, v1) in [(0.5f32, 2.0f32), (2.0f32, 0.5f32)] {
        let d = 10.0;
        for source_t in [0.0, 2.5, 5.0, 7.5, 10.0] {
            let timeline_elapsed = smooth_speed_ramp_elapsed_timeline_secs(source_t, d, v0, v1);
            let back = smooth_speed_ramp_source_secs_at(timeline_elapsed, d, v0, v1);
            assert!(
                (back - source_t).abs() < 1e-3,
                "v0={v0} v1={v1} source_t={source_t}: round-trip gave {back}"
            );
        }
    }
}

#[test]
fn smooth_speed_ramp_source_secs_at_matches_plain_multiply_for_a_constant_speed() {
    let t = smooth_speed_ramp_source_secs_at(4.0, 10.0, 2.0, 2.0);
    assert!((t - 8.0).abs() < 1e-6);
}

#[test]
fn scale_filter_expr_is_none_for_no_keyframes() {
    assert_eq!(scale_filter_expr(&[], 30, 1, 5.0), None);
}

#[test]
fn scale_filter_expr_is_none_for_a_single_unity_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 1.0,
    }];
    assert_eq!(scale_filter_expr(&keyframes, 30, 1, 5.0), None);
}

#[test]
fn scale_filter_expr_is_static_crop_scale_for_a_single_non_unity_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 2.0,
    }];
    let expr = scale_filter_expr(&keyframes, 30, 1, 5.0).unwrap();
    assert!(expr.starts_with("crop=iw/2.00000:ih/2.00000"));
    assert!(!expr.contains("geq"));
}

#[test]
fn scale_filter_expr_is_none_when_every_keyframe_is_unity() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    assert_eq!(scale_filter_expr(&keyframes, 30, 1, 5.0), None);
}

#[test]
fn scale_filter_expr_builds_a_geq_expression_for_an_animated_ramp() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.5,
        },
    ];
    let expr = scale_filter_expr(&keyframes, 30, 1, 5.0).unwrap();
    assert!(expr.starts_with("geq=lum="));
    assert!(expr.contains("if(lt(N,"));
    assert!(expr.contains("if(between(N,"));
}

#[test]
fn rotation_filter_angle_expr_is_none_for_no_keyframes() {
    assert_eq!(rotation_filter_angle_expr(&[], 5.0), None);
}

#[test]
fn rotation_filter_angle_expr_is_none_when_every_keyframe_is_zero() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    assert_eq!(rotation_filter_angle_expr(&keyframes, 5.0), None);
}

#[test]
fn rotation_filter_angle_expr_converts_a_single_keyframe_to_radians() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 180.0,
    }];
    let expr = rotation_filter_angle_expr(&keyframes, 5.0).unwrap();
    let radians: f32 = expr.parse().unwrap();
    assert!((radians - std::f32::consts::PI).abs() < 1e-4);
}

#[test]
fn rotation_filter_angle_expr_uses_t_for_an_animated_ramp() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 90.0,
        },
    ];
    let expr = rotation_filter_angle_expr(&keyframes, 5.0).unwrap();
    assert!(expr.contains("if(lt(t,"));
    assert!(expr.contains("if(between(t,"));
}

#[test]
fn gain_filter_db_expr_is_none_for_no_keyframes() {
    assert_eq!(gain_filter_db_expr(&[], 5.0), None);
}

#[test]
fn gain_filter_db_expr_is_none_when_every_keyframe_is_zero_db() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    assert_eq!(gain_filter_db_expr(&keyframes, 5.0), None);
}

#[test]
fn gain_filter_db_expr_converts_a_single_keyframe_to_a_linear_multiplier() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: -6.0206,
    }];
    let expr = gain_filter_db_expr(&keyframes, 5.0).unwrap();
    let linear: f32 = expr.parse().unwrap();
    // -6.0206 dB is the standard "half amplitude" reference point.
    assert!((linear - 0.5).abs() < 1e-3);
}

#[test]
fn gain_filter_db_expr_uses_t_for_an_animated_ramp() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: -20.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    let expr = gain_filter_db_expr(&keyframes, 5.0).unwrap();
    assert!(expr.contains("if(lt(t,"));
    assert!(expr.contains("if(between(t,"));
}

#[test]
fn color_balance_filter_expr_is_none_when_everything_is_neutral() {
    assert_eq!(
        color_balance_filter_expr(&[], &[], &[], 0.0, 1.0, 1.0, 5.0),
        None
    );
}

#[test]
fn color_balance_filter_expr_uses_constants_for_unanimated_axes() {
    let expr = color_balance_filter_expr(&[], &[], &[], 0.3, 1.0, 1.0, 5.0).unwrap();
    assert!(!expr.contains("eval=frame"));
    assert!(expr.starts_with("eq=brightness=0.3"));
    assert!(expr.contains("contrast=1.0000000"));
    assert!(expr.contains("saturation=1.0000000"));
}

#[test]
fn color_balance_filter_expr_animates_only_the_keyframed_axis() {
    let brightness_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: -0.5,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.5,
        },
    ];
    let expr =
        color_balance_filter_expr(&brightness_keyframes, &[], &[], 0.0, 1.5, 1.0, 5.0).unwrap();
    assert!(expr.contains("eval=frame"));
    assert!(expr.contains("if(lt(t,"));
    // The un-animated contrast axis still uses its own constant.
    assert!(expr.contains("contrast=1.5000000"));
}

#[test]
fn crop_filter_expr_is_none_for_a_fully_uncropped_unanimated_frame() {
    assert_eq!(
        crop_filter_expr(&[], &[], &[], &[], 0.0, 0.0, 1.0, 1.0, 30, 1, 5.0),
        None
    );
}

#[test]
fn crop_filter_expr_builds_a_geq_expression_for_a_static_crop_with_no_keyframes() {
    let expr = crop_filter_expr(&[], &[], &[], &[], 0.1, 0.2, 0.5, 0.6, 30, 1, 5.0).unwrap();
    assert!(expr.starts_with("geq="));
    assert!(!expr.contains("if(lt(N,")); // no animation, just plain constants
}

#[test]
fn crop_filter_expr_animates_only_the_keyframed_axis() {
    let crop_x_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.5,
        },
    ];
    let expr = crop_filter_expr(
        &crop_x_keyframes,
        &[],
        &[],
        &[],
        0.0,
        0.0,
        0.5,
        0.5,
        30,
        1,
        5.0,
    )
    .unwrap();
    assert!(expr.contains("if(lt(N,"));
    assert!(expr.contains("if(between(N,"));
}

#[test]
fn shape_axis_expr_is_a_plain_constant_for_no_keyframes() {
    assert_eq!(shape_axis_expr(&[], 0.5, 1.0, 3.0), "0.5000");
}

#[test]
fn shape_axis_expr_is_a_plain_constant_for_a_single_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.25,
    }];
    assert_eq!(shape_axis_expr(&keyframes, 0.5, 1.0, 3.0), "0.2500");
}

#[test]
fn shape_axis_expr_offsets_t_by_the_shapes_start_secs_when_animated() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.1,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.9,
        },
    ];
    let expr = shape_axis_expr(&keyframes, 0.5, 2.5, 3.0);
    assert!(expr.contains("(T-2.500000)"));
    assert!(expr.contains("if(lt((T-2.500000),"));
    assert!(expr.contains("if(between((T-2.500000),"));
}

#[test]
fn text_opacity_alpha_expr_is_none_for_no_keyframes() {
    assert_eq!(text_opacity_alpha_expr(&[], 1.0, 3.0), None);
}

#[test]
fn text_opacity_alpha_expr_is_none_when_every_keyframe_is_fully_opaque() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    assert_eq!(text_opacity_alpha_expr(&keyframes, 1.0, 3.0), None);
}

#[test]
fn text_opacity_alpha_expr_clamps_a_single_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: -0.5,
    }];
    assert_eq!(
        text_opacity_alpha_expr(&keyframes, 1.0, 3.0),
        Some("0.0000000".to_string())
    );
}

#[test]
fn text_opacity_alpha_expr_offsets_t_by_start_secs_for_a_fade() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    let expr = text_opacity_alpha_expr(&keyframes, 2.0, 4.0).unwrap();
    assert!(expr.contains("(T-2.000000)"));
}

#[test]
fn text_position_offset_expr_is_none_for_no_keyframes() {
    assert_eq!(text_position_offset_expr(&[], 0.1, 0.0, 3.0, 1920.0), None);
}

#[test]
fn text_position_offset_expr_is_none_for_a_single_keyframe() {
    // A single keyframe's value is exactly `base` by construction (text_clip_to_segments bakes
    // the raster at the first keyframe's value) -- nothing left to offset.
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 0.3,
    }];
    assert_eq!(
        text_position_offset_expr(&keyframes, 0.3, 0.0, 3.0, 1920.0),
        None
    );
}

#[test]
fn text_position_offset_expr_is_none_when_every_keyframe_equals_base() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.2,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.2,
        },
    ];
    assert_eq!(
        text_position_offset_expr(&keyframes, 0.2, 0.0, 3.0, 1920.0),
        None
    );
}

#[test]
fn text_position_offset_expr_scales_the_delta_by_canvas_extent_px() {
    // base=0.1, moving to 0.6 -- a 0.5 fraction delta, times a 1920px-wide canvas is 960px.
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.1,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.6,
        },
    ];
    let expr = text_position_offset_expr(&keyframes, 0.1, 0.0, 2.0, 1920.0).unwrap();
    // piecewise_expr's final (last-keyframe) branch is the plain formatted last value.
    assert!(expr.contains("960.0000000"));
}

#[test]
fn text_position_offset_expr_offsets_t_by_start_secs() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    let expr = text_position_offset_expr(&keyframes, 0.0, 5.0, 2.0, 1080.0).unwrap();
    assert!(expr.contains("(t-5.000000)"));
}

#[test]
fn text_scale_sample_exprs_is_none_for_no_keyframes() {
    assert_eq!(text_scale_sample_exprs(&[], 100.0, 50.0, 0.0, 3.0), None);
}

#[test]
fn text_scale_sample_exprs_is_none_for_a_single_unity_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 1.0,
    }];
    assert_eq!(
        text_scale_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0),
        None
    );
}

#[test]
fn text_scale_sample_exprs_builds_a_static_remap_for_a_single_non_unity_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 2.0,
    }];
    let (x, y) = text_scale_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0).unwrap();
    assert!(x.contains("100.0000"));
    assert!(x.contains("2.0000000"));
    assert!(y.contains("50.0000"));
    assert!(y.contains("2.0000000"));
}

#[test]
fn text_scale_sample_exprs_is_none_when_every_keyframe_is_unity() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    assert_eq!(
        text_scale_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0),
        None
    );
}

#[test]
fn text_scale_sample_exprs_builds_a_piecewise_ramp_for_an_animated_scale() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 2.0,
        },
    ];
    let (x, y) = text_scale_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 2.0).unwrap();
    // piecewise_expr's final (last-keyframe) branch is the plain formatted last value.
    assert!(x.contains("2.0000000"));
    assert!(y.contains("2.0000000"));
    assert!(x.starts_with("(100.0000+(X-(100.0000))/("));
    assert!(y.starts_with("(50.0000+(Y-(50.0000))/("));
}

#[test]
fn text_scale_sample_exprs_offsets_capital_t_by_start_secs() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 2.0,
        },
    ];
    let (x, _y) = text_scale_sample_exprs(&keyframes, 100.0, 50.0, 5.0, 2.0).unwrap();
    assert!(x.contains("(T-5.000000)"));
}

#[test]
fn text_rotation_sample_exprs_is_none_for_no_keyframes() {
    assert_eq!(text_rotation_sample_exprs(&[], 100.0, 50.0, 0.0, 3.0), None);
}

#[test]
fn text_rotation_sample_exprs_is_none_for_a_single_zero_degree_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 0.0,
    }];
    assert_eq!(
        text_rotation_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0),
        None
    );
}

#[test]
fn text_rotation_sample_exprs_builds_a_static_remap_for_a_single_non_zero_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 90.0,
    }];
    let (x, y) = text_rotation_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0).unwrap();
    assert!(x.contains("100.0000"));
    assert!(x.contains("sin("));
    assert!(x.contains("cos("));
    assert!(y.contains("50.0000"));
    assert!(y.contains("sin("));
    assert!(y.contains("cos("));
}

#[test]
fn text_rotation_sample_exprs_is_none_when_every_keyframe_is_zero_degrees() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    assert_eq!(
        text_rotation_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 3.0),
        None
    );
}

#[test]
fn text_rotation_sample_exprs_builds_a_piecewise_ramp_for_an_animated_angle() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 90.0,
        },
    ];
    let (x, y) = text_rotation_sample_exprs(&keyframes, 100.0, 50.0, 0.0, 2.0).unwrap();
    assert!(x.starts_with("(100.0000+(X-(100.0000))*cos("));
    assert!(y.starts_with("(50.0000-(X-(100.0000))*sin("));
}

#[test]
fn text_rotation_sample_exprs_offsets_capital_t_by_start_secs() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 90.0,
        },
    ];
    let (x, _y) = text_rotation_sample_exprs(&keyframes, 100.0, 50.0, 5.0, 2.0).unwrap();
    assert!(x.contains("(T-5.000000)"));
}

#[test]
fn opacity_alpha_ramp_expr_is_none_for_no_keyframes() {
    assert_eq!(opacity_alpha_ramp_expr(&[], 30, 1, 5.0), None);
}

#[test]
fn opacity_alpha_ramp_expr_is_none_when_fully_opaque() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    assert_eq!(opacity_alpha_ramp_expr(&keyframes, 30, 1, 5.0), None);
}

#[test]
fn opacity_alpha_ramp_expr_clamps_a_single_keyframe() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 1.5,
    }];
    // Clamped to 1.0 (fully opaque, the default) — nothing to animate.
    assert_eq!(opacity_alpha_ramp_expr(&keyframes, 30, 1, 5.0), None);

    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: -0.5,
    }];
    let expr = opacity_alpha_ramp_expr(&keyframes, 30, 1, 5.0).unwrap();
    let value: f32 = expr.parse().unwrap();
    assert_eq!(value, 0.0);
}

#[test]
fn position_overlay_xy_expr_is_none_for_no_keyframes() {
    assert_eq!(position_overlay_xy_expr(&[], 5.0), None);
}

#[test]
fn position_overlay_xy_expr_is_none_when_at_the_origin() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: Position { x: 0.0, y: 0.0 },
    }];
    assert_eq!(position_overlay_xy_expr(&keyframes, 5.0), None);
}

#[test]
fn position_overlay_xy_expr_scales_a_single_keyframe_by_canvas_dimensions() {
    let keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: Position { x: 0.25, y: 0.0 },
    }];
    let (x_expr, y_expr) = position_overlay_xy_expr(&keyframes, 5.0).unwrap();
    assert_eq!(x_expr, "(0.2500000)*main_w");
    assert_eq!(y_expr, "0");
}

#[test]
fn position_overlay_xy_expr_builds_a_t_based_ramp_for_an_animated_axis() {
    let keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: Position { x: 0.0, y: 0.0 },
        },
        Keyframe {
            time_fraction: 1.0,
            value: Position { x: 0.5, y: 0.0 },
        },
    ];
    let (x_expr, y_expr) = position_overlay_xy_expr(&keyframes, 5.0).unwrap();
    assert!(x_expr.contains("if(between(t,"));
    assert!(x_expr.ends_with("*main_w"));
    assert_eq!(y_expr, "0");
}
