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

use std::collections::HashMap;

use avcore::timeline::{
    AudioRole, BlendMode, ClipInstance, ColorFilter, MarkerKind, MaskShape, ShapeClip, ShapeKind,
    Timeline, Track, TrackKind, TransitionType,
};
use avcore::ClipFormatting;
use avcore::{Keyframe, Position};

fn clip(id: u64, start_secs: f64, source_in_secs: f64, source_out_secs: f64) -> ClipInstance {
    ClipInstance {
        id,
        asset_id: 1,
        start_secs,
        source_in_secs,
        source_out_secs,
        composite_id: None,
        color_label: None,
        gain_db: 0.0,
        frozen: false,
        speed_factor: 1.0,
        speed_ramp_end_factor: None,
        nested_sequence_id: None,
        crop_x: 0.0,
        crop_y: 0.0,
        crop_w: 1.0,
        crop_h: 1.0,
        mask_shape: MaskShape::None,
        mask_corner_radius: 0.0,
        flipped_h: false,
        color_filter: ColorFilter::None,
        vignette_intensity: 0.0,
        brightness: 0.0,
        contrast: 1.0,
        saturation: 1.0,
        sharpen: 0.0,
        chroma_key_enabled: false,
        chroma_key_color: [0, 255, 0],
        chroma_key_tolerance: 0.4,
        blur_intensity: 0.0,
        shake_intensity: 0.0,
        glitch_intensity: 0.0,
        pixelize_intensity: 0.0,
        transition_in: TransitionType::None,
        transition_duration_secs: 0.5,
        fade_in_secs: 0.0,
        fade_out_secs: 0.0,
        position_keyframes: vec![],
        scale_keyframes: vec![],
        rotation_keyframes: vec![],
        opacity_keyframes: vec![],
        gain_keyframes: vec![],
        voice_cleanup_enabled: false,
        voice_cleanup_noise_floor_db: -30.0,
        voice_cleanup_compressor_threshold_db: -18.0,
        voice_cleanup_compressor_ratio: 3.0,
        voice_cleanup_ceiling_linear: 0.95,
        brightness_keyframes: vec![],
        contrast_keyframes: vec![],
        saturation_keyframes: vec![],
        crop_x_keyframes: vec![],
        crop_y_keyframes: vec![],
        crop_w_keyframes: vec![],
        crop_h_keyframes: vec![],
        deflicker_enabled: false,
        lut_path: String::new(),
        layer_scale_x: 1.0,
        layer_scale_y: 1.0,
        stabilization_intensity: 0.0,
        background_removal_enabled: false,
        background_removal_mask_path: String::new(),
        blend_mode: BlendMode::Normal,
        anchor_x: 0.5,
        anchor_y: 0.5,
        reframe_seed_point: None,
        privacy_blur_enabled: false,
        privacy_blur_mask_path: String::new(),
        privacy_blur_sigma: 15.0,
        privacy_blur_seed_vertices: Vec::new(),
        privacy_blur_seed_center_x_frac: 0.0,
        privacy_blur_seed_center_y_frac: 0.0,
    }
}

#[test]
fn clip_duration_is_out_minus_in() {
    assert_eq!(clip(1, 0.0, 10.0, 30.0).duration_secs(), 20.0);
}

#[test]
fn zero_gain_db_is_unity_linear_gain() {
    assert_eq!(clip(1, 0.0, 0.0, 10.0).gain_linear(), 1.0);
}

#[test]
fn positive_gain_db_scales_linear_gain_above_unity() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.gain_db = 6.0;
    assert!((c.gain_linear() - 1.9953).abs() < 0.001);
}

#[test]
fn negative_gain_db_scales_linear_gain_below_unity() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.gain_db = -6.0;
    assert!((c.gain_linear() - 0.5012).abs() < 0.001);
}

#[test]
fn video_filter_chain_is_empty_for_a_neutral_clip() {
    assert_eq!(clip(1, 0.0, 0.0, 10.0).video_filter_chain(), "");
}

#[test]
fn video_filter_chain_includes_crop_when_cropped() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.crop_x = 0.1;
    c.crop_y = 0.2;
    c.crop_w = 0.5;
    c.crop_h = 0.6;
    assert_eq!(c.video_filter_chain(), "crop=iw*0.5:ih*0.6:iw*0.1:ih*0.2");
}

#[test]
fn video_filter_chain_adjusts_color_with_a_lut_filter() {
    // The shipped FFmpeg is an LGPL build: no `eq`, so the adjustment is a `lutyuv`.
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.brightness = 0.2;
    assert_eq!(
        c.video_filter_chain(),
        "lutyuv=y='clip((val/maxval-0.5)*1.0000000+0.5+(0.2000000),0,1)*maxval'"
    );
}

#[test]
fn video_filter_chain_maps_black_and_white_to_hue_desaturate() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.color_filter = ColorFilter::BlackAndWhite;
    assert_eq!(c.video_filter_chain(), "hue=s=0");
}

#[test]
fn video_filter_chain_maps_sepia_to_colorchannelmixer() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.color_filter = ColorFilter::Sepia;
    assert_eq!(
        c.video_filter_chain(),
        "colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131:0"
    );
}

#[test]
fn video_filter_chain_includes_colorkey_when_chroma_keyed() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.chroma_key_enabled = true;
    c.chroma_key_color = [0, 255, 0];
    c.chroma_key_tolerance = 0.4;
    assert_eq!(c.video_filter_chain(), "colorkey=0x00ff00:0.400:0.1");
}

#[test]
fn video_filter_chain_includes_lut3d_when_a_lut_is_set() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.lut_path = "C:/luts/cinematic.cube".to_string();
    assert_eq!(
        c.video_filter_chain(),
        "lut3d=file='C:/luts/cinematic.cube'"
    );
}

#[test]
fn video_filter_chain_escapes_backslashes_and_quotes_in_the_lut_path() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.lut_path = r"C:\luts\bob's cube.cube".to_string();
    assert_eq!(
        c.video_filter_chain(),
        "lut3d=file='C:/luts/bob'\\''s cube.cube'"
    );
}

#[test]
fn video_filter_chain_orders_lut_after_color_filter_before_chroma_key() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.color_filter = ColorFilter::Sepia;
    c.lut_path = "cinematic.cube".to_string();
    c.chroma_key_enabled = true;
    let chain = c.video_filter_chain();
    let color_pos = chain.find("colorchannelmixer").unwrap();
    let lut_pos = chain.find("lut3d").unwrap();
    let colorkey_pos = chain.find("colorkey").unwrap();
    assert!(color_pos < lut_pos && lut_pos < colorkey_pos);
}

#[test]
fn split_clip_at_keeps_lut_on_both_halves() {
    let mut t = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    t.clips[0].lut_path = "cinematic.cube".to_string();
    t.split_clip_at(4.0, 2);
    assert_eq!(t.clips[0].lut_path, "cinematic.cube");
    assert_eq!(t.clips[1].lut_path, "cinematic.cube");
}

#[test]
fn video_filter_chain_includes_a_circle_mask_when_masked() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.mask_shape = MaskShape::Circle;
    // The shape is drawn once on the first frame and merged in as every frame's alpha plane.
    assert_eq!(
        c.video_filter_chain(),
        "split[km1a][km1b];\
         [km1b]trim=end_frame=1,geq=lum='255*lte(pow(X-W/2,2)+pow(Y-H/2,2),pow(min(W,H)/2,2))',\
         extractplanes=y[km1m];\
         [km1a][km1m]alphamerge"
    );
}

#[test]
fn video_filter_chain_includes_a_rounded_rect_mask_when_masked() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.mask_shape = MaskShape::RoundedRect;
    c.mask_corner_radius = 0.3;
    let chain = c.video_filter_chain();
    assert!(
        chain.starts_with("split[km1a][km1b];[km1b]trim=end_frame=1,geq=lum='255*lte(sqrt(pow(")
    );
    assert!(chain.contains("min(0.3000*min(W,H),min(W,H)/2)"));
    assert!(chain.ends_with("[km1a][km1m]alphamerge"));
}

#[test]
fn mask_labels_are_unique_per_clip() {
    let mut a = clip(7, 0.0, 0.0, 10.0);
    a.mask_shape = MaskShape::Circle;
    let mut b = clip(8, 0.0, 0.0, 10.0);
    b.mask_shape = MaskShape::Circle;
    assert!(a.video_filter_chain().contains("[km7m]"));
    assert!(b.video_filter_chain().contains("[km8m]"));
    assert!(!b.video_filter_chain().contains("km7"));
}

#[test]
fn a_mask_over_chroma_key_or_animated_opacity_still_multiplies_the_existing_alpha() {
    let mut keyed = clip(1, 0.0, 0.0, 10.0);
    keyed.mask_shape = MaskShape::Circle;
    keyed.chroma_key_enabled = true;
    assert!(keyed
        .video_filter_chain()
        .contains("geq=lum='p(X,Y)':cb='cb(X,Y)':cr='cr(X,Y)':a='alpha(X,Y)*lte("));

    let mut faded = clip(1, 0.0, 0.0, 10.0);
    faded.mask_shape = MaskShape::Circle;
    faded.opacity_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    assert!(faded.video_filter_chain().contains("alpha(X,Y)*lte("));
}

#[test]
fn animated_opacity_is_evaluated_on_one_pixel_not_the_whole_frame() {
    let mut c = clip(3, 0.0, 0.0, 10.0);
    c.opacity_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.0,
        },
    ];
    let chain = c.keyframe_video_filter_chain(30, 1, 10.0, false).unwrap();
    assert!(chain.starts_with(
        "split=3[ko3a][ko3b][ko3c];[ko3b]scale=1:1:flags=neighbor,format=gray,geq=lum='255*("
    ));
    assert!(chain.ends_with("[ko3a][ko3s]alphamerge"));
    assert!(!chain.contains("alpha(X,Y)"));
}

