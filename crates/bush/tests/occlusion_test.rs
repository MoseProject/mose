//! A layer an opaque layer above completely covers is neither decoded nor composited. The
//! covered layer here points at a file that does not exist, so any attempt to decode it fails the
//! frame: a frame that renders proves it was left alone, and one that fails proves a layer that
//! does not cover was still used.
use std::path::PathBuf;
use std::time::Duration;

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, LayerTransform, MediaSource, Placement, RendererChoice,
    SceneSnapshot, SceneVersions, SourceRange, Version,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn layer(id: u64, path: PathBuf, placement: Placement) -> Contributor {
    Contributor {
        id: ContributorId(id),
        layer: id as i32,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path,
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement,
    }
}

fn fitted() -> Placement {
    Placement {
        fit_canvas: true,
        ..Placement::default()
    }
}

fn missing() -> PathBuf {
    PathBuf::from("does-not-exist.mp4")
}

/// Renders the layers (bottom first) at 0.4 s; `Err` carries the failure message.
fn render(layers: Vec<Contributor>) -> Result<bush::VideoFrame, String> {
    let engine = Engine::new(EngineConfig {
        canvas_width: 160,
        canvas_height: 90,
        max_open_sources: 2,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    engine
        .submit(EngineRequest {
            scene: SceneSnapshot::new(SceneVersions::default(), layers).unwrap(),
            playhead_secs: 0.4,
            overlays: Vec::new(),
        })
        .unwrap();
    match engine.wait(Duration::from_secs(30)).unwrap() {
        EngineEvent::Frame { frame, .. } => Ok(frame),
        EngineEvent::Failed { message, .. } => Err(message),
    }
}

#[test]
fn an_opaque_layer_covering_the_canvas_hides_what_is_below_it() {
    let frame = render(vec![
        layer(1, missing(), fitted()),
        layer(2, fixture(), fitted()),
    ])
    .expect("the covered layer is never decoded");
    assert_eq!((frame.width(), frame.height()), (160, 90));
    // The same picture as the top layer alone.
    let alone = render(vec![layer(2, fixture(), fitted())]).unwrap();
    assert_eq!(frame.pixels(), alone.pixels());
}

#[test]
fn a_layer_larger_than_the_canvas_also_covers_it() {
    let bigger = Placement {
        transform: LayerTransform {
            scale_x: 1.5,
            scale_y: 1.5,
            ..LayerTransform::default()
        },
        ..fitted()
    };
    render(vec![
        layer(1, missing(), fitted()),
        layer(2, fixture(), bigger),
    ])
    .expect("covered");
}

#[test]
fn anything_that_leaves_a_gap_or_a_see_through_pixel_keeps_the_layer_below() {
    let cases: Vec<(&str, Placement)> = vec![
        (
            "half opacity",
            Placement {
                opacity: 0.5,
                ..fitted()
            },
        ),
        (
            "smaller than the canvas",
            Placement {
                transform: LayerTransform {
                    scale_x: 0.5,
                    scale_y: 0.5,
                    ..LayerTransform::default()
                },
                ..fitted()
            },
        ),
        ("moved off centre", Placement { x: 20, ..fitted() }),
        (
            "rotated",
            Placement {
                transform: LayerTransform {
                    rotation_deg: 5.0,
                    ..LayerTransform::default()
                },
                ..fitted()
            },
        ),
        ("not stretched to the canvas", Placement::default()),
        (
            "blended",
            Placement {
                blend: bush::BlendMode::Multiply,
                ..fitted()
            },
        ),
    ];
    for (name, top) in cases {
        let result = render(vec![
            layer(1, missing(), fitted()),
            layer(2, fixture(), top),
        ]);
        assert!(result.is_err(), "{name}: the layer below was skipped");
    }
}
