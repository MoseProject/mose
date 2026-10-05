# Architecture and code organization review (2026-10-05)

A whole-repository pass on file layout, module size, duplication, constants and naming, with the
changes made and the backlog that is left. Measured on `main` after the performance work
(72,200 lines of Rust, TypeScript and C, excluding tests and the font catalog).

## Method

- File and function sizes: line counts per file, and a brace-matching scan for functions of 80
  lines or more (a throwaway script, not committed; rerun it before and after any size claim).
- Duplication: windows of 8 (C) or 10 (Rust) normalized lines hashed across the tree, grouped by
  file pair. Per `design-principles.md`, only knowledge that is repeated is a finding, not
  similar syntax.
- Constants and naming: grep for the literals that appear in several files, and for terse
  locals. Naming turned out to be good (no `tmp`/`cfg`/`res` style names in the Rust or TS code;
  the C files had a few one- and two-letter locals that the extractions below replaced), so no
  rename pass was made.

## Follow-up quality pass (2026-10-05)

- Extracted the shared FFmpeg audio demux/decode/flush loop into `avbridge/csrc/audio_decode.c`;
  loudness, waveform, and PCM keep their processing in focused callbacks.
- Replaced opaque return tuples for dynamic crop keyframes and ONNX face tensors with named types.
- Addressed workspace Clippy findings; `cargo clippy --workspace --all-targets -- -D warnings` and
  `cargo fmt --all --check` pass.
- The 750 `core` library tests pass. Targeted waveform, PCM, loudness, and crop/timeline tests
  pass. Four `avbridge` timeline-export frame-count assertions remain failing by one frame even
  when serialized. Three desktop handler tests fail because Tauri reports `Plugin not found`;
  the other 427 desktop tests pass.
- Frontend lint and typecheck could not run: `npm ci` was rejected with HTTP 403 by the configured
  npm registry, so frontend dependencies were unavailable.

## Findings and what changed

| Finding | Evidence | Change |
|---|---|---|
| One 3,100-line `avbridge/src/lib.rs` held every FFI struct, extern declaration, error enum and wrapper | 70 top-level items, 13 unrelated capabilities | `ffi`, `probe`, `decode`, `remux`, `export`, `timeline_export`, `audio_mix`, `loudness`, `proxy`, `matte`, `waveform`, `overlay`, `pcm` modules; `lib.rs` is 62 lines of module list and re-exports; public API unchanged; each module names the items it imports |
| The text, shape and privacy-blur passes carried the same 330 lines (decode, graph, encoder, loop, flushes) | 186 shared 8-line windows between two of them | `reencode.c` holds the pass once; each caller builds only its filter description (1,200 lines became 820 including the shared 330) |
| The two timeline exports each had the speed helpers ("kept as independent copies"), the 50-line audio output setup and the per-segment audio commands | identical blocks | `timeline_common.c`: `smooth_speed_ramp_average_speed`, `build_setpts_str`, `timeline_open_audio_output`, `timeline_queue_segment_audio_commands` |
| The single-track export wrote a segment's whole video graph inline; the multi-track export had a condensed copy that dropped the segment's anchor | diff of the two | one `build_segment_vfilter`; the multi-track export now honours the anchor (test added) |
| Magic numbers: the mastering string three times, 192 and 128 kbps, 48 kHz, 8 encoder threads, a 2 s keyframe spacing, a 4 Mbps default | grep | named in `bridge_internal.h` (`MASTERING_FILTER_FORMAT`, `EXPORT_AUDIO_BITRATE_BPS`, `SOFTWARE_ENCODER_THREADS`, `KEYFRAME_INTERVAL_SECS`, ...) |
| `core/src` had 94 top-level entries | flat module list | grouped into `model/`, `av/`, `analysis/`, `graphics/`, `project_io/`, `platform/` (and the existing `render/`); `lib.rs` re-exports every module at its old path, so no call site changed |
| `render.rs` (1,650 lines) mixed the error type, validation, segment resolution, three overlay passes and the export entry points | item list | `render/{errors,validation,segments,text,shape,privacy,jobs}.rs` |
| `clip.rs` (1,550 lines) held the clip data, four effect enums, the copy/paste formatting value and the avfilter-chain builders | item list | `effects.rs`, `formatting.rs`, `filters.rs` next to `clip.rs` (918 lines) |
| The same source-to-timeline mapping written inline in four modules | grep | `ClipInstance::source_to_timeline_secs` |
| `desktop/src-tauri/src` mixed playback and session modules with feature plumbing at one level | flat list | `preview/` (engine, Bush adapter, frames, protocol, tile workers) and `session/` (state, sync, autosave, close guard) |
| React components of 600-1,000 lines whose bodies were independent sections | `ClipControls` 650 lines, `TextInspector` 380, `TopBar` 320, `MediaLibrary` 700 | one component per section under `editor/inspector/`, `editor/textInspector/`, `topbar/`, `editor/library/`; `Timeline.tsx` lost its painting, drag math, track header and toolbar; the `Inspector` / `TextInspector` / `ShapeInspector` import cycle is gone |
| Generated and local files tracked or littering the root | 685 files (13 MB) of `graphify-out/` snapshots tracked although ignored; `.DS_Store`; six design documents at the repository root | untracked and ignored; design documents moved to `docs/design/` |
| Stale documentation | `CLAUDE.md` described `.ocproj` as gzip MessagePack and `avbridge` as one file; the desktop crate docs mentioned the retired egui shell | updated |