#[test]
fn video_filter_chain_orders_mask_after_chroma_key_before_blur() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.chroma_key_enabled = true;
    c.chroma_key_color = [0, 255, 0];
    c.chroma_key_tolerance = 0.4;
    c.mask_shape = MaskShape::Circle;
    c.blur_intensity = 0.5;
    let chain = c.video_filter_chain();
    let colorkey_pos = chain.find("colorkey").unwrap();
    let mask_pos = chain.find("geq").unwrap();
    let blur_pos = chain.find("avgblur").unwrap();
    assert!(colorkey_pos < mask_pos && mask_pos < blur_pos);
}

#[test]
fn video_filter_chain_includes_avgblur_when_blurred() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.blur_intensity = 0.5;
    assert_eq!(c.video_filter_chain(), "avgblur=sizeX=5:sizeY=5");
}

#[test]
fn video_filter_chain_includes_deflicker_when_enabled() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.deflicker_enabled = true;
    assert_eq!(c.video_filter_chain(), "deflicker=mode=am:size=5");
}

#[test]
fn video_filter_chain_orders_deflicker_after_crop_before_color() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.deflicker_enabled = true;
    c.crop_w = 0.5;
    c.color_filter = ColorFilter::Sepia;
    assert_eq!(
        c.video_filter_chain(),
        "crop=iw*0.5:ih*1:iw*0:ih*0,deflicker=mode=am:size=5,\
         colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.534:.131:0"
    );
}

#[test]
fn video_filter_chain_includes_deshake_when_stabilized() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.stabilization_intensity = 0.0;
    assert_eq!(c.video_filter_chain(), "");
    c.stabilization_intensity = 1.0;
    assert_eq!(c.video_filter_chain(), "deshake=rx=64:ry=64:edge=mirror");
}

#[test]
fn video_filter_chain_scales_stabilization_radius_with_intensity() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.stabilization_intensity = 0.5;
    assert_eq!(c.video_filter_chain(), "deshake=rx=34:ry=34:edge=mirror");
}

#[test]
fn video_filter_chain_orders_stabilization_after_deflicker_before_color() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.deflicker_enabled = true;
    c.stabilization_intensity = 1.0;
    c.color_filter = ColorFilter::Sepia;
    let chain = c.video_filter_chain();
    let deflicker_pos = chain.find("deflicker").unwrap();
    let deshake_pos = chain.find("deshake").unwrap();
    let color_pos = chain.find("colorchannelmixer").unwrap();
    assert!(deflicker_pos < deshake_pos && deshake_pos < color_pos);
}

#[test]
fn split_clip_at_keeps_stabilization_on_both_halves() {
    let mut t = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    t.clips[0].stabilization_intensity = 0.75;
    t.split_clip_at(4.0, 2);
    assert_eq!(t.clips[0].stabilization_intensity, 0.75);
    assert_eq!(t.clips[1].stabilization_intensity, 0.75);
}

#[test]
fn video_filter_chain_includes_unsharp_when_sharpened() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.sharpen = 0.5;
    assert_eq!(c.video_filter_chain(), "unsharp=5:5:1.50:5:5:0.0");
}

#[test]
fn video_filter_chain_includes_vignette_when_vignetted() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.vignette_intensity = 0.8;
    assert_eq!(c.video_filter_chain(), "vignette=PI/4*0.800");
}

#[test]
fn video_filter_chain_includes_hflip_last_when_flipped() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.flipped_h = true;
    c.vignette_intensity = 0.5;
    assert_eq!(c.video_filter_chain(), "vignette=PI/4*0.500,hflip");
}

#[test]
fn video_filter_chain_orders_crop_before_color_before_flip() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.crop_w = 0.5;
    c.color_filter = ColorFilter::Sepia;
    c.flipped_h = true;
    assert_eq!(
        c.video_filter_chain(),
        "crop=iw*0.5:ih*1:iw*0:ih*0,colorchannelmixer=.393:.769:.189:0:.349:.686:.168:0:.272:.\
         534:.131:0,hflip"
    );
}

#[test]
fn empty_timeline_has_zero_duration() {
    let timeline = Timeline {
        tracks: vec![],
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    assert_eq!(timeline.duration_secs(), 0.0);
}

#[test]
fn timeline_duration_is_the_furthest_clip_end_across_all_tracks() {
    let timeline = Timeline {
        tracks: vec![
            Track {
                id: 1,
                name: "V1".to_string(),
                kind: TrackKind::Video,
                clips: vec![clip(1, 0.0, 0.0, 30.0), clip(2, 30.0, 0.0, 44.0)],

                text_clips: vec![],
                shape_clips: vec![],

                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                locked: false,
                mix: Default::default(),
                color_label: None,
            },
            Track {
                id: 2,
                name: "A2".to_string(),
                kind: TrackKind::Audio,
                // Shorter overall, so it must not win over the V1 track's later end.
                clips: vec![clip(3, 0.0, 0.0, 10.0)],

                text_clips: vec![],
                shape_clips: vec![],

                visible: true,
                audio_role: AudioRole::Unspecified,
                muted: false,
                solo: false,
                locked: false,
                mix: Default::default(),
                color_label: None,
            },
        ],
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    };
    // Track V1's second clip ends at 30 + (44 - 0) = 74.
    assert_eq!(timeline.duration_secs(), 74.0);
}

fn track_with(clips: Vec<ClipInstance>) -> Track {
    Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips,

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    }
}

#[test]
fn clips_that_follow_each_other_share_lane_zero_and_an_overlap_opens_lane_one() {
    let track = track_with(vec![
        clip(1, 0.0, 0.0, 4.0),
        clip(2, 4.0, 0.0, 4.0),
        clip(3, 3.5, 0.0, 2.0),
    ]);
    let lanes = track.overlap_lanes();
    assert_eq!(lanes[&1], 0);
    assert_eq!(lanes[&3], 1, "clip 3 starts inside clip 1");
    assert_eq!(lanes[&2], 0, "clip 2 starts when clip 1 ends");
    let plain = track_with(vec![clip(1, 0.0, 0.0, 4.0), clip(2, 4.0, 0.0, 4.0)]);
    assert!(plain.overlap_lanes().values().all(|&lane| lane == 0));
}

#[test]
fn a_dissolve_overlaps_the_previous_clip_and_pulls_the_following_clips_earlier() {
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 4.0),
        clip(2, 4.0, 0.0, 4.0),
        clip(3, 8.0, 0.0, 4.0),
    ]);
    assert!(track.apply_dissolve_overlap(2, 1.0));
    let start =
        |track: &Track, id: u64| track.clips.iter().find(|c| c.id == id).unwrap().start_secs;
    assert_eq!(start(&track, 1), 0.0);
    assert_eq!(start(&track, 2), 3.0);
    assert_eq!(start(&track, 3), 7.0);
    assert_eq!(track.overlap_with_earlier_secs(2), 1.0);
    assert_eq!(track.overlap_lanes()[&2], 1);
}

#[test]
fn a_dissolve_is_limited_to_half_of_the_shorter_clip_and_needs_a_clip_right_before() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 1.0), clip(2, 1.0, 0.0, 4.0)]);
    assert!(track.apply_dissolve_overlap(2, 3.0));
    assert_eq!(track.clips[1].start_secs, 0.5, "half of the 1 s clip");

    let mut gap = track_with(vec![clip(1, 0.0, 0.0, 1.0), clip(2, 3.0, 0.0, 4.0)]);
    assert!(!gap.apply_dissolve_overlap(2, 0.5), "a gap is not a cut");
    assert_eq!(gap.clips[1].start_secs, 3.0);
    let mut first = track_with(vec![clip(1, 0.0, 0.0, 1.0)]);
    assert!(!first.apply_dissolve_overlap(1, 0.5));
}

#[test]
fn a_dissolving_clips_keyframes_are_rescaled_over_its_head_and_tail() {
    // Scale animates 1.0 -> 2.0 over the whole 10 s clip, which overlaps the clip before it for
    // its first 4 s: the head must run 1.0 -> 1.4 and the tail 1.4 -> 2.0, not the full ramp each.
    let mut second = clip(2, 6.0, 0.0, 10.0);
    second.transition_in = TransitionType::Dissolve;
    second.transition_duration_secs = 4.0;
    second.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 2.0,
        },
    ];
    let track = track_with(vec![clip(1, 0.0, 0.0, 10.0), second]);
    let pieces = track.lane_pieces();
    let head = pieces
        .iter()
        .find(|(c, lane)| c.id == 2 && *lane == 1)
        .map(|(c, _)| c)
        .unwrap();
    let tail = pieces
        .iter()
        .find(|(c, lane)| c.id == 2 && *lane == 0)
        .map(|(c, _)| c)
        .unwrap();
    let ends = |keys: &[Keyframe<f32>]| (keys.first().unwrap().value, keys.last().unwrap().value);
    let (head_start, head_end) = ends(&head.scale_keyframes);
    let (tail_start, tail_end) = ends(&tail.scale_keyframes);
    assert!((head_start - 1.0).abs() < 1e-5 && (head_end - 1.4).abs() < 1e-5);
    assert!((tail_start - 1.4).abs() < 1e-5 && (tail_end - 2.0).abs() < 1e-5);
    assert_eq!(head.duration_secs(), 4.0);
    assert_eq!(tail.duration_secs(), 6.0);
    assert_eq!(tail.start_secs, 10.0);
}

