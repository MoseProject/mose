//! `Placement::fit_canvas`: the decoded frame (which may be a proxy of any size) is stretched to
//! the canvas by the engine, centred, with `x`/`y` as an offset.
use std::path::PathBuf;
use std::time::Duration;

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, Engine, EngineConfig,
    EngineEvent, EngineRequest, LayerTransform, MediaSource, Placement, RendererChoice,
    SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn render(canvas: (u32, u32), placement: Placement) -> VideoFrame {
    let engine = Engine::new(EngineConfig {
        canvas_width: canvas.0,
        canvas_height: canvas.1,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    })
    .unwrap();
    let clip = Contributor {
        id: ContributorId(1),
        layer: 0,
        visible: true,
        audible: false,
        content: ContributorContent::Media(MediaSource {
            path: fixture(),
            range: SourceRange::new(0.0, 0.9).unwrap(),
            version: Version(1),
        }),
        placement,
    };
    engine
        .submit(EngineRequest {
            scene: SceneSnapshot::new(SceneVersions::default(), vec![clip]).unwrap(),
            playhead_secs: 0.4,
            overlays: Vec::new(),
        })
        .unwrap();
    match engine.wait(Duration::from_secs(30)).unwrap() {
        EngineEvent::Frame { frame, .. } => frame,
        EngineEvent::Failed { message, .. } => panic!("{message}"),
    }
}

fn alpha(frame: &VideoFrame, x: u32, y: u32) -> u8 {
    frame.pixels()[(y as usize * frame.width() as usize + x as usize) * 4 + 3]
}

#[test]
fn a_source_of_any_size_fills_a_different_canvas() {
    // The fixture is not 100x60; without the fit it would not cover this canvas.
    let plain = render((100, 60), Placement::default());
    let fitted = render(
        (100, 60),
        Placement {
            fit_canvas: true,
            ..Placement::default()
        },
    );
    for (x, y) in [(0, 0), (99, 0), (0, 59), (99, 59), (50, 30)] {
        assert_eq!(alpha(&fitted, x, y), 255, "pixel ({x}, {y}) not covered");
    }
    assert_ne!(plain.pixels(), fitted.pixels());
}

#[test]
fn scale_shrinks_around_the_centre_and_xy_offsets_it() {
    let half = Placement {
        fit_canvas: true,
        transform: LayerTransform {
            scale_x: 0.5,
            scale_y: 0.5,
            ..LayerTransform::default()
        },
        ..Placement::default()
    };
    let centred = render((100, 60), half.clone());
    // Drawn 50x30 at (25, 15): corners empty, centre filled.
    assert_eq!(alpha(&centred, 2, 2), 0);
    assert_eq!(alpha(&centred, 97, 57), 0);
    assert_eq!(alpha(&centred, 50, 30), 255);
    assert_eq!(alpha(&centred, 26, 16), 255);
    assert_eq!(alpha(&centred, 24, 14), 0);

    let moved = render(
        (100, 60),
        Placement {
            x: 20,
            y: 10,
            ..half
        },
    );
    assert_eq!(alpha(&moved, 26, 16), 0);
    assert_eq!(alpha(&moved, 46, 26), 255);
    assert_eq!(alpha(&moved, 94, 54), 255);
}
