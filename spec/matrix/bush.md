# Bush (native preview engine) - status matrix

Bush is the standalone Rust preview engine described in
[../architecture/preview-engine-replacement.md](../architecture/preview-engine-replacement.md).
It has **no caller yet**: `core`, `desktop` and the GStreamer path are untouched. The adapter that
would connect it is a separate approval point (plan phase 7) and is not part of this matrix.

Legend: `[x]` done and verified against real media, `[~]` partial, `[ ]` not started.
Everything below runs in `cargo test -p bush` (and `cargo test -p avbridge` for the native side)
unless a note says otherwise; tests use real FFmpeg decode, real fixtures and, for audio output,
the real default device.

## Engine surface

- [x] `Engine` (`engine.rs`): `submit` replaces any unstarted request and cancels the in-flight
      one at the next decode boundary; one thread owns decoders, caches and the compositor;
      generation-tagged events, stale generations never published, bounded everywhere.
      Tests: `engine_test` (single clip, speed/start mapping, overlay order, burst of 40 submits
      publishes only the newest, cache hit on a live edit, redacted failure messages, recovery).
- [x] `Player` (`player.rs`): clock-driven playback with injected `now`; rate changes, end of
      timeline, seek, live scene edit. Tests: `player_test` (no sleeps in the logic).
- [x] Scene model (`scene.rs`): validated immutable `SceneSnapshot`, `Placement` (timeline start,
      speed, position, opacity, transform, blend, effects, gain, animation, fade, LUT, deflicker).
- [x] Frame-accurate decode (`DecodeSource::frame_at`): key-unit seek, decode forward, forward
      cursor so sequential playback never re-seeks, shared `Arc` frames. Tests: `decode_source_test`.
- [x] Parallel decode across *different* sources in one frame (multicam, layered clips); misses
      on one source stay sequential. Tests: `engine_test` multicam cases.
- [x] Proxy/original/optimized variant selection (`source_variants.rs`, from the earlier phase).
- [x] Latency percentiles (`Engine::latency`, last 1024 frames) and counters (`Engine::stats`).
- [x] Reliability: truncated and randomly corrupted media (video and audio) never panic or hang
      and the engine keeps serving good media; 30 create/drop lifecycles; 150-scrub soak with a
      three-frame cache. Tests: `reliability_test`. Canvas limited to `MAX_CANVAS_PIXELS`.

## Compositing (CPU reference, `cpu_renderer.rs`, `layer_mods.rs`)

- [x] Normal/multiply/screen/overlay/darken/lighten/add/difference blends (W3C formulas, exact).
- [x] Scale, rotation, crop, placement by top-left corner; nearest sampling.
- [x] Brightness, contrast, saturation, vignette, pixelize, chroma key (soft ramp), rectangle /
      rounded-rectangle / ellipse masks (feather, invert), trilinear 3D LUT.
- [x] Keyframe tracks (linear/hold/ease-in/ease-out/ease-in-out) for opacity, x, y, scale,
      rotation, plus linear fade in/out. Tests: `keyframes_test`.
- [x] Temporal deflicker with constant per-stream state; repeats do not drift; seeks reset it.
- [x] Parallel banded compositing plus exact fast paths, verified against an independent
      per-pixel oracle (`parallel_fast_path_matches_an_independent_oracle`).
- [x] GPU compositor (`gpu_renderer.rs` + `gpu_renderer.wgsl`, `--features gpu`, wgpu compute
      shader, one dispatch per layer, same geometry code as the CPU path). Selected per engine
      with `EngineConfig::renderer = RendererChoice::GpuWithCpuFallback`; without the feature or
      an adapter the engine silently uses the CPU compositor (`Engine::renderer_name` says which).
      Parity against the CPU reference on an RTX 4070 (`gpu_parity_test`): byte-exact for alpha
      layers, rotation, crop, upscale, padded strides, off-canvas layers and empty scenes; at most
      one level off on a fraction of a percent of pixels for blends, colour effects, chroma key,
      masks, pixelize and LUT; a thin fringe of nearest-neighbour ties when downscaling.
      Limits: frames are uploaded and the canvas read back every render (the engine API returns
      CPU frames), so the win is largest with effects and many layers (table below).
- [x] Gaussian blur (three box passes, cost independent of sigma, premultiplied alpha) and
      unsharp-mask sharpen (`blur.rs`, `Placement::blur_sigma` / `sharpen`), applied to the layer
      frame before compositing so CPU and GPU renderers share them. Measured: blur 18 ms at
      1080p and 77 ms at 4K, sharpen 34 ms / 148 ms on the CPU (a GPU blur is future work).
      Tests: `blur_test`, `engine_test` (blur and sharpen through the engine, cache reuse).
- [x] Glitch (`Placement::glitch`: channel split plus displaced bands, 12 steps per second) and
      shake (`Placement::shake`: smooth value-noise jitter in canvas pixels). Both are pure
      functions of clip time, so paused frames never flicker. Tests: `glitch_test`.
- [ ] Stabilization (needs motion history), slide/zoom transitions
      (expressible today with keyframes, no dedicated effect), background-removal matte inputs.
      None of these exist in the scene model, so none can be silently ignored.
- [ ] Text/shape rasterization is the caller job by design: Bush composites caller-supplied
      RGBA overlays (`EngineRequest::overlays`); no text layout lives here.

## Audio

- [x] Seekable audio decode in C + FFI (`avbridge::AudioDecodeSession`): float, any rate,
      mono/stereo, resampler rebuilt per seek. Tests: `audio_decode_session_test`.
