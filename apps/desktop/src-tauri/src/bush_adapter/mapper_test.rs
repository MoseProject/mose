use std::path::PathBuf;
use std::sync::Arc;

use avcore::keyframe::{Keyframe, Position};
use avcore::timeline::{
    AudioRole, BlendMode as CoreBlend, ClipInstance, ColorFilter, MaskShape as CoreMask, ShapeClip,
    TextClip, Timeline, Track, TrackKind, TransitionType,
};
use avcore::{MediaAsset, MediaKind};
use bush::{BlendMode, Contributor, ContributorContent, Lut3D, MaskShape, SceneVersions};

use super::{map_timeline, Approximation, Mapped, OverlayCache, SourceResolver};

const CANVAS: (u32, u32) = (1280, 720);

struct Resolver {
    missing: Vec<u64>,
}

impl SourceResolver for Resolver {
    fn source_path(&mut self, asset: &MediaAsset) -> Option<PathBuf> {
        (!self.missing.contains(&asset.id))
            .then(|| PathBuf::from(format!("asset-{}.mp4", asset.id)))
    }

    fn lut(&mut self, _path: &str) -> Option<Arc<Lut3D>> {
        None
    }
}

fn asset(id: u64, kind: MediaKind) -> MediaAsset {
    MediaAsset {
        id,
        file_name: format!("asset-{id}"),
        source_path: PathBuf::from(format!("asset-{id}.mp4")),
        kind,
        has_audio: true,
        duration_secs: 20.0,
        codec: "h264".into(),
        source_bitrate_mbps: 8.0,
        resolution: Some((1920, 1080)),
        fps: Some(30.0),
        sample_rate_khz: None,
        loudness: None,
        proxy_path: None,
        waveform_peaks: None,
        favorited: false,
    }
}

fn track(id: u64, kind: TrackKind, clips: Vec<ClipInstance>) -> Track {
    Track {
        id,
        name: format!("T{id}"),
        kind,
        clips,
        text_clips: Vec::new(),
        shape_clips: Vec::new(),
        visible: true,
        audio_role: AudioRole::Unspecified,
        color_label: None,
        locked: false,
    }
}

