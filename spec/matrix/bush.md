# Bush (native preview engine) - status matrix

Bush is the standalone Rust preview engine described in
[../architecture/preview-engine-replacement.md](../architecture/preview-engine-replacement.md).
It is the only preview engine: `apps/desktop` drives it through `bush_adapter/` and `core`'s frame sampler decodes through it. GStreamer was removed (see [../architecture/gstreamer-removal-plan.md](../architecture/gstreamer-removal-plan.md)).

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

- [x] All 40 core blend modes (`BlendMode::ALL`) computed with FFmpeg 8-bit integer arithmetic,
      so a preview of opaque pixels equals the export. Verified against tables measured from a
      real FFmpeg build (CRC goldens for 37 modes bit-exact, `interpolate` within 1 level on 0.16 %
      of input pairs, `hardoverlay` one pair; a live-ffmpeg test re-measures when `ffmpeg` is on
      PATH) and the GPU shader reproduces every table. `softlight` is a measured 64 KB table.
      Note: FFmpeg `overlay` keys on the blended layer and `hardlight` on the backdrop (the
      reverse of Photoshop), and Bush follows FFmpeg because the export does.
- [x] Scale, rotation, crop, placement by top-left corner; nearest sampling.
- [x] Horizontal/vertical flip, sepia, and keyframes on brightness, contrast, saturation and the
      crop window (CPU and GPU parity tested).
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
      unsharp-mask sharpen as compositor layer effects (`LayerEffects::blur_sigma` / `sharpen`,
      `blur.rs`). CPU: blur 18 ms at 1080p and 77 ms at 4K, sharpen on top 34 / 148 ms. GPU
      (`gpu_blur.rs`, `gpu_blur.wgsl`): the same integer arithmetic on compute shaders, bit-exact
      against the CPU in every tested case (sigma 0.8 to 64, translucent frames, tiny frames,
      several layers per frame); end to end including upload and readback blur 8 takes 7.9 ms
      versus 24 ms on the CPU at 1080p, blur + sharpen 9.2 versus 55 ms, and at 4K 30 versus
      215 ms. Tests: `blur_test`, `engine_test`, `gpu_parity_test`.
- [x] GPU loss: `GpuRenderer` flags device loss instead of panicking; the engine switches to the
      CPU compositor mid-session and retries the frame (`EngineStats::gpu_fallbacks`). Tests:
      `gpu_loss_test` (simulated loss and a destroyed wgpu device; the wgpu loss callback itself
      was not separately exercised).
- [x] GPU source upload reuse: `VideoFrame::id` identifies pixel content, and a layer slot that
      already holds that frame skips the upload (`GpuRenderer::uploads_skipped`). Helps static
      layers, paused playback and live edits of other layers; a playing video layer changes
      frame every tick and still uploads. Static-layer bench: 4K 2 layers 14 ms, 4K 6 layers
      with effects 16 ms (was 65 ms). Tests: `gpu_upload_reuse_test`.
- [x] Glitch (`Placement::glitch`: channel split plus displaced bands, 12 steps per second) and
      shake (`Placement::shake`: smooth value-noise jitter in canvas pixels). Both are pure
      functions of clip time, so paused frames never flicker. Tests: `glitch_test`.
- [x] Stabilization (`stabilizer.rs`, `Placement::stabilize`): global motion by SAD block
      matching with sub-pixel refinement on a 160 px analysis plane, EMA-smoothed camera path,
      counter-translation plus a 6 % coverage zoom. Constant state; repeats do not drift; jumps
      reset it. Verified on synthetic jitter (`stabilizer_test`: jitter std cut by more than half,
      a steady pan followed with the expected EMA lag) and through the engine. Limits: translation
      only (no rotation or rolling-shutter correction), motion above 16 px per frame at 1080p is
      not tracked.