#[test]
fn dissolve_fades_follow_the_real_overlap() {
    let mut second = clip(2, 3.0, 0.0, 4.0);
    second.transition_in = TransitionType::Dissolve;
    second.transition_duration_secs = 1.5;
    let track = track_with(vec![clip(1, 0.0, 0.0, 4.0), second]);
    // Overlap is 1 s, shorter than the 1.5 s transition.
    assert_eq!(track.dissolve_in_secs(&track.clips[1]), 1.0);
    assert_eq!(track.dissolve_out_secs(&track.clips[0]), 1.0);
    assert_eq!(track.dissolve_out_secs(&track.clips[1]), 0.0);

    let mut alone = clip(5, 0.0, 0.0, 2.0);
    alone.transition_in = TransitionType::Dissolve;
    alone.transition_duration_secs = 0.5;
    let lone = track_with(vec![alone]);
    assert_eq!(
        lone.dissolve_in_secs(&lone.clips[0]),
        0.5,
        "no overlap: a plain fade-in"
    );
}

#[test]
fn track_duration_is_zero_for_an_empty_track() {
    assert_eq!(track_with(Vec::new()).duration_secs(), 0.0);
}

#[test]
fn track_duration_is_the_single_clips_end() {
    assert_eq!(
        track_with(vec![clip(1, 5.0, 0.0, 20.0)]).duration_secs(),
        25.0
    );
}

#[test]
fn track_duration_is_the_furthest_clip_end_on_this_track_only() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 50.0, 0.0, 5.0)]);
    // Second clip ends at 50 + 5 = 55, which is furthest even though it's shorter.
    assert_eq!(track.duration_secs(), 55.0);
}

#[test]
fn track_duration_accounts_for_shape_clips() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    track.shape_clips = vec![ShapeClip {
        id: 1,
        start_secs: 50.0,
        duration_secs: 5.0,
        shape_kind: ShapeKind::rectangle(),
        center_x: 0.5,
        center_y: 0.5,
        center_x_keyframes: vec![],
        center_y_keyframes: vec![],
        width_keyframes: vec![],
        height_keyframes: vec![],
        rotation_keyframes: vec![],
        width: 0.3,
        height: 0.3,
        rotation_deg: 0.0,
        color_rgba: [255, 255, 255, 255],
        stroke_thickness_px: 0.0,
    }];
    // The shape clip ends at 50 + 5 = 55, past the video clip's 10 — a shape-only or mixed
    // track must report the furthest end across every clip kind it carries.
    assert_eq!(track.duration_secs(), 55.0);
}

#[test]
fn clip_at_finds_the_clip_covering_a_position() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 5.0)]);
    assert_eq!(track.clip_at(3.0).unwrap().id, 1);
    assert_eq!(track.clip_at(12.0).unwrap().id, 2);
}

#[test]
fn clip_at_is_inclusive_of_a_clips_start() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 5.0)]);
    assert_eq!(track.clip_at(10.0).unwrap().id, 2);
}

#[test]
fn clip_at_is_exclusive_of_a_clips_end() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    assert!(track.clip_at(10.0).is_none());
}

#[test]
fn clip_at_is_none_for_a_gap_between_clips() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 5.0), clip(2, 20.0, 0.0, 5.0)]);
    assert!(track.clip_at(10.0).is_none());
}

#[test]
fn clip_at_is_none_for_an_empty_track() {
    assert!(track_with(Vec::new()).clip_at(0.0).is_none());
}

#[test]
fn split_clip_at_divides_the_covering_clip_into_two() {
    let mut track = track_with(vec![clip(1, 10.0, 0.0, 20.0)]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert_eq!(
        track.clips,
        vec![clip(1, 10.0, 0.0, 10.0), clip(99, 20.0, 10.0, 20.0)]
    );
}

#[test]
fn split_clip_at_keeps_both_halves_in_the_same_composite_group() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.composite_id = Some(7);
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert_eq!(track.clips[0].composite_id, Some(7));
    assert_eq!(track.clips[1].composite_id, Some(7));
}

#[test]
fn split_clip_at_keeps_gain_db_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.gain_db = 3.0;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert_eq!(track.clips[0].gain_db, 3.0);
    assert_eq!(track.clips[1].gain_db, 3.0);
}

#[test]
fn split_clip_at_keeps_frozen_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.frozen = true;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert!(track.clips[0].frozen);
    assert!(track.clips[1].frozen);
}

#[test]
fn new_clip_defaults_to_normal_speed() {
    assert_eq!(clip(1, 0.0, 0.0, 10.0).speed_factor, 1.0);
}

#[test]
fn split_clip_at_keeps_speed_factor_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.speed_factor = 2.0;
    let mut track = track_with(vec![clip]);

    // With speed=2.0 the clip's timeline duration is (20-0)/2=10s, spanning 10..20.
    // Split at 15.0 (midpoint) — splitting at the former end boundary (20.0) would now
    // be outside the clip and return false.
    let split = track.split_clip_at(15.0, 99);

    assert!(split);
    assert_eq!(track.clips[0].speed_factor, 2.0);
    assert_eq!(track.clips[1].speed_factor, 2.0);
}

#[test]
fn split_clip_at_continues_a_smooth_speed_ramp_across_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.speed_factor = 1.0;
    clip.speed_ramp_end_factor = Some(3.0);
    let full_duration = clip.duration_secs();
    let mut track = track_with(vec![clip]);

    // Split partway through the ramp's own timeline duration (not the source range).
    let split_at = 10.0 + full_duration * 0.4;
    let split = track.split_clip_at(split_at, 99);
    assert!(split);

    let first = &track.clips[0];
    let second = &track.clips[1];

    // Each half keeps its own ramp, sharing the exact same speed at the cut — no jump.
    assert_eq!(first.speed_factor, 1.0);
    assert_eq!(second.speed_ramp_end_factor, Some(3.0));
    assert_eq!(first.speed_ramp_end_factor, Some(second.speed_factor));

    // Speed at the cut lands strictly between the original start/end speeds.
    let speed_at_cut = second.speed_factor;
    assert!(speed_at_cut > 1.0 && speed_at_cut < 3.0);

    // Source ranges stay contiguous, same invariant every other split has.
    assert_eq!(first.source_out_secs, second.source_in_secs);
    assert_eq!(first.source_in_secs, 0.0);
    assert_eq!(second.source_out_secs, 20.0);

    // The two halves' own durations still sum to (approximately) the original ramp's whole
    // timeline duration — splitting shouldn't change the clip's total on-timeline length.
    assert!((first.duration_secs() + second.duration_secs() - full_duration).abs() < 1e-3);
}

#[test]
fn new_clip_defaults_to_an_uncropped_full_frame() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.is_cropped());
    assert_eq!(
        (c.crop_x, c.crop_y, c.crop_w, c.crop_h),
        (0.0, 0.0, 1.0, 1.0)
    );
}

#[test]
fn is_cropped_is_true_when_any_crop_field_differs_from_the_full_frame() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.crop_w = 0.5;
    assert!(c.is_cropped());
}

#[test]
fn split_clip_at_keeps_crop_rect_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.crop_x = 0.1;
    clip.crop_y = 0.2;
    clip.crop_w = 0.5;
    clip.crop_h = 0.6;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(
            (half.crop_x, half.crop_y, half.crop_w, half.crop_h),
            (0.1, 0.2, 0.5, 0.6)
        );
    }
}

#[test]
fn new_clip_defaults_to_unmasked() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.is_masked());
    assert_eq!(c.mask_shape, MaskShape::None);
}

#[test]
fn is_masked_is_true_for_any_shape_but_none() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.mask_shape = MaskShape::Circle;
    assert!(c.is_masked());
}

#[test]
fn split_clip_at_keeps_mask_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.mask_shape = MaskShape::RoundedRect;
    clip.mask_corner_radius = 0.3;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.mask_shape, MaskShape::RoundedRect);
        assert_eq!(half.mask_corner_radius, 0.3);
    }
}

#[test]
fn new_clip_defaults_to_unflipped() {
    assert!(!clip(1, 0.0, 0.0, 10.0).flipped_h);
}

#[test]
fn split_clip_at_keeps_flip_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.flipped_h = true;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert!(half.flipped_h);
    }
}

#[test]
fn new_clip_defaults_to_unfiltered() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.is_color_filtered());
    assert_eq!(c.color_filter, ColorFilter::None);
}

#[test]
fn is_color_filtered_is_true_for_any_filter_but_none() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.color_filter = ColorFilter::Sepia;
    assert!(c.is_color_filtered());
}

#[test]
fn split_clip_at_keeps_color_filter_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.color_filter = ColorFilter::BlackAndWhite;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.color_filter, ColorFilter::BlackAndWhite);
    }
}

#[test]
fn new_clip_defaults_to_no_vignette() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.has_vignette());
    assert_eq!(c.vignette_intensity, 0.0);
}

#[test]
fn has_vignette_is_true_above_zero() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.vignette_intensity = 0.4;
    assert!(c.has_vignette());
}

#[test]
fn split_clip_at_keeps_vignette_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.vignette_intensity = 0.6;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.vignette_intensity, 0.6);
    }
}

#[test]
fn new_clip_defaults_to_unchanged_color_adjustment() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert_eq!((c.brightness, c.contrast, c.saturation), (0.0, 1.0, 1.0));
}

#[test]
fn split_clip_at_keeps_color_adjustment_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.brightness = 0.2;
    clip.contrast = 1.5;
    clip.saturation = 0.5;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(
            (half.brightness, half.contrast, half.saturation),
            (0.2, 1.5, 0.5)
        );
    }
}

#[test]
fn new_clip_defaults_to_unsharpened() {
    assert_eq!(clip(1, 0.0, 0.0, 10.0).sharpen, 0.0);
}

#[test]
fn split_clip_at_keeps_sharpen_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.sharpen = 0.4;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.sharpen, 0.4);
    }
}

#[test]
fn new_clip_defaults_to_chroma_key_disabled() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.is_chroma_keyed());
    assert_eq!(c.chroma_key_color, [0, 255, 0]);
}

