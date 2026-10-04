use std::path::PathBuf;

use bush::{
    Animation, Contributor, ContributorContent, ContributorId, Easing, Fade, Keyframe,
    KeyframeError, KeyframeTrack, MediaSource, Placement, SceneError, SceneSnapshot, SceneVersions,
    SourceRange, Version,
};

fn key(time_secs: f64, value: f32, easing: Easing) -> Keyframe {
    Keyframe {
        time_secs,
        value,
        easing,
    }
}

fn track(keys: Vec<Keyframe>) -> KeyframeTrack {
    KeyframeTrack::new(keys).unwrap()
}

#[test]
fn track_validation_rejects_malformed_keyframes() {
    assert_eq!(
        KeyframeTrack::new(vec![]).unwrap_err(),
        KeyframeError::Empty
    );
    assert_eq!(
        KeyframeTrack::new(vec![key(f64::NAN, 0.0, Easing::Linear)]).unwrap_err(),
        KeyframeError::NonFinite
    );
    assert_eq!(
        KeyframeTrack::new(vec![key(0.0, f32::INFINITY, Easing::Linear)]).unwrap_err(),
        KeyframeError::NonFinite
    );
    assert_eq!(
        KeyframeTrack::new(vec![key(-1.0, 0.0, Easing::Linear)]).unwrap_err(),
        KeyframeError::NegativeTime
    );
    assert_eq!(
        KeyframeTrack::new(vec![
            key(1.0, 0.0, Easing::Linear),
            key(1.0, 1.0, Easing::Linear)
        ])
        .unwrap_err(),
        KeyframeError::UnorderedTimes
    );
}

#[test]
fn values_clamp_outside_the_track_and_interpolate_inside() {
    let track = track(vec![
        key(1.0, 10.0, Easing::Linear),
        key(3.0, 30.0, Easing::Linear),
    ]);
    assert_eq!(track.value_at(0.0), 10.0);
    assert_eq!(track.value_at(1.0), 10.0);
    assert_eq!(track.value_at(2.0), 20.0);
    assert_eq!(track.value_at(2.5), 25.0);
    assert_eq!(track.value_at(3.0), 30.0);
    assert_eq!(track.value_at(99.0), 30.0);
}

#[test]
fn easing_shapes_each_segment_by_its_starting_keyframe() {
    let at_quarter =
        |easing| track(vec![key(0.0, 0.0, easing), key(4.0, 100.0, Easing::Linear)]).value_at(1.0);
    assert_eq!(at_quarter(Easing::Linear), 25.0);
    assert_eq!(at_quarter(Easing::Hold), 0.0);
    assert_eq!(at_quarter(Easing::EaseIn), 6.25);
    assert_eq!(at_quarter(Easing::EaseOut), 43.75);
    assert_eq!(at_quarter(Easing::EaseInOut), 15.625);

    for easing in [
        Easing::Linear,
        Easing::EaseIn,
        Easing::EaseOut,
        Easing::EaseInOut,
    ] {
        let curve = track(vec![key(0.0, 0.0, easing), key(1.0, 1.0, Easing::Linear)]);
        assert_eq!(curve.value_at(0.0), 0.0);
        assert_eq!(curve.value_at(1.0), 1.0);
    }
}

#[test]
fn fade_ramps_in_and_out_and_never_exceeds_one() {
    let fade = Fade {
        in_secs: 1.0,
        out_secs: 2.0,
    };
    assert_eq!(fade.factor_at(0.0, Some(10.0)), 0.0);
    assert_eq!(fade.factor_at(0.5, Some(10.0)), 0.5);
    assert_eq!(fade.factor_at(1.0, Some(10.0)), 1.0);
    assert_eq!(fade.factor_at(5.0, Some(10.0)), 1.0);
    assert_eq!(fade.factor_at(9.0, Some(10.0)), 0.5);
    assert_eq!(fade.factor_at(10.0, Some(10.0)), 0.0);
    // Without an end only the fade-in applies.
    assert_eq!(fade.factor_at(1_000.0, None), 1.0);
    // Overlapping ramps take the smaller factor.
    let short = Fade {
        in_secs: 3.0,
        out_secs: 3.0,
    };
    assert_eq!(short.factor_at(1.5, Some(4.0)), 0.5);
}

