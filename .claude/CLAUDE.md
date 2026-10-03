# Project Instructions

`oca` — a native Rust desktop video editor for cutting gameplay footage, shipped as a Tauri 2
app (React/TypeScript webview over a Rust backend). Headline features: automatic loudness
normalization and export-that-matches-the-source-bitrate. This file adapts the generic
`.claude/` package to this repository's actual architecture — see `CLAUDE.md` (repo root) for
the authoritative build/test/env instructions; this file supplements it with desktop-UI-specific
workflow guidance.

**The original shell was `egui`/`eframe` (`crates/ui`), deleted once the Tauri shell reached
full parity** — see [`spec/architecture/tauri-migration.md`](../spec/architecture/tauri-migration.md)
for that history. `engineering/EGUI.md`, `engineering/ARCHITECTURE.md`, and
`agents/egui-specialist.md` describe that deleted shell's implementation details and are kept
only as historical reference for anyone tracing an old decision back to its egui-era rationale —
do not follow them as current guidance; the equivalent live guidance is below and in
`apps/desktop`'s own source. `domain/*.md` (the `avcore` data model) and `design/DESIGN_SYSTEM.md`
still apply as-is — the domain model and visual language didn't change with the shell.
`ux/WORKFLOWS.md` is domain-level but was written against the egui shell's own naming
(`App::selected_clip_id`, `screens/editor/properties_panel/`, etc.) — read it for the workflow
shape, not the literal Rust paths, which now live in `apps/desktop/src/`.

## Real workspace structure

Dependency direction: `avbridge -> core -> desktop` (`apps/desktop`), plus an isolated
`ytbridge`:

- **`avbridge`** — thin hand-written FFI wrapper (`src/lib.rs`, ~1700 lines) over
  `csrc/*.c` (`export.c`, `filters.c`, `probe.c`, `proxy.c`, `timeline_export.c`,
  `timeline_export_multi.c`, `audio_mix.c`, `loudness.c`, `waveform.c`, `text_overlay.c`,
  `shape_overlay.c`, `matte_encode.c`, `gpu_encoder.c`), which call libavformat/libavcodec/
  libavfilter/libavutil directly. `build.rs` locates FFmpeg via `FFMPEG_DIR` and gives
  Windows import libs unique names so the link cannot pick another FFmpeg's copies.
- **`core`** — package name `core`, **`[lib] name = "avcore"`** (Rust code imports it as
  `avcore`, never `core`, since that would shadow libcore). ~35 top-level modules: domain
  (`project.rs`, `timeline.rs`, `undo.rs`), persistence (`persistence.rs` — `.ocproj`/`.ocqueue`/
  `.octr` framing), media (`probe.rs`, `media.rs`), a frame sampler over Bush's decoder
  (`frame_sampler.rs`), FFmpeg export rendering (`render.rs`, `export.rs`, delegating to
  `avbridge`), plus one module per AI/analysis feature (`transcribe.rs`/`transcript.rs`,
  `scene_detection.rs`, `background_removal.rs`, `motion_tracking.rs`, `auto_reframe.rs`,
  `highlight_detection.rs`, `silence_detection.rs`, `nested_sequence.rs`, etc). Depends on
  `bush` (decoder), `whisper-rs` (CPU), `ort` (ONNX Runtime),
  `espeak-rs` (TTS), `rmp-serde`+`flate2` (MessagePack+gzip persistence). Logic the desktop
  shell needs belongs here, never duplicated in `apps/desktop` itself.
- **`apps/desktop`** — the app itself. `src-tauri/` (Rust crate `desktop`, bin `oca-desktop`):
  `state.rs` holds `AppState`/`Session` (the backend-owned project state — the webview never
  owns it) and `publish()` (the one place every mutating command ends, emitting
  `session-changed` and feeding `PreviewEngine`); `commands/` is one file per feature area
  (`timeline.rs`, `overlays.rs`, `export.rs`, `sequences.rs`, ...), each `#[tauri::command]`
  either read-only or wrapped in `commands::mod::undoable`/`with_session`; `preview_engine.rs` is
  the dedicated playback thread driving the `bush` engine through `bush_adapter/`. `src/`
  (React/TypeScript, Vite): `store.ts` (`uiStore` transient/frontend-only state, `sessionStore`
  the backend's published `SessionView`, `previewStore` the playback tick), `ipc.ts` (the *only*
  module calling `invoke()` — every Rust command has one typed wrapper here), `editor/` (one
  file per panel: `Timeline.tsx` is a hand-painted `<canvas>`, no per-clip DOM element;
  `Inspector.tsx`/`TextInspector.tsx`/`ShapeInspector.tsx` the properties panel;
  `PreviewPanel.tsx` the program monitor), `i18n.ts` (**all UI strings live here**, pt-BR and
  English — never hardcode display text), `timelineGeometry.ts` (pure time<->pixel/hit-test
  math, shared by `Timeline.tsx` and its tests).