- [x] Slide and zoom transitions as `Animation::slide` / `Animation::zoom` builders over the keyframe
      system (easing included); fades are `Placement::fade`.
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
- [x] Speed ramps (`Placement::speed_ramp_end`) linear in source time, the definition core and the
      exporter use: duration `L / (v1 - v0) * ln(v1 / v0)`, speed `v0 * e^(ct)`. Video and audio
      follow it block by block. The exporter's own audio uses the average speed, so a ramped
      clip's audio differs from the export by design (the preview follows the video).
- [x] Animated gain (`Animation::gain_db`): dB keyframes interpolated linearly in dB like the
      exporter's gain expression, applied per audio block with a linear gain between 64-frame steps.
- [x] Mattes (`matte.rs`): `Placement::alpha_matte` multiplies a layer's alpha (background removal,
      clip-local source time, only above the first video track like the export) and
      `Placement::privacy_blur` blurs the finished canvas through a matte (matte spread evenly over
      the clip window). A missing matte degrades to the unmasked layer. Unit tests in `matte.rs`
      and the mapper; not checked visually in the app against a generated matte. The privacy sigma
      is in preview-canvas pixels (the export uses export-canvas pixels).
- [x] Pitch-preserving time stretch (`time_stretch.rs`, WSOLA, speeds 0.25x-4x): a clip with
      `Placement::speed != 1` keeps its pitch, ends early or runs long, and consecutive blocks
      join bit-exactly; seeks restart the stretch at the matching source position. Outside the
      range the renderer returns `AudioRenderError::UnsupportedSpeed`. Cost about 6 ms per
      second of stereo output. Tests: `time_stretch_test` (pitch within 4 Hz at five speeds,
      unit speed transparent, chunking-invariant), `audio_playback_test`.
- [x] Audio-master playback clock (`Player::tick_synced`), verified against the real device:
      video follows the audio clock with zero drift in a 1.5 s run.
- [x] Long-run A/V soak on the real device (`long_run_*`, ignored by default, `BUSH_SOAK_SECS`):
      60 s run, audio clock within 7 ms of wall time at every 10 s report (relative rate error
      -42 ppm), video never led the audio clock, one underflow callback (startup).
- [x] Audio device recovery: `DeviceSink::is_failed` / `recover` / `recoveries` reopen the
      stream on the current default device (queue emptied, clock keeps counting, a format change
      is reported as `FormatChanged`). Verified on the real device by an on-demand reopen; a
      physical unplug was not exercised.

## Hardware decode

- [x] Opt-in (`DecodePolicy::HardwareWithFallback`): first hardware device the codec supports,
      shared per process (creating one costs about 250 ms), software fallback on any failure;
      output matches software within 3 mean levels. Verified on an RTX 4070 through D3D11VA.
- [x] **Measured: not faster here.** Frames are copied back to system memory and converted, so
      hardware is slower than threaded software decode for every fixture tried (see numbers).
      It stays opt-in; a win needs a zero-copy GPU path.
- [ ] Hardware paths other than D3D11VA (VAAPI, VideoToolbox) are unverified: no such machine.

## Desktop adapter (plan phase 7, `apps/desktop/src-tauri/src/bush_adapter/`)

Behind the `bush-preview` cargo feature (off by default); at runtime `OCA_PREVIEW_ENGINE=bush`
selects it and `OCA_BUSH_RENDERER=gpu` lets it composite on the GPU. Design and rollout:
[../architecture/bush-adapter-rfc.md](../architecture/bush-adapter-rfc.md).

- [x] Mapper (`mapper.rs`, 17 tests): every clip of every visible track becomes a contributor
      with its timeline window; keyframes (opacity, position, rotation, crop, colour), Ken-Burns
      zoom as an animated crop, flip, colour filters, masks, chroma key, 40 blend modes, LUT,
      fade/slide/zoom transitions, freeze (near-zero speed), gain, text and shape overlays.
      Known approximations are reported per scene (frozen audio, zoom-out, a missing matte file).
      The intensity curves come from the exporter's filter parameters (blur = boxblur variance,
      sharpen = 3x unsharp, pixelize block 2 + 48i, shake = 8% of the canvas, deflicker = 5-frame
      mean, vignette fitted to FFmpeg's corner falloff). Glitch and stabilization are different
      algorithms from the export (noise and deshake) and share only the 0..1 strength.