fn timeline(tracks: Vec<Track>) -> Timeline {
    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

fn map_with(tracks: Vec<Track>, missing: Vec<u64>) -> Mapped {
    let media = vec![asset(1, MediaKind::Video), asset(2, MediaKind::Audio)];
    map_timeline(
        &timeline(tracks),
        &media,
        CANVAS,
        &mut Resolver { missing },
        &mut OverlayCache::default(),
        SceneVersions::default(),
    )
    .unwrap()
}

fn one_clip(clip: ClipInstance) -> (Mapped, Contributor) {
    let mapped = map_with(vec![track(1, TrackKind::Video, vec![clip])], Vec::new());
    let contributor = mapped.scene.contributors()[0].clone();
    (mapped, contributor)
}

fn clip(start: f64, source_in: f64, source_out: f64) -> ClipInstance {
    ClipInstance::new(10, 1, start, source_in, source_out)
}

#[test]
fn a_plain_clip_keeps_its_window_and_fills_the_canvas() {
    let (_, contributor) = one_clip(clip(2.0, 1.0, 6.0));
    assert!(contributor.visible && contributor.audible);
    assert_eq!(contributor.id.0, 10);
    assert_eq!(contributor.placement.timeline_start_secs, 2.0);
    assert_eq!(contributor.placement.speed, 1.0);
    assert!(contributor.placement.fit_canvas);
    assert_eq!(contributor.timeline_duration_secs(), Some(5.0));
    let ContributorContent::Media(source) = &contributor.content else {
        panic!("media expected");
    };
    assert_eq!((source.range.in_secs, source.range.out_secs), (1.0, 6.0));
}

#[test]
fn audio_tracks_are_audible_only_and_layers_follow_track_order() {
    let mapped = map_with(
        vec![
            track(1, TrackKind::Video, vec![clip(0.0, 0.0, 4.0)]),
            track(
                2,
                TrackKind::Audio,
                vec![ClipInstance::new(11, 2, 0.0, 0.0, 4.0)],
            ),
            track(
                3,
                TrackKind::Video,
                vec![ClipInstance::new(12, 1, 0.0, 0.0, 4.0)],
            ),
        ],
        Vec::new(),
    );
    let by_id = |id: u64| {
        mapped
            .scene
            .contributors()
            .iter()
            .find(|c| c.id.0 == id)
            .unwrap()
    };
    assert!(by_id(10).visible);
    assert!(!by_id(11).visible && by_id(11).audible);
    assert!(by_id(12).layer > by_id(10).layer);
}

#[test]
fn hidden_tracks_and_clips_without_media_are_left_out() {
    let mut hidden = track(1, TrackKind::Video, vec![clip(0.0, 0.0, 4.0)]);
    hidden.visible = false;
    let mapped = map_with(
        vec![
            hidden,
            track(
                2,
                TrackKind::Video,
                vec![ClipInstance::new(20, 1, 0.0, 0.0, 4.0)],
            ),
        ],
        vec![1],
    );
    assert!(mapped.scene.contributors().is_empty());
}

#[test]
fn constant_speed_changes_the_window_like_core() {
    let mut fast = clip(0.0, 0.0, 8.0);
    fast.speed_factor = 2.0;
    let expected = fast.duration_secs();
    let (_, contributor) = one_clip(fast);
    assert_eq!(contributor.placement.speed, 2.0);
    assert!((contributor.timeline_duration_secs().unwrap() - expected).abs() < 1e-9);
}

#[test]
fn a_speed_ramp_matches_core_duration_and_source_time() {
    let mut ramped = clip(0.0, 0.0, 8.0);
    ramped.speed_factor = 1.0;
    ramped.speed_ramp_end_factor = Some(3.0);
    let expected = ramped.duration_secs();
    let at_half = avcore::keyframe::smooth_speed_ramp_source_secs_at(
        expected / 2.0,
        ramped.source_out_secs - ramped.source_in_secs,
        1.0,
        3.0,
    );
    let (_, contributor) = one_clip(ramped);
    assert!((contributor.timeline_duration_secs().unwrap() - expected).abs() < 1e-9);
    assert!((contributor.source_offset_at(expected / 2.0) - at_half).abs() < 1e-9);
}

#[test]
fn a_frozen_clip_holds_one_frame_for_its_whole_window_and_is_silent() {
    let mut frozen = clip(1.0, 3.0, 7.0);
    frozen.frozen = true;
    let expected = frozen.duration_secs();
    let (mapped, contributor) = one_clip(frozen);
    assert!(!contributor.audible);
    assert!(mapped
        .approximations
        .contains(&Approximation::FrozenAudioMuted));
    assert!((contributor.timeline_duration_secs().unwrap() - expected).abs() < 1e-6);
    let ContributorContent::Media(source) = &contributor.content else {
        panic!("media expected");
    };
    assert!(source.range.out_secs - source.range.in_secs < 0.01);
    // Halfway through the window the source time has barely moved off the held frame.
    let offset = contributor.source_offset_at(expected / 2.0);
    assert!(offset < 0.01, "source moved {offset} s");
}

#[test]
fn opacity_and_position_keyframes_become_linear_tracks_over_the_clip_duration() {
    let mut animated = clip(2.0, 0.0, 10.0);
    animated.opacity_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    animated.position_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: Position { x: 0.0, y: 0.0 },
        },
        Keyframe {
            time_fraction: 1.0,
            value: Position { x: 0.5, y: -0.25 },
        },
    ];
    let (_, contributor) = one_clip(animated);
    // Clip spans 2..12 s; halfway is 7 s.
    let halfway = contributor.placement_at(7.0);
    assert!((halfway.opacity - 0.5).abs() < 1e-6);
    assert_eq!(halfway.x, (0.25 * CANVAS.0 as f32).round() as i32);
    assert_eq!(halfway.y, (-0.125 * CANVAS.1 as f32).round() as i32);
}