#[test]
fn split_clip_at_keeps_chroma_key_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.chroma_key_enabled = true;
    clip.chroma_key_color = [10, 200, 30];
    clip.chroma_key_tolerance = 0.7;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert!(half.chroma_key_enabled);
        assert_eq!(half.chroma_key_color, [10, 200, 30]);
        assert_eq!(half.chroma_key_tolerance, 0.7);
    }
}

#[test]
fn new_clip_defaults_to_no_other_effects() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert_eq!(
        (
            c.blur_intensity,
            c.shake_intensity,
            c.glitch_intensity,
            c.pixelize_intensity
        ),
        (0.0, 0.0, 0.0, 0.0)
    );
}

#[test]
fn split_clip_at_keeps_deflicker_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.deflicker_enabled = true;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert!(track.clips[0].deflicker_enabled);
    assert!(track.clips[1].deflicker_enabled);
}

#[test]
fn split_clip_at_keeps_other_effects_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.blur_intensity = 0.1;
    clip.shake_intensity = 0.2;
    clip.glitch_intensity = 0.3;
    clip.pixelize_intensity = 0.4;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(
            (
                half.blur_intensity,
                half.shake_intensity,
                half.glitch_intensity,
                half.pixelize_intensity
            ),
            (0.1, 0.2, 0.3, 0.4)
        );
    }
}

#[test]
fn new_clip_defaults_to_no_transition() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(!c.has_transition());
    assert_eq!(c.transition_in, TransitionType::None);
    assert_eq!(c.transition_duration_secs, 0.5);
}

#[test]
fn split_clip_at_keeps_transition_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.transition_in = TransitionType::Fade;
    clip.transition_duration_secs = 1.2;
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.transition_in, TransitionType::Fade);
        assert_eq!(half.transition_duration_secs, 1.2);
    }
}

#[test]
fn new_clip_defaults_to_no_color_label() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert_eq!(c.color_label, None);
}

#[test]
fn split_clip_at_keeps_the_color_label_on_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.color_label = Some([229, 83, 83]);
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    for half in &track.clips {
        assert_eq!(half.color_label, Some([229, 83, 83]));
    }
}

#[test]
fn new_clip_defaults_to_no_keyframes() {
    let c = clip(1, 0.0, 0.0, 10.0);
    assert!(c.position_keyframes.is_empty());
    assert!(c.scale_keyframes.is_empty());
    assert!(c.rotation_keyframes.is_empty());
    assert!(c.opacity_keyframes.is_empty());
    assert!(c.gain_keyframes.is_empty());
    assert!(c.brightness_keyframes.is_empty());
    assert!(c.contrast_keyframes.is_empty());
    assert!(c.saturation_keyframes.is_empty());
    assert!(c.crop_x_keyframes.is_empty());
    assert!(c.crop_y_keyframes.is_empty());
    assert!(c.crop_w_keyframes.is_empty());
    assert!(c.crop_h_keyframes.is_empty());
    assert!(!c.has_scale_keyframes());
    assert!(!c.has_gain_keyframes());
    assert!(!c.has_color_keyframes());
    assert!(!c.has_crop_keyframes());
}

#[test]
fn video_filter_chain_uses_the_static_crop_stage_with_no_crop_keyframes() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.crop_x = 0.1;
    c.crop_w = 0.5;
    assert_eq!(c.video_filter_chain(), "crop=iw*0.5:ih*1:iw*0.1:ih*0");
}

#[test]
fn video_filter_chain_suppresses_the_static_crop_stage_when_crop_keyframes_are_present() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.crop_x = 0.1;
    c.crop_w = 0.5;
    c.crop_x_keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: 0.2,
    }];
    assert_eq!(c.video_filter_chain(), "");
}

#[test]
fn split_clip_at_rescales_crop_keyframes_onto_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.crop_x_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 0.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.4,
        },
    ];
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert!(track.clips[0].has_crop_keyframes());
    assert!(track.clips[1].has_crop_keyframes());
    assert_eq!(
        track.clips[0].crop_x_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: 0.0
            },
            Keyframe {
                time_fraction: 1.0,
                value: 0.2
            },
        ]
    );
}

#[test]
fn video_filter_chain_uses_the_static_color_stage_with_no_color_keyframes() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.brightness = 0.3;
    assert_eq!(
        c.video_filter_chain(),
        "lutyuv=y='clip((val/maxval-0.5)*1.0000000+0.5+(0.3000000),0,1)*maxval'"
    );
}

#[test]
fn video_filter_chain_suppresses_the_static_eq_stage_when_color_keyframes_are_present() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.brightness = 0.3;
    c.brightness_keyframes = vec![Keyframe {
        time_fraction: 0.0,
        value: -0.5,
    }];
    // The static path defers entirely to keyframe_video_filter_chain -- no eq stage here, even
    // though `brightness` is still non-neutral, so it never gets double-emitted.
    assert_eq!(c.video_filter_chain(), "");
}

#[test]
fn split_clip_at_rescales_color_keyframes_onto_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.brightness_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: -0.5,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.5,
        },
    ];
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert!(track.clips[0].has_color_keyframes());
    assert!(track.clips[1].has_color_keyframes());
    assert_eq!(
        track.clips[0].brightness_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: -0.5
            },
            Keyframe {
                time_fraction: 1.0,
                value: 0.0
            },
        ]
    );
}

#[test]
fn split_clip_at_rescales_gain_keyframes_onto_both_halves() {
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.gain_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: -20.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 0.0,
        },
    ];
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert!(track.clips[0].has_gain_keyframes());
    assert!(track.clips[1].has_gain_keyframes());
    assert_eq!(
        track.clips[0].gain_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: -20.0
            },
            Keyframe {
                time_fraction: 1.0,
                value: -10.0
            },
        ]
    );
    assert_eq!(
        track.clips[1].gain_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: -10.0
            },
            Keyframe {
                time_fraction: 1.0,
                value: 0.0
            },
        ]
    );
}

#[test]
fn split_clip_at_rescales_scale_keyframes_onto_both_halves() {
    // scale_keyframes spans the whole clip (1.0 -> 2.5); splitting at the midpoint should
    // rescale each half's keyframes to its own 0.0..=1.0 range and insert a synthetic
    // boundary keyframe (the interpolated value at the split point) on both halves, so the
    // animation has no jump at the cut — replacing the old zoom_start/zoom_end behavior this
    // is based on, which used to just copy the same two values onto both halves unscaled.
    let mut clip = clip(1, 10.0, 0.0, 20.0);
    clip.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.0,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 2.5,
        },
    ];
    let mut track = track_with(vec![clip]);

    let split = track.split_clip_at(20.0, 99);

    assert!(split);
    assert_eq!(
        track.clips[0].scale_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: 1.0
            },
            Keyframe {
                time_fraction: 1.0,
                value: 1.75
            },
        ]
    );
    assert_eq!(
        track.clips[1].scale_keyframes,
        vec![
            Keyframe {
                time_fraction: 0.0,
                value: 1.75
            },
            Keyframe {
                time_fraction: 1.0,
                value: 2.5
            },
        ]
    );
}

#[test]
fn split_clip_at_only_splits_the_clip_that_covers_the_position() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 20.0)]);

    let split = track.split_clip_at(15.0, 99);

    assert!(split);
    assert_eq!(track.clips[0], clip(1, 0.0, 0.0, 10.0));
    assert_eq!(
        track.clips[1..],
        [clip(2, 10.0, 0.0, 5.0), clip(99, 15.0, 5.0, 20.0)]
    );
}

#[test]
fn split_clip_at_is_a_no_op_when_nothing_covers_the_position() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);

    let split = track.split_clip_at(50.0, 99);

    assert!(!split);
    assert_eq!(track.clips, vec![clip(1, 0.0, 0.0, 10.0)]);
}

#[test]
fn split_clip_at_is_a_no_op_exactly_on_a_clip_boundary() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 20.0)]);

    let split = track.split_clip_at(10.0, 99);

    assert!(!split);
    assert_eq!(
        track.clips,
        vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 20.0)]
    );
}

#[test]
fn trim_start_shifts_start_and_source_in_by_the_same_delta() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    let trimmed = c.trim_start(15.0, 1.0);

    assert!(trimmed);
    assert_eq!(c, clip(1, 15.0, 10.0, 30.0));
}

#[test]
fn trim_start_is_a_no_op_when_it_would_go_negative() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    // Would push start_secs to -1.0.
    let trimmed = c.trim_start(-1.0, 1.0);

    assert!(!trimmed);
    assert_eq!(c, clip(1, 10.0, 5.0, 30.0));
}

#[test]
fn trim_start_is_a_no_op_when_it_would_shrink_below_the_minimum_duration() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    // delta = 24.5 -> source_in becomes 29.5 -> duration becomes 0.5, under the 1.0 minimum.
    let trimmed = c.trim_start(34.5, 1.0);

    assert!(!trimmed);
    assert_eq!(c, clip(1, 10.0, 5.0, 30.0));
}

#[test]
fn trim_end_extends_source_out_and_leaves_start_untouched() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    let trimmed = c.trim_end(25.0, 1.0, None);

    assert!(trimmed);
    // start=10, source_in=5, new duration = 25-10 = 15, so source_out = 5+15 = 20.
    assert_eq!(c, clip(1, 10.0, 5.0, 20.0));
}

#[test]
fn trim_end_is_a_no_op_past_the_source_medias_own_duration() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    // Would need source_out_secs = 35.0, past the 32.0 source duration.
    let trimmed = c.trim_end(40.0, 1.0, Some(32.0));

    assert!(!trimmed);
    assert_eq!(c, clip(1, 10.0, 5.0, 30.0));
}