fn clip(placement: Placement) -> Contributor {
    Contributor {
        id: ContributorId(1),
        layer: 0,
        visible: true,
        audible: true,
        content: ContributorContent::Media(MediaSource {
            path: PathBuf::from("clip.mp4"),
            range: SourceRange::new(0.0, 4.0).unwrap(),
            version: Version(1),
        }),
        placement,
    }
}

#[test]
fn placement_at_evaluates_animation_relative_to_the_clip_start() {
    let contributor = clip(Placement {
        timeline_start_secs: 10.0,
        x: 5,
        animation: Animation {
            x: Some(track(vec![
                key(0.0, 0.0, Easing::Linear),
                key(2.0, 200.0, Easing::Linear),
            ])),
            scale_x: Some(track(vec![
                key(0.0, 1.0, Easing::Linear),
                key(2.0, 2.0, Easing::Linear),
            ])),
            rotation_deg: Some(track(vec![
                key(0.0, 0.0, Easing::Linear),
                key(4.0, 90.0, Easing::Linear),
            ])),
            ..Animation::default()
        },
        ..Placement::default()
    });

    let halfway = contributor.placement_at(11.0);
    assert_eq!(halfway.x, 100);
    assert_eq!(halfway.transform.scale_x, 1.5);
    assert_eq!(halfway.transform.rotation_deg, 22.5);
    // Un-animated properties keep their static values.
    assert_eq!(halfway.y, 0);
    assert_eq!(halfway.transform.scale_y, 1.0);
    assert_eq!(halfway.opacity, 1.0);

    // Before the animation window starts the first keyframe holds, after it the last.
    assert_eq!(contributor.placement_at(5.0).x, 0);
    assert_eq!(contributor.placement_at(13.5).x, 200);
}

#[test]
fn fade_multiplies_with_keyframed_and_static_opacity() {
    let contributor = clip(Placement {
        opacity: 0.8,
        fade: Fade {
            in_secs: 2.0,
            out_secs: 0.0,
        },
        ..Placement::default()
    });
    // Static 0.8 at half of a 2s fade-in: 0.4.
    assert!((contributor.placement_at(1.0).opacity - 0.4).abs() < 1e-6);

    let animated = clip(Placement {
        fade: Fade {
            in_secs: 0.0,
            out_secs: 2.0,
        },
        animation: Animation {
            opacity: Some(track(vec![
                key(0.0, 1.0, Easing::Linear),
                key(4.0, 0.5, Easing::Linear),
            ])),
            ..Animation::default()
        },
        ..Placement::default()
    });
    // At t=3 of a 4s clip: keyframe 0.625 times fade-out factor 0.5.
    assert!((animated.placement_at(3.0).opacity - 0.3125).abs() < 1e-6);
}

#[test]
fn scenes_reject_animation_values_outside_property_ranges() {
    let bad_opacity = Placement {
        animation: Animation {
            opacity: Some(track(vec![key(0.0, 1.5, Easing::Linear)])),
            ..Animation::default()
        },
        ..Placement::default()
    };
    let bad_scale = Placement {
        animation: Animation {
            scale_y: Some(track(vec![key(0.0, 0.0, Easing::Linear)])),
            ..Animation::default()
        },
        ..Placement::default()
    };
    let bad_fade = Placement {
        fade: Fade {
            in_secs: -1.0,
            out_secs: 0.0,
        },
        ..Placement::default()
    };
    for placement in [bad_opacity, bad_scale, bad_fade] {
        assert_eq!(
            SceneSnapshot::new(SceneVersions::default(), vec![clip(placement)]).unwrap_err(),
            SceneError::InvalidPlacement
        );
    }
}

#[test]
fn slide_and_zoom_builders_make_transitions() {
    let slide = Animation::slide(-200.0, 0.0, 1.0, Easing::EaseOut).unwrap();
    let contributor = clip(Placement {
        animation: slide,
        ..Placement::default()
    });
    assert_eq!(contributor.placement_at(0.0).x, -200);
    assert_eq!(contributor.placement_at(0.5).x, -50);
    assert_eq!(contributor.placement_at(1.0).x, 0);
    assert_eq!(contributor.placement_at(9.0).x, 0);

    let zoom = Animation::zoom(0.5, 1.0, 2.0, Easing::Linear).unwrap();
    let contributor = clip(Placement {
        animation: zoom,
        ..Placement::default()
    });
    let halfway = contributor.placement_at(1.0);
    assert_eq!(halfway.transform.scale_x, 0.75);
    assert_eq!(halfway.transform.scale_y, 0.75);

    assert!(Animation::slide(0.0, 1.0, 0.0, Easing::Linear).is_none());
    assert!(Animation::slide(0.0, 1.0, f64::NAN, Easing::Linear).is_none());
    assert!(Animation::zoom(0.0, 1.0, 1.0, Easing::Linear).is_none());
    assert!(Animation::zoom(1.0, -1.0, 1.0, Easing::Linear).is_none());
}