- [x] `AudioSource`: sample-accurate frame reads with preroll after seeks (AAC needs it),
      zero padding outside the stream, cancellation. `AudioRenderer`: timeline placement, clip
      gain, mixer with hard limiter. `AudioPlayer` + `AudioSink` + deterministic `FakeSink`
      (underflow, overrun, device loss). Tests: `audio_playback_test`.
- [x] Real output (`DeviceSink`, `--features cpal`): validated on the default device of the
      development machine (WASAPI, 48 kHz): consumption within +1.9 % of wall clock over 0.6 s,
      underflow counting, bounded queue. Tests: `device_audio_test` (skips itself without a device).
- [x] Pitch-preserving time stretch (`time_stretch.rs`, WSOLA, speeds 0.25x-4x): a clip with
      `Placement::speed != 1` keeps its pitch, ends early or runs long, and consecutive blocks
      join bit-exactly; seeks restart the stretch at the matching source position. Outside the
      range the renderer returns `AudioRenderError::UnsupportedSpeed`. Cost about 6 ms per
      second of stereo output. Tests: `time_stretch_test` (pitch within 4 Hz at five speeds,
      unit speed transparent, chunking-invariant), `audio_playback_test`.
- [ ] Long-run A/V drift measurement and device-loss recovery on real hardware (the fake sink
      covers the contract; only the short clock test ran on a device).

## Hardware decode

- [x] Opt-in (`DecodePolicy::HardwareWithFallback`): first hardware device the codec supports,
      shared per process (creating one costs about 250 ms), software fallback on any failure;
      output matches software within 3 mean levels. Verified on an RTX 4070 through D3D11VA.
- [x] **Measured: not faster here.** Frames are copied back to system memory and converted, so
      hardware is slower than threaded software decode for every fixture tried (see numbers).
      It stays opt-in; a win needs a zero-copy GPU path.
- [ ] Hardware paths other than D3D11VA (VAAPI, VideoToolbox) are unverified: no such machine.

## Measured numbers

Machine: AMD Ryzen 9 5950X (32 threads), RTX 4070, Windows 11, FFmpeg 8.1 LGPL shared build,
release profile, Criterion unless noted. Media: 640x360 H.264 fixture (checked in), 1080p H.264
8 Mbps and 4K HEVC 20 Mbps made locally with `h264_nvenc`/`hevc_nvenc` (point
`BUSH_BENCH_MEDIA_DIR` at a directory with `h264_1080p.mp4` / `hevc_2160p.mp4` to reproduce).

| Workload | Before | After |
| --- | --- | --- |
| 1080p, two layers, CPU composite | 77 ms | 7.4 ms |
| 1080p, two layers scaled + rotated + multiply | 74 ms | 6.9 ms |
| 1080p H.264 decode + RGBA, 24 frames, software | 281 ms | 163 ms (6.8 ms/frame) |
| 4K HEVC decode + RGBA, 24 frames, software | 1.10 s | 0.63 s (26 ms/frame) |
| 1080p H.264, 24 frames, hardware (shared device) | 526 ms | 316 ms |
| 4K HEVC, 24 frames, hardware | 1.26 s | 1.07 s |
| Engine, 1080p sequential, 24 frames | n/a | 244 ms (10 ms/frame) |
| Engine, 4K sequential, 24 frames | n/a | 883 ms (37 ms/frame) |
| Engine, 3 x 1080p angles, 24 frames | n/a | 600 ms (25 ms/frame) |

GPU versus CPU compositing, same inputs (scaled, rotated, multiply-blended layers; "effects" adds
contrast, saturation, vignette, chroma key, ellipse mask and a 33-point LUT; GPU time includes
upload of every layer and readback of the canvas):

| Scenario | CPU | GPU (RTX 4070) |
| --- | --- | --- |
| 1080p, 2 layers | 12.4 ms | 7.2 ms |
| 1080p, 4 layers with effects | 85 ms | 10.9 ms |
| 4K, 2 layers | 56 ms | 24 ms |
| 4K, 6 layers with effects | 594 ms | 65 ms |

Per-request engine latency (`cargo run --release -p bush --example latency_probe`, 200 requests):

| Path | p50 | p95 | p99 |
| --- | --- | --- | --- |
| 640x360 random scrub | 0.80 ms | 1.2 ms | 4.5 ms |
| 640x360 live edit (cached source frame) | 0.76 ms | 0.94 ms | 1.2 ms |
| 1080p random scrub | 7.9 ms | 27.6 ms | 39.5 ms |
| 1080p live edit (cached source frame) | 4.9 ms | 5.7 ms | 6.3 ms |

Frame-accurate seeks decode from the preceding key unit but convert only the presented frame
to RGBA (the 1080p scrub went from p50 12.3 / p95 38 ms to 7.9 / 27.6 ms with that change).
Reading them: the GPU path removes the compositing bottleneck for effect-heavy and 4K scenes. 1080p playback and a 1080p live edit are inside a 33 ms budget. A 1080p scrub
tail (p95 27.6 ms) is the seek-to-key-unit plus decode-forward cost of a 30-frame GOP; 4K software
decode sits just under 30 fps and is the first thing a GPU/zero-copy path would fix.

## Commands

```bash
make test-bush            # cargo test -p bush
make bench-bush           # all Bush benches; set BUSH_BENCH_MEDIA_DIR for the large-media ones
cargo test -p bush --features cpal --test device_audio_test -- --nocapture   # real device
cargo test -p bush --features gpu --test gpu_parity_test -- --nocapture      # GPU vs CPU parity
cargo bench -p bush --features gpu --bench engine_workloads -- gpu_vs_cpu    # GPU vs CPU speed
```
