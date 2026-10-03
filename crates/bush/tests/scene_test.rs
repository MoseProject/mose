use std::path::PathBuf;

use bush::{
    Contributor, ContributorContent, ContributorId, MediaSource, Placement, RasterOverlay,
    SceneError, SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn media(id: u64, layer: i32, visible: bool, audible: bool) -> Contributor {
    Contributor {
        id: ContributorId(id),
        layer,
        visible,
        audible,
        content: ContributorContent::Media(MediaSource {
            path: PathBuf::from(format!("clip-{id}.mp4")),
            range: SourceRange::new(0.0, 2.0).unwrap(),
            version: Version(1),
        }),
        placement: Placement::default(),
    }
}

#[test]
fn scene_sorts_contributors_once_by_layer_then_identity() {
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![
            media(3, 2, true, false),
            media(2, 1, true, true),
            media(1, 1, true, true),
        ],
    )
    .unwrap();

    let ids: Vec<_> = scene
        .contributors()
        .iter()
        .map(|contributor| contributor.id)
        .collect();
    assert_eq!(
        ids,
        vec![ContributorId(1), ContributorId(2), ContributorId(3)]
    );
}

#[test]
fn scene_skips_hidden_and_muted_contributors_without_reordering() {
    let scene = SceneSnapshot::new(
        SceneVersions::default(),
        vec![
            media(1, 0, true, false),
            media(2, 1, false, true),
            media(3, 2, true, true),
        ],
    )
    .unwrap();

    let visible: Vec<_> = scene
        .visible_contributors()
        .map(|contributor| contributor.id)
        .collect();
    let audible: Vec<_> = scene
        .audible_contributors()
        .map(|contributor| contributor.id)
        .collect();

    assert_eq!(visible, vec![ContributorId(1), ContributorId(3)]);
    assert_eq!(audible, vec![ContributorId(2), ContributorId(3)]);
}

#[test]
fn scene_rejects_invalid_content_before_runtime_work() {
    let error = SceneSnapshot::new(
        SceneVersions::default(),
        vec![Contributor {
            id: ContributorId(1),
            layer: 0,
            visible: true,
            audible: false,
            content: ContributorContent::RasterOverlay(RasterOverlay {
                width: 0,
                height: 10,
                version: Version(1),
            }),
            placement: Placement::default(),
        }],
    )
    .unwrap_err();

    assert_eq!(error, SceneError::EmptyRasterOverlay);
}

#[test]
fn scene_rejects_duplicate_contributor_identity() {
    let error = SceneSnapshot::new(
        SceneVersions::default(),
        vec![media(1, 0, true, true), media(1, 1, true, true)],
    )
    .unwrap_err();

    assert_eq!(error, SceneError::DuplicateContributor(ContributorId(1)));
}

#[test]
fn source_range_rejects_nan_negative_and_empty_ranges() {
    assert_eq!(
        SourceRange::new(f64::NAN, 1.0),
        Err(SceneError::NonFiniteSourceRange)
    );
    assert_eq!(
        SourceRange::new(-1.0, 1.0),
        Err(SceneError::InvalidSourceRange {
            in_secs: -1.0,
            out_secs: 1.0,
        })
    );
    assert_eq!(
        SourceRange::new(1.0, 1.0),
        Err(SceneError::InvalidSourceRange {
            in_secs: 1.0,
            out_secs: 1.0,
        })
    );
}

fn placed(start: f64, speed: f64) -> Contributor {
    let mut contributor = media(1, 0, true, true);
    contributor.placement = Placement {
        timeline_start_secs: start,
        speed,
        ..Placement::default()
    };
    contributor
}

#[test]
fn media_window_and_source_time_follow_start_and_speed() {
    // Source range 0..2s at 2x speed, starting at timeline 10s: occupies 10..11s.
    let contributor = placed(10.0, 2.0);

    assert_eq!(contributor.timeline_duration_secs(), Some(1.0));
    assert!(!contributor.is_active_at(9.99));
    assert!(contributor.is_active_at(10.0));
    assert!(!contributor.is_active_at(11.0));
    assert_eq!(contributor.source_time_at(10.5), Some(1.0));
    assert_eq!(contributor.source_time_at(11.5), None);
}

#[test]
fn overlay_without_duration_is_always_active() {
    let overlay = Contributor {
        id: ContributorId(9),
        layer: 5,
        visible: true,
        audible: false,
        content: ContributorContent::RasterOverlay(RasterOverlay {
            width: 4,
            height: 4,
            version: Version(1),
        }),
        placement: Placement::default(),
    };
    assert!(overlay.is_active_at(1_000.0));
    assert_eq!(overlay.source_time_at(1.0), None);
}