#[test]
fn trim_end_is_a_no_op_when_it_would_shrink_below_the_minimum_duration() {
    let mut c = clip(1, 10.0, 5.0, 30.0);

    let trimmed = c.trim_end(10.5, 1.0, None);

    assert!(!trimmed);
    assert_eq!(c, clip(1, 10.0, 5.0, 30.0));
}

#[test]
fn clip_mut_finds_a_clip_by_id() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 20.0)]);

    let found = track.clip_mut(2).unwrap();

    assert_eq!(found.start_secs, 10.0);
}

#[test]
fn clip_mut_returns_none_for_an_unknown_id() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);

    assert!(track.clip_mut(99).is_none());
}

#[test]
fn move_clip_repositions_start_secs_and_leaves_the_source_range_untouched() {
    let mut track = track_with(vec![clip(1, 0.0, 5.0, 15.0)]);

    let moved = track.move_clip(1, 40.0);

    assert!(moved);
    assert_eq!(track.clips[0], clip(1, 40.0, 5.0, 15.0));
}

#[test]
fn move_clip_is_a_no_op_for_a_negative_position() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);

    let moved = track.move_clip(1, -5.0);

    assert!(!moved);
    assert_eq!(track.clips[0], clip(1, 0.0, 0.0, 10.0));
}

fn timeline_with(tracks: Vec<Track>) -> Timeline {
    Timeline {
        tracks,
        playhead_secs: 0.0,
        markers: Vec::new(),
        multicam_groups: Vec::new(),
    }
}

#[test]
fn move_clip_to_track_relocates_the_clip_to_a_same_kind_track() {
    let mut timeline = timeline_with(vec![
        Track {
            id: 1,
            name: "V1".to_string(),
            kind: TrackKind::Video,
            clips: vec![clip(1, 0.0, 0.0, 10.0)],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
        Track {
            id: 2,
            name: "V2".to_string(),
            kind: TrackKind::Video,
            clips: vec![],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
    ]);

    let moved = timeline.move_clip_to_track(1, 2, 5.0);

    assert!(moved);
    assert!(timeline.tracks[0].clips.is_empty());
    assert_eq!(timeline.tracks[1].clips, vec![clip(1, 5.0, 0.0, 10.0)]);
}

#[test]
fn move_clip_to_track_is_a_no_op_across_mismatched_kinds() {
    let mut timeline = timeline_with(vec![
        Track {
            id: 1,
            name: "V1".to_string(),
            kind: TrackKind::Video,
            clips: vec![clip(1, 0.0, 0.0, 10.0)],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
        Track {
            id: 2,
            name: "A1".to_string(),
            kind: TrackKind::Audio,
            clips: vec![],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
    ]);

    let moved = timeline.move_clip_to_track(1, 2, 5.0);

    assert!(!moved);
    assert_eq!(timeline.tracks[0].clips, vec![clip(1, 0.0, 0.0, 10.0)]);
    assert!(timeline.tracks[1].clips.is_empty());
}

#[test]
fn move_clip_to_track_is_a_no_op_for_an_unknown_target_track() {
    let mut timeline = timeline_with(vec![Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 0.0, 0.0, 10.0)],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    }]);

    let moved = timeline.move_clip_to_track(1, 99, 5.0);

    assert!(!moved);
    assert_eq!(timeline.tracks[0].clips, vec![clip(1, 0.0, 0.0, 10.0)]);
}

#[test]
fn move_clip_to_track_is_a_no_op_for_a_negative_position() {
    let mut timeline = timeline_with(vec![
        Track {
            id: 1,
            name: "V1".to_string(),
            kind: TrackKind::Video,
            clips: vec![clip(1, 0.0, 0.0, 10.0)],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
        Track {
            id: 2,
            name: "V2".to_string(),
            kind: TrackKind::Video,
            clips: vec![],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
    ]);

    let moved = timeline.move_clip_to_track(1, 2, -5.0);

    assert!(!moved);
    assert_eq!(timeline.tracks[0].clips, vec![clip(1, 0.0, 0.0, 10.0)]);
    assert!(timeline.tracks[1].clips.is_empty());
}

#[test]
fn move_clip_is_a_no_op_for_an_unknown_clip_id() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);

    let moved = track.move_clip(99, 5.0);

    assert!(!moved);
    assert_eq!(track.clips[0], clip(1, 0.0, 0.0, 10.0));
}

#[test]
fn formatting_roundtrip_preserves_all_fields() {
    let mut c = clip(1, 0.0, 0.0, 10.0);
    c.gain_db = 3.0;
    c.frozen = true;
    c.speed_factor = 1.5;
    c.crop_x = 0.1;
    c.crop_y = 0.2;
    c.crop_w = 0.7;
    c.crop_h = 0.8;
    c.mask_shape = MaskShape::Circle;
    c.mask_corner_radius = 0.25;
    c.flipped_h = true;
    c.color_filter = ColorFilter::Sepia;
    c.vignette_intensity = 0.6;
    c.brightness = 0.3;
    c.contrast = 1.2;
    c.saturation = 0.8;
    c.sharpen = 0.4;
    c.chroma_key_enabled = true;
    c.chroma_key_color = [10, 200, 30];
    c.chroma_key_tolerance = 0.35;
    c.blur_intensity = 0.2;
    c.shake_intensity = 0.1;
    c.glitch_intensity = 0.05;
    c.pixelize_intensity = 0.15;
    c.transition_in = TransitionType::Fade;
    c.transition_duration_secs = 1.0;
    c.position_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: Position { x: 0.1, y: -0.2 },
    }];
    c.scale_keyframes = vec![
        Keyframe {
            time_fraction: 0.0,
            value: 1.2,
        },
        Keyframe {
            time_fraction: 1.0,
            value: 1.8,
        },
    ];
    c.rotation_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 15.0,
    }];
    c.opacity_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.6,
    }];
    c.gain_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: -12.0,
    }];
    c.brightness_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: -0.2,
    }];
    c.contrast_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 1.3,
    }];
    c.saturation_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.7,
    }];
    c.crop_x_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.15,
    }];
    c.crop_y_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.25,
    }];
    c.crop_w_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.6,
    }];
    c.crop_h_keyframes = vec![Keyframe {
        time_fraction: 0.5,
        value: 0.7,
    }];
    c.deflicker_enabled = true;

    let fmt = c.formatting();

    let mut dest = clip(2, 5.0, 0.0, 20.0);
    dest.apply_formatting(&fmt);

    assert_eq!(dest.formatting(), fmt);
    // Structural fields must not be touched by apply_formatting.
    assert_eq!(dest.id, 2);
    assert_eq!(dest.start_secs, 5.0);
    assert_eq!(dest.source_in_secs, 0.0);
    assert_eq!(dest.source_out_secs, 20.0);
}

#[test]
fn apply_formatting_overwrites_all_effect_fields() {
    let neutral: ClipFormatting = clip(1, 0.0, 0.0, 10.0).formatting();
    let mut c = clip(2, 0.0, 0.0, 5.0);
    c.gain_db = 6.0;
    c.color_filter = ColorFilter::BlackAndWhite;
    c.vignette_intensity = 0.9;

    c.apply_formatting(&neutral);

    assert_eq!(c.gain_db, 0.0);
    assert_eq!(c.color_filter, ColorFilter::None);
    assert_eq!(c.vignette_intensity, 0.0);
}