#[test]
fn ken_burns_zoom_is_an_animated_centred_crop() {
    let mut zoomed = clip(0.0, 0.0, 10.0);
    zoomed.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 2.0,
        },
    ];
    let (mapped, contributor) = one_clip(zoomed);
    assert!(mapped.approximations.is_empty());
    let end = contributor.placement_at(10.0).transform.crop;
    assert!((end.width - 0.5).abs() < 1e-4 && (end.height - 0.5).abs() < 1e-4);
    assert!((end.x - 0.25).abs() < 1e-4 && (end.y - 0.25).abs() < 1e-4);
    // Halfway the zoom is 1.5x, so the window is 1/1.5 wide (not the linear 0.75).
    let middle = contributor.placement_at(5.0).transform.crop;
    assert!((middle.width - 1.0 / 1.5).abs() < 0.01, "{}", middle.width);
}

#[test]
fn zooming_out_is_held_and_reported() {
    let mut zoomed = clip(0.0, 0.0, 4.0);
    zoomed.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.5,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    let (mapped, contributor) = one_clip(zoomed);
    assert!(mapped.approximations.contains(&Approximation::ZoomOutHeld));
    assert_eq!(contributor.placement_at(0.0).transform.crop.width, 1.0);
}

#[test]
fn flip_filters_masks_and_blend_modes_map_across() {
    let mut styled = clip(0.0, 0.0, 4.0);
    styled.flipped_h = true;
    styled.color_filter = ColorFilter::Sepia;
    styled.mask_shape = CoreMask::RoundedRect;
    styled.mask_corner_radius = 0.3;
    styled.blend_mode = CoreBlend::Multiply;
    styled.layer_scale_x = 0.5;
    styled.layer_scale_y = 0.5;
    let (_, contributor) = one_clip(styled);
    let placement = &contributor.placement;
    assert!(placement.transform.flip_x);
    assert_eq!(placement.effects.sepia, 1.0);
    assert_eq!(placement.blend, BlendMode::Multiply);
    assert_eq!(placement.transform.scale_x, 0.5);
    let mask = placement.effects.mask.unwrap();
    assert_eq!(mask.shape, MaskShape::Rectangle);
    assert!((mask.corner_radius - 0.3).abs() < 1e-6);

    let mut grey = clip(0.0, 0.0, 4.0);
    grey.color_filter = ColorFilter::BlackAndWhite;
    grey.mask_shape = CoreMask::Circle;
    let (_, contributor) = one_clip(grey);
    assert_eq!(contributor.placement.effects.saturation, 0.0);
    assert_eq!(
        contributor.placement.effects.mask.unwrap().shape,
        MaskShape::Ellipse
    );
}

#[test]
fn every_core_blend_mode_has_its_bush_counterpart() {
    for mode in CoreBlend::ALL {
        let mut clip = clip(0.0, 0.0, 4.0);
        clip.blend_mode = mode;
        let (_, contributor) = one_clip(clip);
        assert_eq!(
            contributor.placement.blend.ffmpeg_name(),
            mode.ffmpeg_name(),
            "{mode:?}"
        );
    }
}

#[test]
fn a_fade_transition_becomes_the_placement_fade() {
    let mut faded = clip(0.0, 0.0, 6.0);
    faded.transition_in = TransitionType::Fade;
    faded.transition_duration_secs = 1.5;
    let (_, contributor) = one_clip(faded);
    assert_eq!(contributor.placement.fade.in_secs, 1.5);
    assert!((contributor.placement_at(0.75).opacity - 0.5).abs() < 1e-6);
}

#[test]
fn a_slide_transition_is_skipped_when_position_keyframes_own_the_axis() {
    let mut slid = clip(0.0, 0.0, 6.0);
    slid.transition_in = TransitionType::Slide;
    slid.transition_duration_secs = 1.0;
    let (mapped, contributor) = one_clip(slid.clone());
    assert!(mapped.approximations.is_empty());
    assert_eq!(contributor.placement_at(0.0).x, -(CANVAS.0 as i32));
    assert_eq!(contributor.placement_at(1.0).x, 0);

    slid.position_keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: Position { x: 0.1, y: 0.0 },
    }];
    let (mapped, _) = one_clip(slid);
    assert!(mapped
        .approximations
        .contains(&Approximation::TransitionIgnored));
}

