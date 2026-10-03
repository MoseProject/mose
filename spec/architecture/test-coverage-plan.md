# Test coverage plan: 100% of the testable code

Goal: every line of logic is exercised by a test. Baseline measured 2026-10-03 with
`cargo llvm-cov` (line coverage, tests as they were on `main` at `96aa2683`) and `vitest --coverage`.

| Area | Lines covered | Missed lines |
|---|---|---|
| `crates/avbridge` (Rust) | 70.4% | 292 |
| `crates/bush` | 91.4% | 337 |
| `crates/core` | 88.5% | 2100 |
| `apps/desktop/src-tauri` (Rust) | 26.4% | 4740 |
| `apps/desktop/src` (TypeScript) | 7.0% | 1412 |

`avbridge/csrc/*.c` is C and is not measured by llvm-cov (it is also ignored in `codecov.yml`); it
is exercised through the Rust FFI tests.

## Policy: what "100%" means

100% of lines that a test can reach on a development machine without a GPU window, an audio
device, a downloaded model, the network or an elevated process. Anything else is listed in the
exclusion table below, with the reason, and is excluded from the number through
`--ignore-filename-regex` in `make coverage`, never with an inline marker that hides logic. A line
that needs hardware is first split: the decision logic moves into a pure function that is tested,
and only the one-line call into the hardware stays excluded.

Excluded by file (reason):

| File | Reason |
|---|---|
| `apps/desktop/src-tauri/src/main.rs` | Tauri builder and window setup, runs only inside a real app |
| `crates/bush/src/device_sink.rs` | Needs an audio output device (`device_audio_test` runs it where one exists) |
| `crates/bush/src/gpu_*.rs` | Needs a GPU adapter; parity covered by `gpu_parity_test` where one exists |
| `crates/core/src/update_check.rs` network and install calls | Needs GitHub and an elevated installer; the pure parts are tested |
| `crates/core/src/youtube_download.rs` process launch | Needs yt-dlp and the network; argument building and parsing are tested |

The list is reviewed at the end of each phase and only shrinks.

## Method

Test first for each uncovered branch (red, then green), one commit per module, named
`test(<crate>): cover <module>`. A production change is allowed only when it makes a line testable
(extract a pure function, inject a path or a clock) and goes in its own `refactor:` commit before
the test commit. Placement follows `CLAUDE.md`: `core`, `avbridge` and `bush` integration tests in
`crates/<crate>/tests/`, private helpers in `src/<module>/<module>_test.rs`, `desktop` Rust in
`src/<module>/<module>_test.rs`, frontend in `*.test.ts(x)`.

Measure with `make coverage` (added in phase 0) and record the new number in this file after every
phase.

## Phase 0: tooling

- [ ] `make coverage` target: `cargo llvm-cov` per crate with the exclusion regex, summary table
- [ ] `make coverage-web`: `vitest run --coverage`
- [ ] `@vitest/coverage-v8` dev dependency, `coverage` block in `vite.config.ts`
- [ ] Commit this plan

## Phase 1: `bush` (91.4%, 337 lines)

- [ ] `cpu_renderer.rs` 57: blend and mask branches not hit
- [ ] `engine.rs` 50: GPU fallback, cancel paths, matte fetch failure, privacy blur
- [ ] `preview_worker.rs` 46 (72% -> 100%): command failure and shutdown paths
- [ ] `audio_playback.rs` 30, `player.rs` 28, `scene.rs` 24 (validation errors)
- [ ] `audio_source.rs` 17, `source_variants.rs` 17, `keyframes.rs` 16, `decode_source.rs` 12
- [ ] The rest: `audio_mixer`, `layer_mods`, `frame_mailbox`, `blend8`, `time_stretch`,
      `stabilizer`, `video_frame`, `byte_cache`, `deflicker`

## Phase 2: `avbridge` (70.4%, 292 lines)

- [ ] `src/lib.rs` 279: error mapping for every FFI wrapper (bad path, bad arguments, cancel)
- [ ] `src/audio_decode.rs` 13