#[test]
fn timeline_clip_mut_finds_a_clip_across_tracks() {
    let mut timeline = timeline_with(vec![
        Track {
            id: 1,
            name: "V1".to_string(),
            kind: TrackKind::Video,
            clips: vec![clip(1, 0.0, 0.0, 10.0)],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
        Track {
            id: 2,
            name: "A1".to_string(),
            kind: TrackKind::Audio,
            clips: vec![clip(2, 0.0, 0.0, 10.0)],

            text_clips: vec![],
            shape_clips: vec![],

            visible: true,
            audio_role: AudioRole::Unspecified,
            muted: false,
            solo: false,
            locked: false,
            mix: Default::default(),
            color_label: None,
        },
    ]);

    let found = timeline.clip_mut(2).unwrap();
    found.gain_db = 3.0;

    assert_eq!(timeline.tracks[1].clips[0].gain_db, 3.0);
    assert_eq!(timeline.tracks[0].clips[0].gain_db, 0.0);
}

#[test]
fn timeline_clip_mut_returns_none_for_an_unknown_id() {
    let mut timeline = timeline_with(vec![Track {
        id: 1,
        name: "V1".to_string(),
        kind: TrackKind::Video,
        clips: vec![clip(1, 0.0, 0.0, 10.0)],

        text_clips: vec![],
        shape_clips: vec![],

        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        mix: Default::default(),
        color_label: None,
    }]);

    assert!(timeline.clip_mut(99).is_none());
}

#[test]
fn add_marker_assigns_ids_starting_at_one_and_clamps_a_negative_position() {
    let mut timeline = timeline_with(vec![]);

    let first = timeline.add_marker(5.0, MarkerKind::Standard);
    let second = timeline.add_marker(-3.0, MarkerKind::ToDo);

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(timeline.markers[0].position_secs, 5.0);
    assert_eq!(timeline.markers[1].position_secs, 0.0);
    assert_eq!(timeline.markers[1].kind, MarkerKind::ToDo);
    assert!(!timeline.markers[1].completed);
}

#[test]
fn add_marker_reuses_the_max_plus_one_id_even_after_a_removal() {
    let mut timeline = timeline_with(vec![]);
    let first = timeline.add_marker(0.0, MarkerKind::Standard);
    let second = timeline.add_marker(1.0, MarkerKind::Standard);
    timeline.remove_marker(second);

    let third = timeline.add_marker(2.0, MarkerKind::Standard);

    assert_eq!(first, 1);
    assert_eq!(second, 2);
    assert_eq!(
        third, 2,
        "the freed id 2 is reused since it's max(remaining) + 1"
    );
}

#[test]
fn remove_marker_reports_whether_anything_was_removed() {
    let mut timeline = timeline_with(vec![]);
    let id = timeline.add_marker(0.0, MarkerKind::Standard);

    assert!(timeline.remove_marker(id));
    assert!(timeline.markers.is_empty());
    assert!(
        !timeline.remove_marker(id),
        "already removed, second call is a no-op"
    );
}

#[test]
fn marker_mut_edits_the_right_marker_and_none_for_an_unknown_id() {
    let mut timeline = timeline_with(vec![]);
    let id = timeline.add_marker(0.0, MarkerKind::ToDo);

    timeline.marker_mut(id).unwrap().label = "Fix the intro".to_string();
    timeline.marker_mut(id).unwrap().completed = true;

    assert_eq!(timeline.markers[0].label, "Fix the intro");
    assert!(timeline.markers[0].completed);
    assert!(timeline.marker_mut(404).is_none());
}

#[test]
fn markers_sorted_orders_by_position_regardless_of_insertion_order() {
    let mut timeline = timeline_with(vec![]);
    timeline.add_marker(10.0, MarkerKind::Standard);
    timeline.add_marker(2.0, MarkerKind::Standard);
    timeline.add_marker(6.0, MarkerKind::Standard);

    let positions: Vec<f64> = timeline
        .markers_sorted()
        .iter()
        .map(|m| m.position_secs)
        .collect();

    assert_eq!(positions, vec![2.0, 6.0, 10.0]);
}

// --- Named trim modes (ROADMAP.md P2 item 11): Ripple / Roll / Slip / Slide ---

#[test]
fn slip_shifts_source_in_and_out_together_without_moving_on_the_timeline() {
    let mut c = clip(1, 5.0, 2.0, 8.0);

    assert!(c.slip(1.0, None));

    assert_eq!(
        c.start_secs, 5.0,
        "slip never moves the clip on the timeline"
    );
    assert_eq!(c.source_in_secs, 3.0);
    assert_eq!(c.source_out_secs, 9.0);
    assert_eq!(c.duration_secs(), 6.0, "duration is unchanged by a slip");
}

#[test]
fn slip_refuses_to_push_source_in_below_zero() {
    let mut c = clip(1, 5.0, 2.0, 8.0);

    assert!(!c.slip(-3.0, None));
    assert_eq!(
        c.source_in_secs, 2.0,
        "a refused slip leaves the clip untouched"
    );
    assert_eq!(c.source_out_secs, 8.0);
}

#[test]
fn slip_refuses_to_push_source_out_past_the_assets_own_duration() {
    let mut c = clip(1, 5.0, 2.0, 8.0);

    assert!(!c.slip(5.0, Some(10.0)));
    assert_eq!(c.source_out_secs, 8.0);
}

#[test]
fn previous_and_next_clip_id_walk_the_track_by_position() {
    let track = track_with(vec![clip(1, 0.0, 0.0, 6.0), clip(2, 6.0, 0.0, 9.0)]);

    assert_eq!(track.previous_clip_id(2), Some(1));
    assert_eq!(track.next_clip_id(1), Some(2));
    assert_eq!(
        track.previous_clip_id(1),
        None,
        "the earliest clip has no previous neighbor"
    );
    assert_eq!(
        track.next_clip_id(2),
        None,
        "the latest clip has no next neighbor"
    );
    assert_eq!(
        track.previous_clip_id(404),
        None,
        "an unknown id has no neighbors"
    );
}

#[test]
fn ripple_trim_start_shifts_only_clips_after_the_trimmed_clips_own_start() {
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 10.0),
        clip(2, 10.0, 0.0, 5.0),
        clip(3, 15.0, 0.0, 5.0),
    ]);

    assert!(track.ripple_trim_start(2, 12.0, 0.1));

    assert_eq!(
        track.clips[0].start_secs, 0.0,
        "clips before the edit point don't move"
    );
    let clip2 = track.clips.iter().find(|c| c.id == 2).unwrap();
    assert_eq!(clip2.start_secs, 12.0);
    assert_eq!(clip2.source_in_secs, 2.0);
    let clip3 = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(
        clip3.start_secs, 17.0,
        "later clips shift by the same delta, no gap left"
    );
}

#[test]
fn ripple_trim_start_leaves_the_track_untouched_when_the_trim_itself_is_refused() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 5.0)]);

    // Shrinking clip 2 below the minimum duration refuses the underlying trim_start.
    assert!(!track.ripple_trim_start(2, 14.95, 0.1));
    assert_eq!(track.clips[1].start_secs, 10.0);
}

#[test]
fn ripple_trim_end_shifts_only_clips_after_the_trimmed_clip() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 5.0)]);

    assert!(track.ripple_trim_end(1, 8.0, 0.1, None));

    let clip1 = track.clips.iter().find(|c| c.id == 1).unwrap();
    assert_eq!(clip1.source_out_secs, 8.0);
    let clip2 = track.clips.iter().find(|c| c.id == 2).unwrap();
    assert_eq!(
        clip2.start_secs, 8.0,
        "shifted left by the 2s shrink, no gap left"
    );
}

#[test]
fn roll_edit_moves_the_shared_boundary_leaving_the_pairs_overall_span_unchanged() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 2.0, 7.0)]);

    assert!(track.roll_edit(1, 8.0, 0.1, None));

    let clip1 = track.clips.iter().find(|c| c.id == 1).unwrap();
    assert_eq!(clip1.start_secs + clip1.duration_secs(), 8.0);
    let clip2 = track.clips.iter().find(|c| c.id == 2).unwrap();
    assert_eq!(
        clip2.start_secs, 8.0,
        "the boundary landed exactly where clip 1's end did"
    );
    assert_eq!(
        clip2.start_secs + clip2.duration_secs(),
        15.0,
        "the pair's overall span (0..15) is unchanged, just reallocated between them"
    );
}

#[test]
fn roll_edit_is_atomic_a_refused_neighbor_trim_leaves_both_clips_untouched() {
    // clip 2's source_in_secs is already 0.0 -- rolling the boundary earlier would need it to
    // show footage before its own start, which doesn't exist, so its trim_start must refuse.
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 5.0)]);

    assert!(!track.roll_edit(1, 8.0, 0.1, None));

    assert_eq!(
        track.clips[0].source_out_secs, 10.0,
        "clip 1 must not be left half-rolled"
    );
    assert_eq!(
        track.clips[1].start_secs, 10.0,
        "clip 2 must not be left half-rolled either"
    );
}

#[test]
fn roll_edit_is_a_no_op_without_a_next_neighbor() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    assert!(!track.roll_edit(1, 8.0, 0.1, None));
}

#[test]
fn slide_clip_absorbs_the_move_into_both_neighbors_without_changing_its_own_content() {
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 6.0),
        clip(2, 6.0, 0.0, 9.0),
        clip(3, 15.0, 0.0, 5.0),
    ]);

    assert!(track.slide_clip(2, 8.0, 0.1, None));

    let clip2 = track.clips.iter().find(|c| c.id == 2).unwrap();
    assert_eq!(clip2.start_secs, 8.0);
    assert_eq!(
        clip2.duration_secs(),
        9.0,
        "slide never changes the slid clip's own content"
    );
    let clip1 = track.clips.iter().find(|c| c.id == 1).unwrap();
    assert_eq!(
        clip1.start_secs + clip1.duration_secs(),
        8.0,
        "the previous clip's end absorbs the move, meeting clip 2's new start"
    );
    let clip3 = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(
        clip3.start_secs, 17.0,
        "the next clip's start absorbs the move"
    );
    assert_eq!(
        clip3.start_secs + clip3.duration_secs(),
        20.0,
        "the next clip's own end stays put -- nothing past it shifts"
    );
}

#[test]
fn slide_clip_is_atomic_a_refused_neighbor_trim_leaves_everything_untouched() {
    // Sliding clip 2 to 8.0 would need clip 1 to extend to an 8s duration, which is fine, but
    // clip 3's start would need to move to 17.0, shrinking it to a 3s duration -- refuse by
    // asking for an impossibly high minimum duration so the whole edit rolls back.
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 6.0),
        clip(2, 6.0, 0.0, 9.0),
        clip(3, 15.0, 0.0, 5.0),
    ]);

    assert!(!track.slide_clip(2, 8.0, 4.0, None));

    assert_eq!(track.clips[0].source_out_secs, 6.0);
    assert_eq!(track.clips[1].start_secs, 6.0);
    assert_eq!(track.clips[2].start_secs, 15.0);
}

#[test]
fn slide_clip_with_no_neighbors_just_moves_it() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 6.0)]);
    assert!(track.slide_clip(1, 3.0, 0.1, None));
    assert_eq!(track.clips[0].start_secs, 3.0);
}

#[test]
fn ripple_delete_range_drops_a_clip_fully_inside_the_range_and_ripples_later_clips_left() {
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 5.0),
        clip(2, 5.0, 0.0, 3.0),
        clip(3, 8.0, 0.0, 5.0),
    ]);
    let mut next_id = 4;

    assert!(track.ripple_delete_range(5.0, 8.0, &mut next_id));

    assert_eq!(
        track.clips.len(),
        2,
        "clip 2 (fully inside the range) is dropped"
    );
    assert!(track.clips.iter().any(|c| c.id == 1));
    let clip3 = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(clip3.start_secs, 5.0, "shifted left by the removed 3s span");
    assert_eq!(next_id, 4, "no split was needed, no id consumed");
}

