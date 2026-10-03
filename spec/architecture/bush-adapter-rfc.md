# RFC: Bush preview adapter (plan phase 7)

Status: **approved 2026-10-03, first slices implemented behind the `bush-preview` feature**
(off by default; GStreamer stays the default engine). Per
[preview-engine-replacement.md](preview-engine-replacement.md) the adapter was a separate
approval point. Implemented: the timeline-to-scene mapper, the playback backend (seek, step,
play with audio-master clock, device recovery, text and shape overlays, LUT), engine selection
(saved `preview_engine` preference with a picker on the preferences screen, or
`OCA_PREVIEW_ENGINE=bush`), the audio level meter, and e2e tests that drive the real app.
Still open: the `compare` tool, the intensity curves (gap 9, provisional constants), a
GStreamer baseline and removal of GStreamer. Measured results are in
[../matrix/bush.md](../matrix/bush.md). The rest of this document is the original proposal and
is still the plan for what remains.

## Goal and non-goals

Let the desktop app play its preview through Bush instead of GStreamer, selectable at runtime,
with GStreamer untouched and instantly restorable. Export is out of scope (it stays on `avbridge`).
No project, timeline or `.ocproj` schema changes. `ytbridge` is unaffected.

## Where the adapter lives

`apps/desktop/src-tauri/src/preview_engine.rs` is the single place that drives preview today: a
dedicated thread owns an `avcore::preview_worker::PreviewWorker`, `resolve_layers` turns the
session timeline into a `Layers` value (background, overlays, audio, text, shapes), and
`live_updates_between` decides reopen versus live update. The adapter is a second backend behind
the same `PreviewEngine` handle (`Command` channel, `latest_frame()`, playhead atomics), so
`state.rs`, the commands and the whole webview stay unchanged. Bush must not be imported by
`core`: the mapper lives in `apps/desktop/src-tauri/src/bush_adapter/` and is the only code that
knows both `avcore` types and Bush DTOs, exactly as the engine plan requires.

```text
PreviewEngine (existing handle)
  |-- GstBackend   : PreviewWorker (today)
  `-- BushBackend  : bush::Engine + bush::Player (+ DeviceSink/AudioPlayer)   [new, behind flag]