#[test]
fn effect_and_crop_tracks_are_animated_and_kept_valid() {
    let contributor = clip(Placement {
        animation: Animation {
            brightness: Some(track(vec![
                key(0.0, 0.0, Easing::Linear),
                key(2.0, 0.4, Easing::Linear),
            ])),
            contrast: Some(track(vec![
                key(0.0, 1.0, Easing::Linear),
                key(2.0, 2.0, Easing::Linear),
            ])),
            saturation: Some(track(vec![
                key(0.0, 1.0, Easing::Linear),
                key(2.0, 0.0, Easing::Linear),
            ])),
            crop_width: Some(track(vec![
                key(0.0, 1.0, Easing::Linear),
                key(2.0, 0.5, Easing::Linear),
            ])),
            crop_x: Some(track(vec![
                key(0.0, 0.0, Easing::Linear),
                key(2.0, 0.9, Easing::Linear),
            ])),
            ..Animation::default()
        },
        ..Placement::default()
    });

    let halfway = contributor.placement_at(1.0);
    assert!((halfway.effects.brightness - 0.2).abs() < 1e-6);
    assert!((halfway.effects.contrast - 1.5).abs() < 1e-6);
    assert!((halfway.effects.saturation - 0.5).abs() < 1e-6);
    assert!((halfway.transform.crop.width - 0.75).abs() < 1e-6);

    // At the end the animated x (0.9) cannot push the window outside the frame: width 0.5
    // forces x <= 0.5.
    let end = contributor.placement_at(2.0).transform.crop;
    assert_eq!(end.width, 0.5);
    assert!(end.x + end.width <= 1.0 + 1e-6);

    // Out-of-range animated values are rejected at scene construction.
    for bad in [
        Animation {
            brightness: Some(track(vec![key(0.0, 2.0, Easing::Linear)])),
            ..Animation::default()
        },
        Animation {
            contrast: Some(track(vec![key(0.0, -1.0, Easing::Linear)])),
            ..Animation::default()
        },
        Animation {
            crop_height: Some(track(vec![key(0.0, 1.5, Easing::Linear)])),
            ..Animation::default()
        },
    ] {
        assert_eq!(
            SceneSnapshot::new(
                SceneVersions::default(),
                vec![clip(Placement {
                    animation: bad,
                    ..Placement::default()
                })]
            )
            .unwrap_err(),
            SceneError::InvalidPlacement
        );
    }
}

#[test]
fn an_animation_is_empty_until_any_track_is_set() {
    let one = || Some(track(vec![key(0.0, 0.5, Easing::Linear)]));
    assert!(Animation::default().is_empty());
    let animations = vec![
        Animation {
            opacity: one(),
            ..Animation::default()
        },
        Animation {
            x: one(),
            ..Animation::default()
        },
        Animation {
            y: one(),
            ..Animation::default()
        },
        Animation {
            scale_x: one(),
            ..Animation::default()
        },
        Animation {
            scale_y: one(),
            ..Animation::default()
        },
        Animation {
            rotation_deg: one(),
            ..Animation::default()
        },
        Animation {
            brightness: one(),
            ..Animation::default()
        },
        Animation {
            contrast: one(),
            ..Animation::default()
        },
        Animation {
            saturation: one(),
            ..Animation::default()
        },
        Animation {
            crop_x: one(),
            ..Animation::default()
        },
        Animation {
            crop_y: one(),
            ..Animation::default()
        },
        Animation {
            crop_width: one(),
            ..Animation::default()
        },
        Animation {
            crop_height: one(),
            ..Animation::default()
        },
        Animation {
            gain_db: one(),
            ..Animation::default()
        },
    ];
    for animation in animations {
        assert!(!animation.is_empty(), "{animation:?}");
    }
}
