//! GPU loss mid-session: the engine must switch to the CPU compositor and keep producing frames.
use std::path::PathBuf;
use std::time::Duration;

use bush::{
    Contributor, ContributorContent, ContributorId, DecodePolicy, DecodeSource, Engine,
    EngineConfig, EngineEvent, EngineRequest, LayerFrame, MediaSource, Placement, RenderError,
    Renderer, RendererChoice, SceneSnapshot, SceneVersions, SourceRange, Version, VideoFrame,
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../avbridge/tests/fixtures/video.mp4")
}

fn config() -> EngineConfig {
    let info = DecodeSource::open(&fixture()).unwrap().info();
    EngineConfig {
        canvas_width: info.width,
        canvas_height: info.height,
        max_open_sources: 1,
        frame_cache_bytes: 64 * 1024 * 1024,
        decode_policy: DecodePolicy::Software,
        renderer: RendererChoice::Cpu,
    }
}

fn request(playhead: f64) -> EngineRequest {
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
        placement: Placement::default(),
    };
    EngineRequest {
        scene: SceneSnapshot::new(SceneVersions::default(), vec![clip]).unwrap(),
        playhead_secs: playhead,
        overlays: Vec::new(),
    }
}

fn next_frame(engine: &Engine, playhead: f64) -> Result<VideoFrame, String> {
    engine.submit(request(playhead)).unwrap();
    match engine.wait(Duration::from_secs(30)).unwrap() {
        EngineEvent::Frame { frame, .. } => Ok(frame),
        EngineEvent::Failed { message, .. } => Err(message),
    }
}

/// A compositor that behaves like a GPU whose device was lost.
struct LostGpu;

impl Renderer for LostGpu {
    fn render(
        &mut self,
        _width: u32,
        _height: u32,
        _timestamp_secs: f64,
        _layers: &[LayerFrame<'_>],
    ) -> Result<VideoFrame, RenderError> {
        Err(RenderError::Gpu("device lost".into()))
    }

    fn name(&self) -> String {
        "gpu:lost".into()
    }
}

#[test]
fn a_lost_gpu_falls_back_to_the_cpu_and_playback_continues() {
    let reference = {
        let engine = Engine::new(config()).unwrap();
        next_frame(&engine, 0.4).unwrap()
    };

    let engine = Engine::with_renderer(config(), Box::new(LostGpu)).unwrap();
    let first = next_frame(&engine, 0.4).expect("the frame that hit the loss is retried");
    assert_eq!(first.pixels(), reference.pixels());
    assert_eq!(engine.stats().gpu_fallbacks, 1);
    assert_eq!(engine.stats().failures, 0);

    // Later frames go straight to the CPU renderer; no second fallback.
    let second = next_frame(&engine, 0.5).unwrap();
    assert_eq!(
        (second.width(), second.height()),
        (reference.width(), reference.height())
    );
    assert_eq!(engine.stats().gpu_fallbacks, 1);
}

#[cfg(feature = "gpu")]
#[test]
fn a_destroyed_wgpu_device_reports_loss_instead_of_panicking() {
    let info = DecodeSource::open(&fixture()).unwrap().info();
    let Ok(gpu) = bush::GpuRenderer::new(info.width as usize * info.height as usize * 4) else {
        eprintln!("skipped: no GPU adapter");
        return;
    };
    assert!(!gpu.is_lost());
    gpu.destroy_device();
    assert!(gpu.is_lost());

    let engine = Engine::with_renderer(config(), Box::new(gpu)).unwrap();
    assert!(next_frame(&engine, 0.4).is_ok());
    assert_eq!(engine.stats().gpu_fallbacks, 1);
}
