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

//! Filter-chain stages, serde defaults and edge handling of `ClipInstance` that no other test
//! reached.

use avcore::timeline::{BlendMode, ClipInstance, MaskShape};
use avcore::Keyframe;

fn clip() -> ClipInstance {
    ClipInstance::new(1, 1, 2.0, 1.0, 5.0)
}

fn key(time_fraction: f32, value: f32) -> Keyframe<f32> {
    Keyframe {
        time_fraction,
        value,
    }
}

#[test]
fn every_blend_mode_has_its_own_ffmpeg_name() {
    let modes = [
        (BlendMode::Normal, "normal"),
        (BlendMode::Addition, "addition"),
        (BlendMode::And, "and"),
        (BlendMode::Average, "average"),
        (BlendMode::Burn, "burn"),
        (BlendMode::Darken, "darken"),
        (BlendMode::Difference, "difference"),
        (BlendMode::GrainExtract, "grainextract"),
        (BlendMode::Divide, "divide"),
        (BlendMode::Dodge, "dodge"),
        (BlendMode::Exclusion, "exclusion"),
        (BlendMode::HardLight, "hardlight"),
        (BlendMode::Lighten, "lighten"),
        (BlendMode::Multiply, "multiply"),
        (BlendMode::Negation, "negation"),
        (BlendMode::Or, "or"),
        (BlendMode::Overlay, "overlay"),
        (BlendMode::Phoenix, "phoenix"),
        (BlendMode::PinLight, "pinlight"),
        (BlendMode::Reflect, "reflect"),
        (BlendMode::Screen, "screen"),
        (BlendMode::SoftLight, "softlight"),
        (BlendMode::Subtract, "subtract"),
        (BlendMode::VividLight, "vividlight"),
        (BlendMode::Xor, "xor"),
        (BlendMode::HardMix, "hardmix"),
        (BlendMode::LinearLight, "linearlight"),
        (BlendMode::Glow, "glow"),
        (BlendMode::GrainMerge, "grainmerge"),
        (BlendMode::Multiply128, "multiply128"),
        (BlendMode::Heat, "heat"),
        (BlendMode::Freeze, "freeze"),
        (BlendMode::Extremity, "extremity"),
        (BlendMode::SoftDifference, "softdifference"),
        (BlendMode::Geometric, "geometric"),
        (BlendMode::Harmonic, "harmonic"),
        (BlendMode::Bleach, "bleach"),
        (BlendMode::Stain, "stain"),
        (BlendMode::Interpolate, "interpolate"),
        (BlendMode::HardOverlay, "hardoverlay"),
    ];
    for (mode, name) in modes {
        assert_eq!(mode.ffmpeg_name(), name);
    }
}

#[test]
fn missing_fields_load_with_their_documented_defaults() {
    let mut value = serde_json::to_value(clip()).unwrap();
    let object = value.as_object_mut().unwrap();
    for field in [
        "speed_factor",
        "transition_duration_secs",
        "crop_w",
        "crop_h",
        "layer_scale_x",
        "layer_scale_y",
        "brightness_unused",
        "chroma_key_color",
        "chroma_key_tolerance",
        "anchor_x",
        "anchor_y",
        "privacy_blur_sigma",
        "voice_cleanup_noise_floor_db",
        "voice_cleanup_compressor_threshold_db",
        "voice_cleanup_compressor_ratio",
        "voice_cleanup_ceiling_linear",
    ] {
        object.remove(field);
    }
    let loaded: ClipInstance = serde_json::from_value(value).unwrap();
    assert_eq!(loaded.speed_factor, 1.0);
    assert_eq!(loaded.transition_duration_secs, 0.5);
    assert_eq!((loaded.crop_w, loaded.crop_h), (1.0, 1.0));
    assert_eq!((loaded.layer_scale_x, loaded.layer_scale_y), (1.0, 1.0));
    assert_eq!(loaded.chroma_key_color, [0, 255, 0]);
    assert_eq!(loaded.chroma_key_tolerance, 0.4);
    assert_eq!((loaded.anchor_x, loaded.anchor_y), (0.5, 0.5));
    assert_eq!(loaded.privacy_blur_sigma, 15.0);
    assert_eq!(loaded.voice_cleanup_noise_floor_db, -30.0);
    assert_eq!(loaded.voice_cleanup_compressor_threshold_db, -18.0);
    assert_eq!(loaded.voice_cleanup_compressor_ratio, 3.0);
    assert_eq!(loaded.voice_cleanup_ceiling_linear, 0.95);
}

#[test]
fn nested_sequence_and_anchor_flags_follow_their_fields() {
    let mut clip = clip();
    assert!(!clip.is_nested_sequence());
    assert!(!clip.has_anchor());
    clip.nested_sequence_id = Some(9);
    clip.anchor_x = 0.25;
    assert!(clip.is_nested_sequence());
    assert!(clip.has_anchor());
    clip.anchor_x = 0.5;
    clip.anchor_y = 0.75;
    assert!(clip.has_anchor());
}

#[test]
fn keyframed_crop_and_colour_become_filter_stages() {
    let mut clip = clip();
    assert_eq!(clip.keyframe_video_filter_chain(30, 1, 4.0), None);
    clip.crop_x_keyframes = vec![key(0.0, 0.0), key(1.0, 0.25)];
    clip.brightness_keyframes = vec![key(0.0, 0.0), key(1.0, 0.2)];
    let chain = clip.keyframe_video_filter_chain(30, 1, 4.0).unwrap();
    // A keyframed crop is a per-pixel `geq` window; keyframed brightness is a `geq` too.
    assert!(chain.contains("geq="), "{chain}");
    assert!(chain.contains("lum='clip((lum(X,Y)"), "{chain}");
}

#[test]
fn stylistic_effects_add_their_filter_stages() {
    let mut clip = clip();
    clip.mask_shape = MaskShape::Circle;
    clip.blur_intensity = 0.5;
    clip.sharpen = 0.5;
    clip.pixelize_intensity = 0.5;
    clip.shake_intensity = 0.5;
    clip.glitch_intensity = 0.5;
    let chain = clip.video_filter_chain();
    assert!(chain.contains("avgblur=sizeX=5:sizeY=5"), "{chain}");
    assert!(chain.contains("unsharp=5:5:1.50"), "{chain}");
    assert!(chain.contains("flags=neighbor"), "{chain}");
    assert!(chain.contains("sin(n*0.31)"), "{chain}");
    assert!(chain.contains("noise=c0s=30"), "{chain}");
    assert!(chain.contains("geq="), "{chain}");

    clip.mask_shape = MaskShape::RoundedRect;
    clip.mask_corner_radius = 0.3;
    assert!(clip.video_filter_chain().contains("pow(max(abs(X-W/2)"));
}

#[test]
fn trimming_and_slipping_refuse_edits_outside_the_media() {
    let mut clip = clip();
    // Right edge: too short, past the end of the media, then a valid trim.
    assert!(!clip.trim_end(2.1, 0.5, None));
    assert!(!clip.trim_end(20.0, 0.5, Some(6.0)));
    assert!(clip.trim_end(5.0, 0.5, Some(6.0)));
    assert_eq!(clip.source_out_secs, 4.0);
    // Slip: before the start of the media, past its end, then valid.
    assert!(!clip.slip(-2.0, None));
    assert!(!clip.slip(5.0, Some(6.0)));
    assert!(clip.slip(1.0, Some(6.0)));
    assert_eq!((clip.source_in_secs, clip.source_out_secs), (2.0, 5.0));
}
