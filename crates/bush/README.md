# bush

Standalone, editor-first preview engine for Mose. It has no caller yet; status, measured numbers
and gaps are in [`spec/matrix/bush.md`](../../spec/matrix/bush.md), the design in
[`spec/architecture/preview-engine-replacement.md`](../../spec/architecture/preview-engine-replacement.md)
and the proposed integration in [`spec/architecture/bush-adapter-rfc.md`](../../spec/architecture/bush-adapter-rfc.md).

Depends only on `avbridge` (FFmpeg). Never depends on `core`, the UI, Tauri or GStreamer.

## Shape

```text
SceneSnapshot (immutable, validated)  --submit-->  Engine (one worker thread)
  contributors: media | raster overlay                |- decode sessions, frame cache (byte bounded)
  Placement: timing, speed (+ramp), transform,        |- parallel decode across sources
    blend, effects, keyframes, fade, LUT, ...         |- CPU compositor (threaded) or GPU (feature gpu)
                                                      `-> EngineEvent::Frame / Failed (generation tagged)
Player: clock-driven playback over an Engine (tick / tick_synced with the audio clock)
AudioRenderer + AudioPlayer + AudioSink (FakeSink, DeviceSink with feature cpal)
```

A newer `submit` replaces any unstarted request and cancels the in-flight one at the next decode
boundary, so a burst of scrubs costs one frame.

## Minimal use

```rust
let engine = bush::Engine::new(bush::EngineConfig {
    canvas_width: 1920,
    canvas_height: 1080,
    max_open_sources: 4,
    frame_cache_bytes: 256 * 1024 * 1024,
    decode_policy: bush::DecodePolicy::Software,
    renderer: bush::RendererChoice::Cpu, // or GpuWithCpuFallback with --features gpu
})?;
let generation = engine.submit(bush::EngineRequest { scene, playhead_secs: 12.5, overlays: vec![] })?;
if let Some(bush::EngineEvent::Frame { frame, .. }) = engine.wait(std::time::Duration::from_secs(1)) {
    // frame.pixels() is RGBA8, frame.timestamp_secs() the playhead it was rendered for
}
```

## Features

| Feature | Adds |
| --- | --- |
| `gpu` | wgpu compute compositor, blur and sharpen (`GpuRenderer`, `RendererChoice::GpuWithCpuFallback`) |
| `cpal` | `DeviceSink`, real audio output |

## Commands

```bash
cargo test -p bush                                   # everything that needs no device
cargo test -p bush --features "gpu cpal"             # GPU parity and real-device audio
cargo test -p bush --features cpal --test device_audio_test long_run -- --ignored --nocapture
cargo bench -p bush                                  # BUSH_BENCH_MEDIA_DIR enables 1080p/4K media
cargo run --release -p bush --example latency_probe  # p50/p95/p99 per request path
```

Tests use real FFmpeg decode of the fixtures under `crates/avbridge/tests/fixtures`. Blend modes
are checked against FFmpeg itself (golden CRCs, plus a live comparison when `ffmpeg` is on PATH).