#[test]
fn filter_intensities_stay_inside_bush_limits_for_extreme_values() {
    let mut extreme = clip(0.0, 0.0, 4.0);
    extreme.blur_intensity = 9.0;
    extreme.sharpen = 9.0;
    extreme.shake_intensity = 9.0;
    extreme.glitch_intensity = 9.0;
    extreme.pixelize_intensity = 9.0;
    extreme.brightness = 9.0;
    extreme.contrast = 99.0;
    extreme.saturation = -4.0;
    extreme.gain_db = 60.0;
    // SceneSnapshot::new validates every placement, so mapping succeeding is the assertion.
    let (_, contributor) = one_clip(extreme);
    assert!(contributor.placement.blur_sigma <= bush::MAX_BLUR_SIGMA);
    assert!(contributor.placement.gain <= 4.0);
}

fn overlay_track(kind: TrackKind, text: Vec<TextClip>, shapes: Vec<ShapeClip>) -> Track {
    let mut overlay = track(9, kind, Vec::new());
    overlay.text_clips = text;
    overlay.shape_clips = shapes;
    overlay
}

#[test]
fn text_and_shape_clips_become_timed_full_canvas_raster_overlays() {
    let mut shape = ShapeClip::new(5, 1.0);
    shape.duration_secs = 2.0;
    let mut caption = TextClip::new(6, 0.5, "hello".into());
    caption.duration_secs = 1.5;
    let mapped = map_with(
        vec![
            track(1, TrackKind::Video, vec![clip(0.0, 0.0, 4.0)]),
            overlay_track(TrackKind::Shape, Vec::new(), vec![shape]),
            overlay_track(TrackKind::Text, vec![caption], Vec::new()),
        ],
        Vec::new(),
    );
    let overlays: Vec<&Contributor> = mapped
        .scene
        .contributors()
        .iter()
        .filter(|c| matches!(c.content, ContributorContent::RasterOverlay(_)))
        .collect();
    assert_eq!(overlays.len(), 2);
    assert_eq!(mapped.overlays.len(), 2);
    for (id, frame) in &mapped.overlays {
        assert!(overlays.iter().any(|c| c.id == *id));
        assert_eq!((frame.width(), frame.height()), CANVAS);
    }
    let by_start = |start: f64| {
        overlays
            .iter()
            .find(|c| c.placement.timeline_start_secs == start)
            .unwrap()
    };
    assert_eq!(by_start(1.0).placement.duration_secs, Some(2.0));
    assert_eq!(by_start(0.5).placement.duration_secs, Some(1.5));
    // Overlays paint above every video layer, text above shapes.
    let video_layer = mapped
        .scene
        .contributors()
        .iter()
        .find(|c| matches!(c.content, ContributorContent::Media(_)))
        .unwrap()
        .layer;
    assert!(by_start(1.0).layer > video_layer);
    assert!(by_start(0.5).layer > by_start(1.0).layer);
}

#[test]
fn unchanged_overlays_are_not_rasterized_again() {
    let tracks = || {
        vec![
            track(1, TrackKind::Video, vec![clip(0.0, 0.0, 4.0)]),
            overlay_track(TrackKind::Shape, Vec::new(), vec![ShapeClip::new(5, 0.0)]),
        ]
    };
    let media = vec![asset(1, MediaKind::Video)];
    let mut cache = OverlayCache::default();
    let map = |cache: &mut OverlayCache| {
        map_timeline(
            &timeline(tracks()),
            &media,
            CANVAS,
            &mut Resolver {
                missing: Vec::new(),
            },
            cache,
            SceneVersions::default(),
        )
        .unwrap()
    };
    let first = map(&mut cache);
    let second = map(&mut cache);
    // The cache hands back the very same frame, not an equal copy.
    assert!(std::sync::Arc::ptr_eq(
        &first.overlays[0].1,
        &second.overlays[0].1
    ));
}

#[test]
fn gain_keyframes_become_a_decibel_track() {
    let mut gained = clip(0.0, 0.0, 4.0);
    gained.gain_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: -6.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 6.0,
        },
    ];
    let (_, contributor) = one_clip(gained);
    let track = contributor.placement.animation.gain_db.as_ref().unwrap();
    assert!((track.value_at(0.0) - -6.0).abs() < 1e-6);
    assert!((track.value_at(2.0) - 0.0).abs() < 1e-5);
    assert!((track.value_at(4.0) - 6.0).abs() < 1e-5);
}
