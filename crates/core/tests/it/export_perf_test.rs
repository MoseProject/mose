//! How long an export takes. Ignored by default: it needs a real clip and prints timings instead of
//! asserting them. Run with
//! `OCA_BENCH_VIDEO=/path/clip.mp4 cargo test --release -p core --test it export_perf -- --ignored --nocapture --test-threads=1`
//! (the clip should have audio; `OCA_BENCH_ENCODER` picks cpu, auto, nvenc, ...;
//! `OCA_BENCH_ONLY` keeps the cases whose name contains it).

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use avcore::render::{render_timeline_export, RenderOutcome};
use avcore::timeline::{AudioRole, MaskShape, Track, TrackKind, TransitionType};
use avcore::Keyframe;
use avcore::{probe_media, GpuEncoderPreference, MediaAsset};

use crate::timeline_export_test::{clip, sequence_with};

fn source() -> Option<(PathBuf, MediaAsset)> {
    let path = PathBuf::from(std::env::var_os("OCA_BENCH_VIDEO")?);
    let probed = probe_media(&path).unwrap();
    let asset = probed.into_media_asset(1, "bench".to_string(), path.clone());
    Some((path, asset))
}

fn encoder() -> GpuEncoderPreference {
    match std::env::var("OCA_BENCH_ENCODER").as_deref() {
        Ok("auto") => GpuEncoderPreference::Auto,
        Ok("nvenc") => GpuEncoderPreference::Nvenc,
        _ => GpuEncoderPreference::Cpu,
    }
}

fn track(clips: Vec<avcore::timeline::ClipInstance>) -> Track {
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
        color_label: None,
    }
}

fn time_export(label: &str, asset: &MediaAsset, clips: Vec<avcore::timeline::ClipInstance>) {
    let secs: f64 = clips
        .iter()
        .map(|c| c.source_out_secs - c.source_in_secs)
        .sum();
    time_tracks(label, asset, vec![track(clips)], secs);
}

fn time_tracks(label: &str, asset: &MediaAsset, tracks: Vec<Track>, secs: f64) {
    // OCA_BENCH_ONLY=word runs only the cases whose name contains it.
    if let Ok(only) = std::env::var("OCA_BENCH_ONLY") {
        if !label.contains(&only) {
            return;
        }
    }
    let output = std::env::temp_dir().join("avcore_export_perf.mp4");
    let started = Instant::now();
    let outcome = render_timeline_export(
        &sequence_with(tracks),
        std::slice::from_ref(asset),
        &output,
        -14.0,
        encoder(),
        &AtomicBool::new(false),
        |_| {},
    )
    .unwrap();
    let took = started.elapsed().as_secs_f64();
    assert_eq!(outcome, RenderOutcome::Completed);
    let fps = asset.fps.unwrap_or(30.0) as f64;
    eprintln!(
        "{label}: {secs:.1}s of video in {took:.2}s = {:.2}x realtime, {:.1} fps",
        secs / took,
        secs * fps / took
    );
    let _ = std::fs::remove_file(output);
}

fn caption(length: f64) -> Track {
    use avcore::timeline::TextClip;
    Track {
        id: 3,
        name: "Legendas".to_string(),
        kind: TrackKind::Text,
        clips: vec![],
        text_clips: vec![TextClip {
            id: 1,
            start_secs: 0.0,
            duration_secs: length,
            text: "Hello World".to_string(),
            font_size: 48.0,
            font_family: Default::default(),
            font_style: Default::default(),
            font_weight: None,
            color_rgba: [255, 255, 255, 255],
            background_rgba: [0, 0, 0, 0],
            background_padding: 8.0,
            background_corner_radius: 8.0,
            pos_x: 0.1,
            pos_y: 0.85,
            words: vec![],
            highlight_enabled: false,
            highlight_color_rgba: [255, 220, 0, 255],
            opacity_keyframes: vec![],
            pos_x_keyframes: vec![],
            pos_y_keyframes: vec![],
            scale_keyframes: vec![],
            rotation_keyframes: vec![],
            direction: Default::default(),
            language: None,
            text_align: Default::default(),
        }],
        shape_clips: vec![],
        visible: true,
        audio_role: AudioRole::Unspecified,
        muted: false,
        solo: false,
        locked: false,
        color_label: None,
    }
}

#[test]
#[ignore]
fn export_perf() {
    let Some((_, asset)) = source() else {
        eprintln!("set OCA_BENCH_VIDEO to a clip to run this");
        return;
    };
    let length = asset.duration_secs.min(20.0);
    time_export("plain clip", &asset, vec![clip(1, 1, 0.0, 0.0, length)]);
    let half = length / 2.0;
    time_export(
        "two cuts",
        &asset,
        vec![clip(1, 1, 0.0, 0.0, half), clip(2, 1, half, half, length)],
    );
    let one = |tweak: &dyn Fn(&mut avcore::timeline::ClipInstance)| {
        let mut c = clip(1, 1, 0.0, 0.0, length);
        tweak(&mut c);
        vec![c]
    };
    time_export("brightness", &asset, one(&|c| c.brightness = 0.1));
    time_export("vignette", &asset, one(&|c| c.vignette_intensity = 0.4));
    time_export("flip", &asset, one(&|c| c.flipped_h = true));
    time_export("blur", &asset, one(&|c| c.blur_intensity = 0.5));
    time_export(
        "circle mask",
        &asset,
        one(&|c| c.mask_shape = MaskShape::Circle),
    );
    time_export(
        "rounded mask",
        &asset,
        one(&|c| {
            c.mask_shape = MaskShape::RoundedRect;
            c.mask_corner_radius = 0.2;
        }),
    );
    time_export(
        "fade transition",
        &asset,
        one(&|c| c.transition_in = TransitionType::Fade),
    );
    time_export(
        "opacity keyframes",
        &asset,
        one(&|c| {
            c.opacity_keyframes = vec![
                Keyframe {
                    time_fraction: 0.0,
                    value: 1.0,
                },
                Keyframe {
                    time_fraction: 1.0,
                    value: 0.5,
                },
            ]
        }),
    );
    for (name, kind) in [
        ("slide transition", TransitionType::Slide),
        ("zoom transition", TransitionType::Zoom),
        ("dissolve transition", TransitionType::Dissolve),
    ] {
        time_export(name, &asset, one(&|c| c.transition_in = kind));
    }
    time_export(
        "scale keyframes",
        &asset,
        one(&|c| {
            c.scale_keyframes = vec![
                Keyframe {
                    time_fraction: 0.0,
                    value: 1.0,
                },
                Keyframe {
                    time_fraction: 1.0,
                    value: 1.5,
                },
            ]
        }),
    );
    time_export(
        "color keyframes",
        &asset,
        one(&|c| {
            c.brightness_keyframes = vec![
                Keyframe {
                    time_fraction: 0.0,
                    value: 0.0,
                },
                Keyframe {
                    time_fraction: 1.0,
                    value: 0.3,
                },
            ]
        }),
    );
    time_export("speed 2x", &asset, one(&|c| c.speed_factor = 2.0));
    let mut layer = clip(2, 1, 0.0, 0.0, length);
    layer.layer_scale_x = 0.4;
    layer.layer_scale_y = 0.4;
    let mut second = track(vec![layer]);
    second.id = 2;
    time_tracks(
        "picture in picture",
        &asset,
        vec![track(vec![clip(1, 1, 0.0, 0.0, length)]), second],
        length,
    );
    time_tracks(
        "text caption",
        &asset,
        vec![track(vec![clip(1, 1, 0.0, 0.0, length)]), caption(length)],
        length,
    );
}
