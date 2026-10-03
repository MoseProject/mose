# Plan: Bush as the only preview engine, GStreamer removed

Branch `refactor/remove-gstreamer`, backup tag `pre-gstreamer-removal` (local). Decision by the
project owner (2026-10-03): replace GStreamer with Bush completely. The RFC's slower removal
criteria ([bush-adapter-rfc.md](bush-adapter-rfc.md): parity table, two release cycles, three
operating systems) are waived. Each step below is its own commit so it can be reverted alone.
Tick a box when the step is done and verified.

Decisions taken (defaults recommended in the plan review):

- Work on the branch, merge to `main` after phase 6.
- The hardware-decode preference is removed from the preferences screen (Bush decodes in
  software; `OCA_BUSH_DECODE=hardware` stays as a developer switch).
- The Bush GPU renderer stays in the build as opt-in (`OCA_BUSH_RENDERER=gpu`), CPU by default.
- Bush gaps (mattes, intensity curves, speed-ramp curve, gain keyframes) are follow-ups, listed
  in phase 7.

## Phase 0: safety net

- [x] Create the local tag `pre-gstreamer-removal` on the current `main`
- [x] Create and switch to the branch `refactor/remove-gstreamer`

## Phase 1: `core` frame sampling without GStreamer

`FrameSampler` decodes through `preview::Preview`; six AI features, thumbnails and the matte
check depend on it. Rebuilt on Bush's `DecodeSource` (which wraps `avbridge::DecodeSession` and
already has the tested "frame at or before T" seek), instead of duplicating that logic. Side
effect: `core` now depends on `bush` (default features, no GPU or audio device).

- [x] Read `avbridge::DecodeSession` (`open_with_policy`, `advance`, `take_rgba`) and how Bush's
      `DecodeSource::frame_at` seeks, to reuse the same approach
- [x] Add `SampledFrame { width, height, rgba }` in `core` (replaces `preview::VideoFrame`)
- [x] Rewrite `FrameSampler::open` / `sample` on `bush::DecodeSource`, with its own error type
      (`SamplerError`) instead of `PreviewError`; the poll interval parameter is gone and the
      deadline is checked once the decode returns
- [x] Keep `FrameSampler::even_sample_times` unchanged (pure scheduling)
- [x] `scopes.rs` needed no code change (it takes plain RGBA); its doc comment is updated in
      phase 3 with the other stale references
- [x] Update `render.rs` (matte frame check) to the new sampler API
- [x] Update the desktop callers: `scene_detection`, `auto_reframe`, `background_removal`,
      `motion_tracking`, `privacy_blur`, `protocol.rs` (thumbnails), `frames.rs`
- [x] Test: sampler against a real fixture, frame at T equals a direct `DecodeSource` decode
- [x] Test: sampling backwards and past the end, a missing file and a zero deadline return
      `None`/error, no panic
- [x] `cargo check -p core -p desktop` and the `frame_sampler` tests pass (12 tests, release)
- [x] Commit (`core`), then commit (`desktop` callers) separately

## Phase 2: `desktop` runs only Bush

- [x] Make `bush` a regular dependency of `desktop` (features `gpu`, `cpal`); remove the
      `bush-preview` feature
- [x] `PreviewEngine::spawn` always runs the Bush backend; drop the engine selection
      (`bush_adapter::selected`, `OCA_PREVIEW_ENGINE`)
- [x] Delete the GStreamer loop in `preview_engine.rs`: `run`, `Layers`, `live_updates_between`,
      `resolve_layers`, `open_request`, `seek_for`, `timeline_position`,
      `refresh_text_highlights`; keep `PreviewTick`, `Command`, `LatestFrame`,
      `preview_source_path`, the interval constants
- [x] Delete `preview_engine_test.rs` tests for the removed functions; keep tests that still apply
      (none did: the file only covered the removed functions)
- [x] Remove the `preview_engine` preference field, the `preview_engines` command and the
      picker in the preferences screen (`types.ts`, `ipc.ts`, `i18n.ts`, `Prefs.tsx`)
- [x] Remove the hardware-decode checkbox from the preferences screen and the
      `preview_set_hardware_decode` command; keep the stored field so existing `prefs.oc` files
      still load