#[test]
fn ripple_delete_range_splits_a_clip_straddling_either_boundary() {
    // A single 20s clip; deleting [5, 15) should leave two remainders: [0,5) and [15,20)
    // ripple-shifted left to close the 10s gap, i.e. starting at 0 and 5.
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 20.0)]);
    let mut next_id = 2;

    assert!(track.ripple_delete_range(5.0, 15.0, &mut next_id));

    assert_eq!(
        next_id, 4,
        "both boundaries needed a split, two ids consumed"
    );
    assert_eq!(track.clips.len(), 2);
    let first = track.clips.iter().find(|c| c.id == 1).unwrap();
    assert_eq!(first.start_secs, 0.0);
    assert_eq!(first.source_out_secs, 5.0);
    let second = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(
        second.start_secs, 5.0,
        "ripple-shifted left by the removed 10s span"
    );
    assert_eq!(second.source_in_secs, 15.0);
}

#[test]
fn ripple_delete_range_no_op_split_at_an_exact_boundary_consumes_no_id() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 5.0), clip(2, 5.0, 0.0, 5.0)]);
    let mut next_id = 3;

    // [0, 5) exactly matches clip 1's own span -- no clip straddles either boundary.
    assert!(track.ripple_delete_range(0.0, 5.0, &mut next_id));

    assert_eq!(next_id, 3, "no split was performed, id counter untouched");
    assert_eq!(track.clips.len(), 1);
    assert_eq!(track.clips[0].id, 2);
    assert_eq!(track.clips[0].start_secs, 0.0);
}

#[test]
fn ripple_delete_range_rejects_an_empty_or_inverted_range() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    let mut next_id = 2;
    assert!(!track.ripple_delete_range(5.0, 5.0, &mut next_id));
    assert!(!track.ripple_delete_range(8.0, 3.0, &mut next_id));
    assert_eq!(next_id, 2);
    assert_eq!(track.clips.len(), 1);
}

// P2 item 10, "Multicam editing" -- MulticamGroup grouping/hiding and
// Timeline::switch_multicam_angle's split+retarget logic.

fn track_with_id_and_asset(id: u64, asset_id: u64, clips: Vec<ClipInstance>) -> Track {
    let mut track = track_with(clips);
    track.id = id;
    for clip in &mut track.clips {
        clip.asset_id = asset_id;
    }
    track
}

#[test]
fn add_multicam_group_hides_every_member_except_the_program_track() {
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 20.0)]);
    let angle_3 = track_with_id_and_asset(3, 30, vec![clip(3, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2, angle_3]);

    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2, 3], 1, HashMap::new())
        .unwrap();

    assert_eq!(timeline.multicam_groups.len(), 1);
    assert_eq!(timeline.multicam_groups[0].id, group_id);
    assert!(timeline.tracks[0].visible, "program track stays visible");
    assert!(!timeline.tracks[1].visible, "non-program angle is hidden");
    assert!(!timeline.tracks[2].visible, "non-program angle is hidden");
}

#[test]
fn add_multicam_group_rejects_fewer_than_two_members_or_an_unknown_program_track() {
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);

    assert!(timeline
        .clone()
        .add_multicam_group("Solo".to_string(), vec![1], 1, HashMap::new())
        .is_none());
    assert!(timeline
        .add_multicam_group("Bad program".to_string(), vec![1, 2], 99, HashMap::new())
        .is_none());
    assert!(timeline.multicam_groups.is_empty());
}

#[test]
fn remove_multicam_group_re_shows_every_member() {
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, HashMap::new())
        .unwrap();
    assert!(!timeline.tracks[1].visible);

    assert!(timeline.remove_multicam_group(group_id));

    assert!(timeline.tracks[1].visible, "hide is undone on removal");
    assert!(timeline.multicam_groups.is_empty());
}

#[test]
fn switch_multicam_angle_splits_the_program_clip_and_retargets_the_second_half() {
    // Two in-sync (offset 0) 20s angles, different assets. Program (angle 1, asset 10) plays
    // the whole 20s; switching to angle 2 (asset 20) at t=8 should leave [0, 8) on asset 10 and
    // retarget [8, 20) to asset 20 at the same source position (no offset between them).
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, HashMap::new())
        .unwrap();

    let switched = timeline.switch_multicam_angle(group_id, 1, 8.0, 100);

    assert!(switched);
    let program_clips = &timeline.tracks[0].clips;
    assert_eq!(program_clips.len(), 2);
    assert_eq!(program_clips[0].id, 1);
    assert_eq!(program_clips[0].asset_id, 10);
    assert_eq!(program_clips[0].start_secs, 0.0);
    assert_eq!(program_clips[0].source_out_secs, 8.0);
    assert_eq!(program_clips[1].id, 100);
    assert_eq!(
        program_clips[1].asset_id, 20,
        "retargeted to angle 2's asset"
    );
    assert_eq!(program_clips[1].start_secs, 8.0);
    assert_eq!(
        program_clips[1].source_in_secs, 8.0,
        "same source position, 0 offset"
    );
    assert_eq!(program_clips[1].source_out_secs, 20.0);
}

#[test]
fn switch_multicam_angle_accounts_for_a_nonzero_sync_offset() {
    // Angle 2 started recording 2s *after* angle 1: for the same real-world moment,
    // angle_2_time = angle_1_time - 2.0, i.e. offset(angle_2) = -2.0 relative to angle 1's 0.0.
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 18.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let mut offsets = HashMap::new();
    offsets.insert(2u64, -2.0);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, offsets)
        .unwrap();

    // Switching at t=8 on the program track's own clock -> angle 2's equivalent time is 8 - 2 = 6.
    let switched = timeline.switch_multicam_angle(group_id, 1, 8.0, 100);

    assert!(switched);
    let switched_clip = &timeline.tracks[0].clips[1];
    assert_eq!(switched_clip.source_in_secs, 6.0);
}

#[test]
fn switch_multicam_angle_reuses_an_existing_boundary_without_a_redundant_split() {
    let program = track_with_id_and_asset(
        1,
        10,
        vec![clip(1, 0.0, 0.0, 10.0), clip(2, 10.0, 0.0, 10.0)],
    );
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(3, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, HashMap::new())
        .unwrap();

    // 10.0 is already the boundary between clip 1 and clip 2 -- no split should occur.
    let switched = timeline.switch_multicam_angle(group_id, 1, 10.0, 100);

    assert!(switched);
    assert_eq!(
        timeline.tracks[0].clips.len(),
        2,
        "reused the existing boundary, did not add a third clip"
    );
    assert_eq!(timeline.tracks[0].clips[1].id, 2, "kept clip 2's own id");
    assert_eq!(timeline.tracks[0].clips[1].asset_id, 20);
}

#[test]
fn switch_multicam_angle_is_a_no_op_for_an_unknown_group_or_angle_index() {
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 20.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, HashMap::new())
        .unwrap();

    assert!(!timeline.switch_multicam_angle(999, 1, 8.0, 100));
    assert!(!timeline.switch_multicam_angle(group_id, 5, 8.0, 100));
    assert!(
        !timeline.switch_multicam_angle(group_id, 0, 8.0, 100),
        "angle 0 is already the program track"
    );
    assert_eq!(timeline.tracks[0].clips.len(), 1, "nothing was split");
}

#[test]
fn switch_multicam_angle_is_a_no_op_when_the_target_angle_has_no_footage_at_that_time() {
    let program = track_with_id_and_asset(1, 10, vec![clip(1, 0.0, 0.0, 20.0)]);
    // Angle 2 only covers [0, 5) -- nothing there at t=8.
    let angle_2 = track_with_id_and_asset(2, 20, vec![clip(2, 0.0, 0.0, 5.0)]);
    let mut timeline = timeline_with(vec![program, angle_2]);
    let group_id = timeline
        .add_multicam_group("Multicam 1".to_string(), vec![1, 2], 1, HashMap::new())
        .unwrap();

    assert!(!timeline.switch_multicam_angle(group_id, 1, 8.0, 100));
    assert_eq!(timeline.tracks[0].clips.len(), 1, "nothing was split");
}

// FONT-01A's persisted-identity swap: TextFontFamily now serializes as font_catalog's stable
// family_id slug (e.g. "bebas-neue"), with a hand-written Deserialize that also accepts the old
// bare-variant-name format (e.g. "BebasNeue") every .ocproj saved before this migration
// actually contains, and falls back to TextFontFamily::Unknown (preserving the exact string)
// for anything matching neither -- never silently normalizing an unrecognized value away.
use avcore::timeline::TextFontFamily;

#[test]
fn text_font_family_serializes_as_the_stable_family_id_slug() {
    assert_eq!(
        serde_json::to_string(&TextFontFamily::BebasNeue).unwrap(),
        "\"bebas-neue\""
    );
    assert_eq!(
        serde_json::to_string(&TextFontFamily::Lato).unwrap(),
        "\"lato\""
    );
}

#[test]
fn text_font_family_round_trips_through_the_new_slug_format() {
    for family in TextFontFamily::ALL {
        let json = serde_json::to_string(&family).unwrap();
        let back: TextFontFamily = serde_json::from_str(&json).unwrap();
        assert_eq!(back, family, "round-trip drifted for {json}");
    }
}

#[test]
fn text_font_family_deserializes_the_old_bare_variant_name_format() {
    // What every .ocproj saved before this migration actually persisted -- serde's default
    // unit-variant encoding, the plain PascalCase name.
    let old: TextFontFamily = serde_json::from_str("\"BebasNeue\"").unwrap();
    assert_eq!(old, TextFontFamily::BebasNeue);

    let old_lato: TextFontFamily = serde_json::from_str("\"Lato\"").unwrap();
    assert_eq!(old_lato, TextFontFamily::Lato);
}