- **`ytbridge`** — standalone binary, deliberately isolated from `core`/`desktop`'s dependency
  graph; vendors its own Python runtime via `pyo3` for yt-dlp. Do not couple it to
  `core`/`desktop` internals.

## Real build/test/quality commands

Use `make build`/`make test`/`make test-core`/`make desktop-dev`/`make test-desktop`/
`make test-e2e`/`make lint`/`make fmt`/`make check` exactly as documented in the repo-root
`CLAUDE.md` — do not invent alternative cargo/npm invocations. Requires `FFMPEG_DIR`,
and for `whisper-rs` `LIBCLANG_PATH` set per-shell
(not persistent on most dev machines — see repo `CLAUDE.md`'s "Commands" section and, if
present, this machine's own build-env memory notes). **Never build a release binary with a bare
`cargo build -p desktop`** — it embeds the Vite dev-server URL instead of the built frontend;
always `npm run tauri build -- --no-bundle` (or `make desktop-build`).

## Before changing UI

Read the relevant files:

- Product/domain changes: `domain/VIDEO_EDITOR.md`
- Timeline changes: `domain/TIMELINE.md` (data model) — `apps/desktop/src/timelineGeometry.ts`
  and `apps/desktop/src/editor/Timeline.tsx` for the real canvas implementation
- Workflow changes: `ux/WORKFLOWS.md` (workflow shape only, see the note above on stale paths)
- UI changes: `design/DESIGN_SYSTEM.md`, `design/PROFESSIONAL_UI.md`
- Architecture changes: `apps/desktop/src-tauri/src/state.rs` and `commands/mod.rs` (the real
  session/command/undo plumbing) — `engineering/ARCHITECTURE.md` is egui-era history only
- Quality gates before calling a change done: `engineering/QUALITY.md` (still applies at the
  principle level; ignore any egui-specific command/file paths in it)

Do not implement a feature from a vague request without identifying:
- the user goal;
- the active object (a `ClipInstance`? a `TextClip`/`ShapeClip`? a `Marker`? the whole
  `Sequence`?);
- the current context (`uiStore.tool`, `uiStore.selectedClipIds`, which screen is mounted);
- the primary interaction;
- the expected state transition;
- undo/redo semantics — remember undo here is **full-`Sequence`-snapshot** (`commands::mod::
  undoable` pushes one snapshot per command, not per-field);
- performance implications, especially inside `Timeline.tsx`'s per-frame `draw()`/`rAF` loop or
  anything called from `preview_engine.rs`'s `run()` loop.

## Design before code

For non-trivial UI changes, first produce a short internal plan covering:
1. user workflow;
2. affected objects and state (which `apps/desktop/src/store.ts` slice, and whether anything
   needs a new field on `Project`/`Timeline`/`ClipInstance` in `avcore`, plus the matching
   `#[tauri::command]` and its `ipc.ts` wrapper);
3. layout and interaction — does it belong in `editor/Timeline.tsx` (canvas-painted), an
   `editor/` component (standard React/DOM layout), or a new backend `commands/<feature>.rs`
   (state + command logic, separate from the React side);
4. architecture — reuse existing primitives (`avcore::timeline::Track::ripple_delete_range`,
   `commands::mod::undoable`/`with_session`, the `PreviewEngine`/`Command` channel pattern, the
   `Section`/`F32KeyframeList` reusable React components) before inventing new ones;
5. performance risks — `Timeline.tsx`'s viewport culling skips off-screen clips, but new
   per-clip work added *before* that check still pays full per-clip cost every frame;
6. validation — which tests (Rust unit/integration, `vitest`, `e2e/` WebDriver), which quality
   gates.

Then implement.

## Non-negotiable UI principles

- Do not design a generic SaaS dashboard.
- Do not overuse cards, rounded containers, shadows, borders, or decorative gradients.
- The editing workspace is more important than application chrome.
- Prefer direct manipulation over modal workflows — batch-review flows (silence review,
  transcript-proposal review) are a deliberate, narrow exception where a reviewable list of
  automatic suggestions genuinely needs a modal; that's not a default pattern to reach for.
- Use progressive disclosure — `Inspector.tsx`'s `Section` component (`<details>`/`<summary>`)
  is the existing collapsible-property-group primitive.
- Keep contextual tools contextual — the properties panel (`Inspector.tsx`/`TextInspector.tsx`/
  `ShapeInspector.tsx`) is already context-aware on the selected clip type; extend that, don't
  bypass it.
- Preserve muscle memory and keyboard efficiency — shortcuts are handled in
  `screens/Editor.tsx`'s global keydown handler before delegating further; check there first.
- Do not invent a new visual pattern when a project component already exists (`Section`,
  `F32KeyframeList`/`PositionKeyframeList`, the flat icon-button convention in
  `SequenceTabs.tsx`/`Timeline.tsx`'s toolbar).
- Do not expose every feature permanently.

## Rust/frontend principles

- Avoid unnecessary clones and allocations in hot paths — `preview_engine.rs`'s `run()` loop and
  `Timeline.tsx`'s `draw()`/pointer-move handlers are the two per-frame call sites; match the
  discipline already there (e.g. `resolve_layers` borrows from `media`/`timeline` rather than
  cloning) in new per-frame code.
- Do not block the UI/webview thread — dispatch anything FFmpeg/Whisper/network-bound
  through a dedicated `std::thread::spawn` + a single-flight `AppState` guard + a Tauri event
  the frontend listens for, matching every existing AI-feature command
  (`transcribing_asset_id`, `scene_cut_detection_clip_id`, etc. on `AppState`) — never
  `tokio::spawn` (only `tokio`'s `sync` feature is enabled, no runtime is present).
- Separate persistent project state (`Project`/`Sequence`/`Timeline`/`ClipInstance` in `avcore`,
  owned by the backend's `Session`) from transient UI state (`uiStore` in `store.ts`) — selection
  (`uiStore.selectedClipIds`) is the confirmed real example: frontend-only, never serialized,
  passed explicitly as a command parameter when a backend command needs it (the backend holds no
  selection state of its own).
- Use `commands::mod::undoable` for undoable edits — one call per user-visible command, the
  closure returns whether anything changed so a rejected edit leaves no empty undo step.
- Keep rendering, interaction, and domain logic separate — `Timeline.tsx`'s own convention (pure
  geometry/hit-test functions in `timelineGeometry.ts`, called from the component's event
  handlers and `draw()`) is the pattern to extend, not a bespoke one.
- Prefer small cohesive modules over giant files — `commands/` and `editor/` are already one
  file per feature area; add a new one for a new feature rather than growing an existing file.
- Run formatting, linting and relevant tests after changes (`make fmt`, `make lint`,
  `make test-core`, `cd apps/desktop && npm run typecheck && npx vitest run`, `cargo test -p
  desktop` — the last one crashes on a known, non-blocking debug-profile-only CRT issue, see
  `spec/architecture/tauri-migration.md`; `cargo check -p desktop` is the reliable substitute).

## Done criteria

A feature is not done until:
- behavior is correct;
- UI states are handled;
- keyboard/mouse interaction is coherent;
- undo/redo is considered where applicable (remember: full-sequence snapshot via `undoable`, not
  per-field);
- performance impact is assessed, especially anything touching `Timeline.tsx`'s per-frame `draw()`
  or `preview_engine.rs`'s `run()` loop;
- tests are added or updated where appropriate (`desktop`'s Rust unit tests live in
  `src/<module>/<module>_test.rs`, frontend tests are `vitest` (`*.test.ts`/`*.test.tsx`), `core`'s
  integration tests in `crates/core/tests/`, and a real user-facing flow gets an `e2e/` WebDriver
  test against the compiled binary);
- no obvious visual regressions remain.