- [x] Backend (`backend.rs`): same command channel, latest-frame slot, playhead and tick as the
      GStreamer loop. Audio is the master clock (device sink, recovery on failure), JPEG
      encoding runs on its own thread, the next frame is requested when the previous one lands
      and paced to the source frame rate, canvas is the clip resolution capped at the preview
      size. Playback stats are logged every 2 s as `bush_stats` in `oca.log`.
- [x] Verified end to end on the real app (`e2e/test_bush_preview.py`, screenshots in
      `e2e/out/`): clip loads, seeks, plays with audio, text and shape overlays composite.
- [x] Audio level meter: `AudioPlayer::peaks_at` keeps each rendered block's peak and the
      backend reads the block the device is playing (e2e checks the bars move while playing).
- [x] Persisted `preview_engine` preference (prefs screen picker, shown only in builds with the
      feature, applied at the next start; `OCA_PREVIEW_ENGINE` overrides it). e2e:
      `test_preview_engine_pref.py`.
- [ ] `compare` tool and a GStreamer baseline run (the GStreamer preview produced no frame in
      this machine's e2e environment, so no side-by-side numbers yet).

Measured in the app, 1080p H.264 with AAC audio, 20 s clip, Bush CPU renderer (GPU renderer
identical on this single-layer scene):

| Metric | First version | After tuning |
| --- | --- | --- |
| Distinct frames delivered while playing | 16.3/s | 28-30/s (source is 30 fps) |
| Seek to a new frame | 94 ms | 47-62 ms |
| JPEG encode per frame (1280x720) | 20.5 ms | 6.7 ms |
| Render p50 (decode + composite) | 17.7 ms | 5.4 ms |
| Audio/video drift | up to 50 ms | 0-20 ms, 1 underflow at start |
| Add to timeline to first frame | | 500 ms |

Decode policy in the app (same clip, CPU renderer): software decode gives first frame in 93 ms
and render p50 4.3-4.8 ms, hardware (D3D11VA) 406 ms and 6.1-6.7 ms, with identical delivery
rate and seek time. The app's hardware-decode preference defaults to on for GStreamer, so the
Bush backend ignores it; `OCA_BUSH_DECODE=hardware` opts in.

### Decode policy, sequential playback

`cargo run --release -p bush --example playback_probe -- <video> 1280 720 240`, CPU renderer, 720p
canvas, 240 frames, `PROBE_DECODE=hardware` for D3D11VA with a CPU download (2026-10-03, this machine):

| Clip | Software p50 / sustained | Hardware p50 / sustained |
|---|---|---|
| 1080p H.264 | 7.4 ms / 130 fps | 15.0 ms / 52 fps |
| 4K H.264 | 23.8 ms / 36 fps | 44.7 ms / 21 fps |
| 4K HEVC | 21.2 ms / 46 fps | 42.2 ms / 23 fps |

Hardware decode with a download is about twice as slow as software at every size, and its first
frame takes 350-500 ms against 60-200 ms. A zero-copy path (decoder surface straight into a wgpu
texture) would remove the download, but it is DX12-only, needs NV12 plane sampling in the shader,
and cannot serve the CPU renderer, the CPU-side effects (blur, glitch, deflicker, stabilizer) or the
JPEG readback the webview needs anyway, so the saving is small for the complexity. Decision: not
built; the hardware-decode preference stays ignored (`OCA_BUSH_DECODE=hardware` still opts in).

The real 4K cost is converting full-size frames to RGBA before the compositor shrinks them to the
canvas (software 4K H.264 sustains only 36 fps with a 47 ms p95). Cheaper wins to try first:
decode proxies (the app already prefers one when it exists) or scaling inside the conversion.

What moved the numbers: encoding off the playback thread, requesting the next frame the moment
one lands (the engine cancels a render when it is superseded, so polling every 8 ms starved it),
pacing to the source rate (the engine had been re-rendering the same picture at 47 fps), and a
4:2:0 AVX2 JPEG encoder in place of the 4:4:4 one.

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