#[test]
fn invalid_placement_is_rejected() {
    for placement in [
        Placement {
            speed: 0.0,
            ..Placement::default()
        },
        Placement {
            opacity: 1.5,
            ..Placement::default()
        },
        Placement {
            timeline_start_secs: -1.0,
            ..Placement::default()
        },
        Placement {
            duration_secs: Some(0.0),
            ..Placement::default()
        },
    ] {
        let mut contributor = media(1, 0, true, true);
        contributor.placement = placement;
        assert_eq!(
            SceneSnapshot::new(SceneVersions::default(), vec![contributor]).unwrap_err(),
            SceneError::InvalidPlacement
        );
    }
}

#[test]
fn speed_ramp_changes_duration_and_integrates_source_time() {
    // 2 s of source ramping 1x -> 3x, linear in source time (the exporter's definition): it
    // lasts 2 / (3 - 1) * ln 3 s on the timeline and its speed grows as e^t.
    let mut contributor = media(1, 0, true, true);
    contributor.content = ContributorContent::Media(MediaSource {
        path: PathBuf::from("clip.mp4"),
        range: SourceRange::new(0.0, 2.0).unwrap(),
        version: Version(1),
    });
    contributor.placement = Placement {
        speed: 1.0,
        speed_ramp_end: Some(3.0),
        ..Placement::default()
    };

    let duration = 3f64.ln();
    assert!((contributor.timeline_duration_secs().unwrap() - duration).abs() < 1e-12);
    assert!(contributor.is_active_at(duration - 0.01));
    assert!(!contributor.is_active_at(duration + 0.01));
    // source(t) = e^t - 1: 0 -> 0, 0.5 -> e^0.5 - 1, the whole duration -> 2.0.
    assert_eq!(contributor.source_offset_at(0.0), 0.0);
    assert!((contributor.source_offset_at(0.5) - (0.5f64.exp() - 1.0)).abs() < 1e-12);
    assert!((contributor.source_offset_at(duration) - 2.0).abs() < 1e-12);
    assert!((contributor.speed_at(0.0) - 1.0).abs() < 1e-12);
    assert!((contributor.speed_at(0.5) - 0.5f64.exp()).abs() < 1e-12);
    assert!((contributor.speed_at(duration) - 3.0).abs() < 1e-12);

    // A ramp down lasts longer: 2 s of source at 1x -> 0.5x is 2 / (0.5 - 1) * ln 0.5 s.
    contributor.placement.speed_ramp_end = Some(0.5);
    let duration = contributor.timeline_duration_secs().unwrap();
    assert!((duration - 4.0 * 2f64.ln()).abs() < 1e-12);
    assert!((contributor.source_offset_at(duration) - 2.0).abs() < 1e-9);

    // Equal start and end speeds are a constant speed.
    contributor.placement.speed_ramp_end = Some(1.0);
    assert!((contributor.timeline_duration_secs().unwrap() - 2.0).abs() < 1e-12);
}

#[test]
fn constant_speed_is_unchanged_and_invalid_ramps_are_rejected() {
    let mut contributor = media(1, 0, true, true);
    contributor.placement = Placement {
        speed: 2.0,
        ..Placement::default()
    };
    assert_eq!(contributor.timeline_duration_secs(), Some(1.0));
    assert_eq!(contributor.source_offset_at(0.25), 0.5);
    assert_eq!(contributor.speed_at(0.9), 2.0);

    for end in [0.0, -1.0, f64::NAN] {
        let mut bad = media(1, 0, true, true);
        bad.placement.speed_ramp_end = Some(end);
        assert_eq!(
            SceneSnapshot::new(SceneVersions::default(), vec![bad]).unwrap_err(),
            SceneError::InvalidPlacement
        );
    }
}

fn matte_source(name: &str) -> bush::MatteSource {
    bush::MatteSource {
        path: PathBuf::from(name),
        version: Version(1),
    }
}

fn scene_with(placement: Placement) -> Result<SceneSnapshot, SceneError> {
    let mut contributor = media(1, 0, true, true);
    contributor.placement = placement;
    SceneSnapshot::new(SceneVersions::default(), vec![contributor])
}