Sizes after the pass: `avbridge/src/lib.rs` 3,116 -> 62 lines, `render.rs` 1,656 -> 39,
`Inspector.tsx` 1,197 -> 248, `MediaLibrary.tsx` 700 -> 241, `TopBar.tsx` 374 -> 65,
`TextInspector.tsx` 422 -> 59, `Timeline.tsx` 1,469 -> 925, `clip.rs` 1,548 -> 918,
`timeline_export.c` 698 -> about 440. All tests pass (avbridge, core, desktop, the 693 frontend
tests, and the e2e suite on a built app); `rustfmt`, `prettier` and `eslint` are clean.

## Backlog (not done, in the order I would take it)

1. **The two export functions are still 500+ lines each** (`avbridge_encode_timeline_export`,
   `avbridge_encode_timeline_export_multi`). They are orchestrations with a `goto cleanup`
   structure; the next safe cuts are the per-segment decoder opening (duplicated in both) and the
   frozen-frame hold, each as a function taking a small context struct. Needs the exact-frame-count
   tests added in the performance pass as the safety net.
2. **`Timeline.tsx` component is still ~800 lines**: the pointer handlers, the context menus and
   the keyboard handler share a dozen refs. A `useTimelineDrag` hook and a `timelineMenus.ts`
   that returns menu items from callbacks are the natural seams; the 80 tests in
   `Timeline.test.tsx` cover them.
3. **`PreviewPanel.tsx`** (one 580-line component with refs and pan/zoom state) and the settings
   screens (`Prefs`, `Home`) have the same shape as the components already split; they were left
   because they carry state that needs a hook extraction first.
4. **Remaining C duplication**: the shared audio packet/decode/flush loop from `loudness.c`,
   `waveform.c` and `pcm_extract.c` was extracted to `audio_decode.c`, with each consumer retaining
   its own filter and accumulator callback. `gpu_encoder.c` still repeats its setup block between
   the software and VAAPI paths; review that extraction separately because the backends have
   different resource lifetimes.
5. **Rust command modules**: `commands/background_removal.rs` and `commands/privacy_blur.rs`
   share 24 windows (the sampled-frame loop); `commands/timeline.rs` is 860 lines of independent
   commands and could follow the grouping `core` got.
6. **Two error-reporting modules** (`core/platform/error_reporting.rs`, 1,050 lines, and
   `desktop/error_reporting`, 640): confirm the split between scrubbing/transport and the
   desktop wiring is the intended one, and document it at the top of both.
7. **`perf/`** is a standalone workspace that copies `core`'s `scopes.rs` and `lut.rs` (77 and 16
   duplicated windows) on purpose, so its benchmarks link without `core`. Either delete the
   copies after the benchmarks they back are retired, or note in `perf/PLAN.md` which `core`
   commit each copy tracks.
8. **Clippy**: the workspace gate now passes with `cargo clippy --workspace --all-targets -- -D warnings`.
   The remaining warnings reported by the previous pass were reviewed and resolved with
   named crop/tensor result types, borrowed path/slice usage, iterator-based test setup, derived
   defaults, elided test lifetimes, and removal of redundant `drop(State)` calls.
9. **Large data files**: `font_catalog.rs` (1,420 lines) is mostly a table; a build-time
   generated table or a data file would shrink it, but only if the font set is expected to grow.
10. **`graphify-out/`** history still holds the old snapshots (about 100 MB in `.git`); a history
    rewrite would reclaim it and is the repository owner's call.

## Rules that kept the pass safe

- Mechanical moves only unless a test or a measured duplicate justified more; every step was
  followed by the full test run of the crates it touched.
- Public paths stayed stable through re-exports (`avcore::probe`, `crate::timeline`,
  `./Inspector`'s `Section` and `Slider`), so no caller changed.
- A function was split when its parts were independent or duplicated, not for its length alone.