## Phase 3: `core` (88.5%, 2100 lines), biggest first

- [ ] `render.rs` 398 (63%): segment resolution for every effect, multi-track, text/shape/privacy passes
- [ ] `timeline/clip.rs` 138, `timeline/mod.rs` 122, `timeline/track.rs` 24
- [ ] `transcribe.rs` 108, `text_to_speech.rs` 88 (pure parts; models are not downloaded)
- [ ] `export_queue.rs` 104, `youtube_download.rs` 106 (argument building, output parsing)
- [ ] `auto_reframe.rs` 90, `interchange/otio_json.rs` 76, `motion_template.rs` 66
- [ ] `update_check.rs` 175 (parsing, version compare, asset selection)
- [ ] `background_removal.rs` 43, `error_reporting.rs` 42, `watched_folder.rs` 35, `bundle.rs` 31
- [ ] Everything under 30 lines: `font_catalog`, `transcript`, `interchange`, `privacy_blur`,
      `motion_tracking`, `semantic_index`, `project`, `lut`, `text_layout`, `collab_bundle`, `keyframe`,
      `nested_sequence`, `gameplay_events`, `probe`, `phonemizer`, `telemetry`, `overlay_render`,
      `voice_cleanup_preview`, `loudness`, `persistence`, `frame_sampler`, `proxy`, and the tail

## Phase 4: `desktop` Rust (26.4%, 4740 lines)

Commands need an `AppHandle`. Use Tauri's `test` feature (`tauri::test::mock_app`,
`get_ipc_response`) so a command runs through the real IPC path with a mock runtime.

- [ ] Test harness: mock app, `AppState`, a session with a tiny fixture project
- [ ] `commands/timeline.rs` 491, `commands/overlays.rs` 184, `commands/markers.rs` 26
- [ ] `bush_adapter/backend.rs` 456 (the pipeline logic that does not need a window), `mapper.rs` 47
- [ ] `error_reporting.rs` 427 and `commands/error_reporting.rs` 91
- [ ] `export.rs` 364 and `commands/export.rs` 69
- [ ] AI command modules: `auto_reframe`, `transcribe`, `privacy_blur`, `background_removal`,
      `motion_tracking`, `scene_detection`, `silence`, `highlight_detection`, `tts` (single-flight
      guards, error events, state transitions; the heavy call is injected)
- [ ] `commands/prefs.rs`, `prefs.rs`, `state.rs`, `autosave.rs`, `commands/mod.rs`, `media.rs`,
      `watch_folder.rs`, `sequences.rs`, `youtube_download.rs`, `update_check.rs`, `sound_library.rs`
- [ ] `protocol.rs`, `preview_engine.rs`, `telemetry.rs`, `frames.rs`

## Phase 5: frontend (7.0%, 1412 lines)

- [ ] Vitest setup for React components (`@testing-library/react`, jsdom, mocked `ipc.ts`)
- [ ] `store.ts`, `i18n.ts`, `types.ts`, `ipc.ts` (one test per wrapper checking the command name
      and arguments)
- [ ] `timelineGeometry.ts` stays covered; `editor/Timeline.tsx` 407 (canvas drawing against a fake
      2D context, pointer handlers through the pure geometry functions)
- [ ] Panels: `PreviewPanel`, `MediaLibrary`, `Inspector`, `TextInspector`, `ShapeInspector`,
      `OverlayFields`, `KeyframeEditor`, `LayerTemplates`, `TranscriptPanel`, `SilenceReviewPanel`,
      `SequenceTabs`, `ExportPanel`
- [ ] Screens: `Editor`, `Home`, `Prefs`, `WatchFolder`, `CrashReviewModal`, `App`

## Phase 6: close out

- [ ] CI coverage step uses `make coverage` and fails below the recorded number
- [ ] `codecov.yml` target raised to the achieved number, exclusions mirrored
- [ ] This file: final numbers and the final exclusion table