- [x] Rename `e2e/test_bush_preview.py` to `e2e/test_preview.py`, drop the engine skip; delete
      `e2e/test_preview_engine_pref.py`
- [x] `cargo check -p desktop --all-targets`, `npm run typecheck`, `npx vitest run` (23 tests)
- [x] e2e: preview loads, seeks, plays with audio, text/shape overlays, meter, cut (1080p clip,
      28.8 frames/s, seek 47 ms, first frame 78 ms, built with no feature flag)
- [x] Commits: desktop backend, desktop frontend, tests

## Phase 3: `core` drops the GStreamer preview

- [ ] Move `Lut3D` (`parse`, `load`, `size`, `entries`) out of `preview_effects.rs` into `lut.rs`
- [ ] Delete `preview.rs`, `preview_worker.rs`, `preview_live_updates.rs` and the rest of
      `preview_effects.rs`; fix `lib.rs` exports
- [ ] Delete `tests/preview_test.rs`, `tests/preview_live_updates_test.rs`,
      `tests/gstreamer_link_test.rs`
- [ ] Remove `gstreamer`, `gstreamer-app`, `gstreamer-video` from `crates/core/Cargo.toml`
- [ ] `error_reporting.rs`: drop the linked GStreamer version; keep the report field optional so
      stored and queued reports still parse
- [ ] `bundle.rs`: remove the `GST_PLUGIN_*` and scanner setup
- [ ] Update the doc comments that point at the removed functions (`timeline/clip.rs`,
      `text_clip.rs`, `overlay_render.rs`, `motion_tracking.rs`, `scopes.rs`)
- [ ] `cargo check --workspace --all-targets` and `cargo clippy` without GStreamer in the
      environment (unset `PKG_CONFIG_PATH`)
- [ ] `cargo test -p core` in debug (the mingw GStreamer crash should be gone)
- [ ] Commits: core code, core tests, error reporting/bundle

## Phase 4: build, CI and packaging

- [ ] `.github/workflows/ci.yml`: remove GStreamer from the three jobs; Linux installs the ALSA
      development package for `cpal`
- [ ] `.github/workflows/release.yml`: remove GStreamer install and packaging on Windows, Linux
      and macOS
- [ ] `.circleci/config.yml`: remove the GStreamer packages, add ALSA development
- [ ] `packaging/assemble_windows.ps1`, `assemble_linux.sh`, `assemble_macos.sh`,
      `bundle-manifest.json`: remove GStreamer
- [ ] `Makefile` and `e2e/conftest.py` / `make test-e2e`: remove GStreamer paths
- [ ] `docs/dependency-bundle.md`: rewrite without GStreamer
- [ ] Decide on `avbridge/build.rs` import-lib renaming (kept for now); verify then remove or
      keep, in its own commit
- [ ] Clean build of the desktop app through the Tauri CLI with no GStreamer in the environment
- [ ] Compare binary and bundle size before and after

## Phase 5: docs and specs

- [ ] `CLAUDE.md`, `AGENTS.md`, `.claude/CLAUDE.md` and the `.claude/*` files that mention GStreamer
- [ ] `README.md`
- [ ] `spec/matrix/bush.md`, `spec/architecture/bush-adapter-rfc.md` (removal done, criteria
      waived, what remains), `preview-engine-replacement.md`, `spec/INDEX.md` link to this plan
- [ ] Project memory notes

## Phase 6: final verification

- [ ] `cargo test --workspace` in debug
- [ ] `cargo clippy --workspace --all-targets`
- [ ] `npm run typecheck` and `npx vitest run`
- [ ] Full e2e suite against the built app; screenshots of the preview
- [ ] Preview performance re-measured on the 1080p clip (frames/s, seek, first frame, drift)
- [ ] Merge `refactor/remove-gstreamer` into `main`; push only on the owner's say-so

## Phase 7: follow-ups (not part of the removal)

- [ ] Calibrate blur, sharpen, pixelize and shake intensity curves against export
- [ ] Speed ramp linear in source time (core) instead of the average-speed approximation
- [ ] Gain keyframes in the audio renderer
- [ ] Render background-removal and privacy-blur mattes in the preview
- [ ] Verify audio and decode on macOS and Linux
- [ ] Zero-copy hardware decode path, then revisit the hardware-decode preference