#[test]
fn text_font_family_preserves_an_unrecognized_id_instead_of_normalizing_to_lato() {
    let unknown: TextFontFamily = serde_json::from_str("\"some-future-font\"").unwrap();
    assert_eq!(
        unknown,
        TextFontFamily::Unknown("some-future-font".to_string())
    );
    // Round-tripping it back out keeps the exact same string -- the whole point of the swap.
    assert_eq!(
        serde_json::to_string(&unknown).unwrap(),
        "\"some-future-font\""
    );
}

#[test]
fn text_font_family_unknown_renders_and_behaves_as_lato() {
    let unknown = TextFontFamily::Unknown("some-future-font".to_string());
    assert_eq!(unknown.family_id(), "some-future-font");
    assert!(
        !unknown.supports_bold(),
        "an unrecognized id has no catalog entry to check a bold face against"
    );
}

#[test]
fn every_fixed_shape_preset_round_trips_through_its_own_shape_kind() {
    use avcore::timeline::ShapePreset;
    for preset in ShapePreset::PICKABLE {
        let kind = preset.to_kind().expect("pickable presets have geometry");
        assert_eq!(ShapePreset::from_kind(&kind), preset);
    }
    assert_eq!(ShapePreset::Custom.to_kind(), None);
}

#[test]
fn a_polygon_matching_no_preset_is_custom_even_with_a_preset_vertex_count() {
    use avcore::timeline::ShapePreset;
    let hand_drawn = ShapeKind::Polygon(vec![(0.1, 0.1), (0.2, 0.3), (-0.4, 0.1)]);
    assert_eq!(ShapePreset::from_kind(&hand_drawn), ShapePreset::Custom);
    let rectangle_like =
        ShapeKind::Polygon(vec![(-0.4, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)]);
    assert_eq!(ShapePreset::from_kind(&rectangle_like), ShapePreset::Custom);
}

#[test]
fn new_overlay_clips_use_the_manual_insert_defaults() {
    let text = avcore::timeline::TextClip::new(7, 2.5, "Text".to_string());
    assert_eq!(
        (text.id, text.start_secs, text.duration_secs),
        (7, 2.5, 3.0)
    );
    assert_eq!((text.pos_x, text.pos_y, text.font_size), (0.1, 0.85, 48.0));
    assert_eq!(text.text, "Text");
    let shape = ShapeClip::new(8, 1.0);
    assert_eq!(
        (shape.id, shape.start_secs, shape.duration_secs),
        (8, 1.0, 3.0)
    );
    assert_eq!(shape.shape_kind, ShapeKind::rectangle());
    assert_eq!((shape.center_x, shape.center_y), (0.5, 0.5));
}

#[test]
fn switching_to_a_family_without_bold_drops_bold_and_static_families_drop_weight() {
    use avcore::timeline::{TextClip, TextFontFamily, TextFontStyle};
    let mut clip = TextClip::new(1, 0.0, String::new());
    clip.font_style = TextFontStyle::Bold;
    clip.font_weight = Some(600);
    assert!(clip.set_font_family(TextFontFamily::BebasNeue));
    assert_eq!(clip.font_family, TextFontFamily::BebasNeue);
    assert_eq!(clip.font_style, TextFontStyle::Regular);
    assert_eq!(clip.font_weight, None);

    let mut clip = TextClip::new(1, 0.0, String::new());
    clip.font_style = TextFontStyle::Bold;
    assert!(!clip.set_font_family(TextFontFamily::Lato));
    assert_eq!(clip.font_style, TextFontStyle::Bold);
}

#[test]
fn lift_range_drops_what_is_inside_and_leaves_later_clips_where_they_are() {
    let mut track = track_with(vec![
        clip(1, 0.0, 0.0, 5.0),
        clip(2, 5.0, 0.0, 3.0),
        clip(3, 8.0, 0.0, 5.0),
    ]);
    let mut next_id = 4;

    assert!(track.lift_range(5.0, 8.0, &mut next_id));

    assert_eq!(track.clips.len(), 2);
    let clip3 = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(clip3.start_secs, 8.0, "the gap stays open");
}

#[test]
fn lift_range_splits_a_straddling_clip_and_keeps_both_remainders_in_place() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 20.0)]);
    let mut next_id = 2;

    assert!(track.lift_range(5.0, 15.0, &mut next_id));

    assert_eq!(next_id, 4);
    assert_eq!(track.clips.len(), 2);
    let tail = track.clips.iter().find(|c| c.id == 3).unwrap();
    assert_eq!(tail.start_secs, 15.0);
    assert_eq!(tail.source_in_secs, 15.0);
}

#[test]
fn lift_range_rejects_an_empty_or_inverted_range() {
    let mut track = track_with(vec![clip(1, 0.0, 0.0, 10.0)]);
    let mut next_id = 2;
    assert!(!track.lift_range(5.0, 5.0, &mut next_id));
    assert!(!track.lift_range(8.0, 3.0, &mut next_id));
    assert_eq!(track.clips.len(), 1);
}

/// Small deterministic generator for the randomized range-edit checks below.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0 >> 33
    }

    /// A multiple of a quarter second in `[min, max]`, so every boundary is exactly representable.
    fn quarter_secs(&mut self, min: u64, max: u64) -> f64 {
        (min + self.next() % (max - min + 1)) as f64 / 4.0
    }
}

fn contiguous_track(rng: &mut Lcg) -> Track {
    let mut clips = Vec::new();
    let mut start = 0.0;
    for id in 1..=(2 + rng.next() % 5) {
        let length = rng.quarter_secs(2, 40);
        clips.push(clip(id, start, 0.0, length));
        start += length;
    }
    track_with(clips)
}

fn total_secs(track: &Track) -> f64 {
    track.clips.iter().map(|c| c.duration_secs()).sum()
}

/// How much of `[a, b)` the clips of `track` cover.
fn covered(track: &Track, a: f64, b: f64) -> f64 {
    track
        .clips
        .iter()
        .map(|c| {
            let end = c.start_secs + c.duration_secs();
            (end.min(b) - c.start_secs.max(a)).max(0.0)
        })
        .sum()
}

#[test]
fn lifting_any_range_removes_exactly_what_it_covers_and_moves_nothing_else() {
    let mut rng = Lcg(7);
    for round in 0..400 {
        let mut track = contiguous_track(&mut rng);
        let a = rng.quarter_secs(0, 120);
        let b = a + rng.quarter_secs(1, 60);
        let before_total = total_secs(&track);
        let inside = covered(&track, a, b);
        let before: Vec<(f64, f64)> = track
            .clips
            .iter()
            .map(|c| (c.start_secs, c.start_secs + c.duration_secs()))
            .collect();
        let mut next_id = 100;

        let clips_before = track.clips.clone();
        let changed = track.lift_range(a, b, &mut next_id);
        // The result says whether anything was removed or moved, no more and no less.
        assert_eq!(changed, track.clips != clips_before);

        let after_total = total_secs(&track);
        assert!(
            (before_total - inside - after_total).abs() < 1e-9,
            "round {round}: {before_total} - {inside} != {after_total} for [{a}, {b})"
        );
        assert_eq!(
            covered(&track, a, b),
            0.0,
            "round {round}: something is left in the range"
        );
        for c in &track.clips {
            let (start, end) = (c.start_secs, c.start_secs + c.duration_secs());
            assert!(
                before
                    .iter()
                    .any(|(s, e)| start >= s - 1e-9 && end <= e + 1e-9),
                "round {round}: a clip [{start}, {end}) is not inside any original clip"
            );
        }
    }
}

#[test]
fn rippling_any_range_out_keeps_a_contiguous_track_contiguous() {
    let mut rng = Lcg(11);
    for round in 0..400 {
        let mut track = contiguous_track(&mut rng);
        let a = rng.quarter_secs(0, 120);
        let b = a + rng.quarter_secs(1, 60);
        let before_total = total_secs(&track);
        let inside = covered(&track, a, b);
        let mut next_id = 100;

        let clips_before = track.clips.clone();
        let changed = track.ripple_delete_range(a, b, &mut next_id);
        // The result says whether anything was removed or moved, no more and no less.
        assert_eq!(changed, track.clips != clips_before);

        assert!(
            (before_total - inside - total_secs(&track)).abs() < 1e-9,
            "round {round}: duration changed by more than the covered range"
        );
        let mut clips: Vec<_> = track.clips.iter().collect();
        clips.sort_by(|x, y| x.start_secs.partial_cmp(&y.start_secs).unwrap());
        let mut cursor = 0.0;
        for c in clips {
            assert!(
                (c.start_secs - cursor).abs() < 1e-9,
                "round {round}: gap or overlap at {cursor} (clip starts at {})",
                c.start_secs
            );
            cursor = c.start_secs + c.duration_secs();
        }
    }
}

#[test]
fn range_edits_hold_for_clips_at_other_speeds() {
    let mut rng = Lcg(23);
    for round in 0..300 {
        let mut track = contiguous_track(&mut rng);
        let mut start = 0.0;
        for c in &mut track.clips {
            c.speed_factor = [0.5, 1.0, 2.0, 4.0][(rng.next() % 4) as usize];
            c.start_secs = start;
            start += c.duration_secs();
        }
        let a = rng.quarter_secs(0, 100);
        let b = a + rng.quarter_secs(1, 50);
        let before_total = total_secs(&track);
        let inside = covered(&track, a, b);
        let mut next_id = 100;

        let clips_before = track.clips.clone();
        let changed = track.lift_range(a, b, &mut next_id);
        // The result says whether anything was removed or moved, no more and no less.
        assert_eq!(changed, track.clips != clips_before);

        assert!(
            (before_total - inside - total_secs(&track)).abs() < 1e-6,
            "round {round}: speed-adjusted duration drifted for [{a}, {b})"
        );
        assert!(
            covered(&track, a, b) < 1e-9,
            "round {round}: range not empty"
        );
    }
}