```

## Mapping

| Desktop side (today) | Bush side | Notes |
| --- | --- | --- |
| `Layers` at the playhead | `SceneSnapshot` (one contributor per visible/audible clip) | Built by `resolve_layers` data plus `ClipInstance`; clips not active at the playhead are still included with their timeline window so Bush, not the adapter, decides activity. |
| `PathBuf` of proxy or original | `MediaSource { path, range, version }` | `preview_source_path` already picks the proxy; `version` = asset id + proxy generation so a re-proxy invalidates caches. |
| `ClipInstance.start_secs`, `source_in/out`, `speed_factor` | `Placement.timeline_start_secs`, `SourceRange`, `Placement.speed` | `frozen` maps to a one-frame range with a long window. |
| `opacity_keyframes`, `position_keyframes`, `scale_keyframes`, `rotation_keyframes` | `Animation` tracks | Core `Keyframe<T>` interpolation must be checked against Bush easing (see gaps). |
| `transition_in` + `transition_duration_secs` | `Placement.fade`, `Animation::slide`, `Animation::zoom` | `HardCut`/`None` map to nothing. |
| `brightness/contrast/saturation/vignette` | `LayerEffects` | `ColorFilter::BlackAndWhite` is saturation 0; `Sepia` needs a LUT or a new effect. |
| `blur_intensity`, `sharpen`, `pixelize_intensity`, `glitch_intensity`, `shake_intensity` | `blur_sigma`, `sharpen`, `pixelize_block`, `glitch`, `shake` | Intensity-to-parameter curves must reproduce the GStreamer look; fixed once with side-by-side frames. |
| `chroma_key_*`, `mask_shape` (`Circle`, `RoundedRect`) | `ChromaKey`, `LayerMask` (`Ellipse`, `Rectangle` with corner radius) | `mask_corner_radius` maps to `LayerMask::corner_radius`. |
| `lut_path` | `Lut3D` shared through `Placement.lut` | `.cube` parsing stays in `core` (`Lut3D` already exists there); the adapter converts the table. |
| `crop_*` and crop keyframes | `LayerTransform.crop` | Crop keyframes are a gap. |
| `blend_mode` of overlay tracks | `BlendMode` | Core exposes more modes than Bush (see gaps). |
| text and shape clips | `RasterOverlay` contributors plus `EngineRequest::overlays` RGBA | `core::overlay_render` already rasterizes; the adapter pushes bytes and bumps the overlay version only when text or shape content changes, which is the existing live-update path. |
| nested/compound clips | already materialized to synthetic assets by `materialize_nested_sequences` | No Bush change. |
| playhead, play/pause, rate, seek | `Player::seek/play/pause/set_rate/tick` | The preview thread supplies `now` from its loop clock. |
| `latest_frame()` RGBA `Arc<Vec<u8>>` | `EngineEvent::Frame` pixels | One copy into the existing latest-frame slot; dimensions already travel with the frame. |
| audio level meter, mute, gain, `gain_keyframes`, roles | `AudioRenderer` + `AudioPlayer` + `DeviceSink` | Meter is computed from the mixed block before the sink. |
| `HardwareDecode(bool)` command | `EngineConfig::decode_policy` | Measured not faster here; keep the setting but default it off for Bush. |
| GPU/CPU choice | `EngineConfig::renderer` | `GpuWithCpuFallback` once parity is accepted on target hardware. |

## What Bush needs before the adapter can reach parity

These are the real gaps found by reading `ClipInstance` against the Bush scene model. Each is
small, but none exists today, and the adapter must not paper over them:

1. ~~Keyframes on effect parameters~~ **done**: `Animation` now animates brightness, contrast,
   saturation and the crop window (kept a valid sub-rectangle at every instant).
2. ~~Speed ramps~~ **done**: `Placement::speed_ramp_end` ramps speed linearly; video and audio
   follow the integrated source time (the audio stretcher changes speed block by block).
3. ~~`flipped_h`~~ **done**: `LayerTransform::flip_x`/`flip_y` (CPU and WGSL, byte-exact parity).
4. ~~`Sepia`~~ **done**: `LayerEffects::sepia` (classic matrix, mix 0..1, CPU and WGSL).
5. ~~Blend modes beyond the eight in Bush~~ **done**: all 40 core modes exist
   (`BlendMode::ALL`, `ffmpeg_name()`), bit-exact against FFmpeg `blend` (see the matrix).
6. ~~Audio master clock~~ **done in Bush**: `Player::tick_synced(now, audio_position_secs)` follows the
   audio device clock (never rewinds), `drift_against` reports sync health; measured on the
   development machine device (video tracks the audio clock exactly, audio clock within 1 % of
   wall time over 1.5 s). The adapter must feed it `DeviceSink::consumed_frames / sample_rate`.
   Long-run (minutes) drift and device loss are still unmeasured.
7. **Voice cleanup** (`voice_cleanup_*`): offline DSP stays outside Bush. The adapter either
   bypasses it in preview (as GStreamer does today if that is the current behaviour; to be
   confirmed) or consumes a pre-rendered cleaned asset.
8. **Position/scale easing parity**: confirm core keyframe interpolation matches Bush easing, or
   bake core keyframes into dense Bush tracks.
9. **Intensity curves** for blur, pixelize, glitch and shake (table above).

## Threading and data flow

`BushBackend::run()` replaces `worker.live_update/open/seek` calls: on every dirty tick it builds
a `SceneSnapshot` only when the timeline or media changed (scene versions make unchanged ticks
free), calls `Player::set_scene` for edits, `seek` for scrubs and `tick(now)` while playing, and
polls `Player::poll()` into the existing latest-frame slot and `preview-frame` event. Overlay
rasters are cached by content version. No Bush call blocks the UI thread: `Engine` is already
a worker with latest-wins submission.

## Telemetry

Expose `Engine::latency()` (p50/p95/p99), `Engine::stats()`, `Engine::renderer_name()` and the
decode policy through the existing diagnostics command, labelled by backend, so a user report
states which engine produced a stall. Failures arrive as `EngineEvent::Failed` with
redacted file names only.

## Rollout

1. Cargo feature `bush-preview` on the desktop crate (off by default) so release builds do not
   carry wgpu/cpal until chosen.
2. Preference `preview_engine = "gstreamer" | "bush"` (default `gstreamer`), read at startup;
   switching needs a restart in the first release.
3. A developer-only `compare` command renders the same timeline positions through both backends
   and reports per-pixel and per-sample differences, reusing Bush parity tooling. Every gap above
   gets a row in a parity table that must be green before the default changes.
4. Soak: run the existing e2e flows against `bush` in CI with a software-only renderer, and the
   GPU renderer on one hardware runner.

## Rollback

Flip the preference (or build without the feature). GStreamer code, `core::preview*` and its
tests are not modified by the adapter, so rollback needs no revert and no data migration.

## Deletion criteria for the GStreamer preview

All of: every gap above closed; the parity table green on Windows, macOS and Linux runners; two
release cycles with `bush` as default and no preview-class regressions; measured scrub, playback
and live-edit latency at least as good as GStreamer on the benchmark set in
[../matrix/bush.md](../matrix/bush.md); packaging verified without GStreamer DLLs for the
preview path. Only then remove `core::preview`, `preview_worker` and the GStreamer dependency
in separate commits, each independently revertible.

## Risks

- Hardware decode did not beat threaded software decode on tested media; users with only
  integrated GPUs may see different results, so the policy stays a setting.
- GPU readback caps the GPU renderer gain for simple scenes; a zero-copy presentation path to the
  webview would change the frame contract and is a separate design.
- Non-Windows device audio and hardware decode are unverified (no machines yet).
- Two engines in one binary raise build time and size; the feature flag contains it.

## Decision requested

Approve the adapter work in two steps: (a) close gaps 1-5 and 9 inside `crates/bush` with
their own tests, then (b) implement `BushBackend` behind the feature flag with the `compare`
tool. Step (b) must not start before (a) lands.