#[test]
fn matte_placements_are_validated() {
    let invalid = |placement: Placement| {
        assert_eq!(
            scene_with(placement).unwrap_err(),
            SceneError::InvalidPlacement
        );
    };
    invalid(Placement {
        alpha_matte: Some(matte_source("")),
        ..Placement::default()
    });
    for (path, sigma) in [
        ("", 4.0),
        ("m.mp4", 0.0),
        ("m.mp4", bush::MAX_BLUR_SIGMA + 1.0),
    ] {
        invalid(Placement {
            privacy_blur: Some(bush::PrivacyBlur {
                matte: matte_source(path),
                sigma,
            }),
            ..Placement::default()
        });
    }
    assert!(scene_with(Placement {
        alpha_matte: Some(matte_source("a.mp4")),
        privacy_blur: Some(bush::PrivacyBlur {
            matte: matte_source("b.mp4"),
            sigma: 8.0,
        }),
        ..Placement::default()
    })
    .is_ok());
}

#[test]
fn matte_times_follow_the_source_for_alpha_and_the_window_for_privacy() {
    // 2 s of source at 2x: a 1 s window starting at 3 s.
    let mut clip = media(1, 0, true, true);
    clip.placement = Placement {
        timeline_start_secs: 3.0,
        speed: 2.0,
        alpha_matte: Some(matte_source("a.mp4")),
        privacy_blur: Some(bush::PrivacyBlur {
            matte: matte_source("b.mp4"),
            sigma: 8.0,
        }),
        ..Placement::default()
    };
    assert_eq!(clip.alpha_matte_time_at(3.5), Some(1.0));
    assert_eq!(clip.privacy_matte_time_at(3.5), Some(1.0));
    assert_eq!(clip.privacy_matte_time_at(3.25), Some(0.5));
    // Outside the window neither has a time.
    assert_eq!(clip.alpha_matte_time_at(2.0), None);
    assert_eq!(clip.privacy_matte_time_at(5.0), None);

    // Without mattes there is nothing to look up, and overlays never have one.
    let mut plain = media(2, 0, true, true);
    assert_eq!(plain.alpha_matte_time_at(0.5), None);
    assert_eq!(plain.privacy_matte_time_at(0.5), None);
    plain.content = ContributorContent::RasterOverlay(RasterOverlay {
        width: 1,
        height: 1,
        version: Version(1),
    });
    plain.placement = Placement {
        alpha_matte: Some(matte_source("a.mp4")),
        privacy_blur: Some(bush::PrivacyBlur {
            matte: matte_source("b.mp4"),
            sigma: 8.0,
        }),
        duration_secs: Some(1.0),
        ..Placement::default()
    };
    assert_eq!(plain.alpha_matte_time_at(0.5), None);
    assert_eq!(plain.privacy_matte_time_at(0.5), None);
}

#[test]
fn a_ramp_with_equal_speeds_is_a_constant_speed() {
    let mut clip = media(1, 0, true, true);
    clip.placement = Placement {
        speed: 2.0,
        speed_ramp_end: Some(2.0),
        ..Placement::default()
    };
    assert_eq!(clip.speed_at(0.3), 2.0);
    assert_eq!(clip.source_offset_at(0.3), 0.6);
}

#[test]
fn position_and_crop_tracks_are_evaluated_at_the_playhead() {
    let track = |from: f32, to: f32| {
        bush::KeyframeTrack::new(vec![
            bush::Keyframe {
                time_secs: 0.0,
                value: from,
                easing: bush::Easing::Linear,
            },
            bush::Keyframe {
                time_secs: 2.0,
                value: to,
                easing: bush::Easing::Linear,
            },
        ])
        .unwrap()
    };
    let mut clip = media(1, 0, true, true);
    clip.placement = Placement {
        animation: bush::Animation {
            x: Some(track(0.0, 100.0)),
            y: Some(track(10.0, 30.0)),
            crop_x: Some(track(0.0, 0.5)),
            crop_y: Some(track(0.0, 0.25)),
            crop_width: Some(track(0.5, 0.5)),
            crop_height: Some(track(0.5, 0.5)),
            ..bush::Animation::default()
        },
        ..Placement::default()
    };
    let at = clip.placement_at(1.0);
    assert_eq!((at.x, at.y), (50, 20));
    let crop = at.transform.crop;
    assert!((crop.x - 0.25).abs() < 1e-6 && (crop.y - 0.125).abs() < 1e-6);
    assert_eq!((crop.width, crop.height), (0.5, 0.5));
}

#[test]
fn an_empty_media_path_is_rejected() {
    let mut clip = media(1, 0, true, true);
    clip.content = ContributorContent::Media(MediaSource {
        path: PathBuf::new(),
        range: SourceRange::new(0.0, 1.0).unwrap(),
        version: Version(1),
    });
    assert_eq!(
        SceneSnapshot::new(SceneVersions::default(), vec![clip]).unwrap_err(),
        SceneError::EmptyMediaPath
    );
}
