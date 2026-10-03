# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## What this is

oca — a native Rust video editor (`egui`/`eframe`) for cutting gameplay footage for the
PacoPaçoca YouTube channel. Headline features: automatic loudness normalization and
export-that-matches-the-source-bitrate. Full phased plan: [`features/request.md`](features/request.md).

## Status

Fase 1-8 of the original phased plan are done — probe/export/loudness/proxy, timeline editing,
effects/keyframes/AI features, export queue, robustness, performance work, and packaging/
auto-update all shipped. **Detailed status now lives in `spec/`, not here** — start at
[`spec/INDEX.md`](spec/INDEX.md), then [`spec/ROADMAP.md`](spec/ROADMAP.md) to pick up the next
task. `spec/matrix/*.md` are done/needed checklists per area; `spec/matrix/changelog.md` is the
old narrative status log (verification caveats, real bugs found) moved there verbatim;
`spec/architecture/*.md` covers the proposed differentiator features and the performance/
caching patterns ported from nimble (sibling project). Check `features/request.md` for the
original plan's own wording before assuming a Fase-4+ feature is live.

## Commands

Requires Rust (stable) via rustup. Both the MSVC target (`stable-x86_64-pc-windows-msvc`, needs
Visual Studio Build Tools with the "Desktop development with C++" workload) and the GNU target
(`x86_64-pc-windows-gnu` + a MinGW-w64 toolchain e.g. WinLibs, `bin` dir on `PATH`) work — this
workspace currently defaults to MSVC, but nothing in it requires MSVC specifically.

`avbridge` links against libavformat/libavcodec/libavfilter/libswscale/libavutil and needs
`FFMPEG_DIR` set to an FFmpeg dev build with `include/` and `lib/` subdirectories (e.g. a BtbN
shared build, with `avfilter`/`swscale`/`libopenh264` among the linked libs/enabled encoders)
to compile at all — the workspace won't build without it. At runtime the FFmpeg DLLs
(`FFMPEG_DIR/bin`) need to be next to the built binary or on `PATH`.

There is no GStreamer dependency anymore: the preview is the `bush` crate. On Linux the desktop
crate also needs the ALSA development package (`libasound2-dev`) for `cpal`'s audio output.

`avbridge/build.rs` copies FFmpeg's Windows import libs into its own `OUT_DIR` under unique
names before linking. The step was added because GStreamer's SDK bundled its own FFmpeg build
(`gst-libav`) whose import libs shared filenames with a different, incompatible FFmpeg version;
a bare-name search could silently resolve to the wrong copy and corrupt `avbridge`'s FFmpeg calls
at runtime (full diagnosis in `features/fase1/commit_plan.md`, chore-002). GStreamer is gone;
the step is kept because it is harmless and keeps the link independent of the linker search path.

```bash
make build     # debug build, whole workspace
make run       # run the GUI app (debug)
make test      # run every crate's test suite
make test-core # cargo test -p core only
make test-e2e  # pytest + pywinauto against a built target/debug/ui.exe (see below)
make bench     # criterion benchmarks (core parsing/serialization) -> target/criterion/report/index.html
make fmt       # cargo fmt --all
make lint      # cargo clippy --workspace --all-targets
make check     # cargo check --workspace --all-targets (fast compile-only loop)
```

No `make` on PATH → run the underlying `cargo`/`rustup` command directly; every Makefile target
is a one-liner (see [`Makefile`](Makefile)).

Single test: `cargo test -p core --test probe_test measure_loudness` (integration tests, one
`<module>_test.rs` file per module under `crates/core/tests/`) or `cargo test -p ui i18n::tests`
for the `src/<module>/<module>_test.rs` unit tests in `ui`.

**End-to-end tests** (`e2e/`) drive the real built `ui.exe` through Windows UI Automation via
`pytest` + `pywinauto` (`backend="uia"`) — a different layer from the Rust unit/integration
tests above: those exercise `OcaApp`'s methods directly, these click on the actual rendered
window the way a user would, so they're what actually proves a change *works*, not just that
it compiles and its logic is correct in isolation. Works because `ui/Cargo.toml` builds
`eframe` with the `accesskit` feature — egui's own widgets (`ui.button`/`ui.label`/...)
publish an accessible name/role for free. Hand-painted controls that skip egui's widget API
don't get this automatically — `nav_rail.rs`'s `rail_button` was patched to call
`response.widget_info(...)` explicitly so the nav rail is reachable by name at all; other
custom-painted controls (media library items, timeline clips) aren't yet, so e2e coverage for
those still needs coordinate-based pywinauto calls, or the same `widget_info` treatment first.
One-time setup: `python -m pip install -r e2e/requirements.txt`. Needs a debug build first
(`make build`) and the FFmpeg runtime DLL dir on `PATH` for the *launched process*
(same requirement as running `ui.exe` by hand) — running `pytest` directly under `e2e/` needs
it exported first. The `oca_window` fixture
(`e2e/conftest.py`) launches its own `ui.exe`, waits for the main window, and calls
`set_focus()` on it before yielding — without that, synthetic clicks land on whatever window
actually has focus (often the terminal `pytest` ran from) and silently do nothing, no
exception raised, the kind of failure that looks like a UI bug but isn't one.

## Architecture

Three-crate split, enforced by dependency direction: `avbridge` has no dependents within
the workspace besides `core`; `core` is UI-agnostic (no `egui` dependency at all)
and `ui` is its only consumer.

- **`avbridge`** — thin C bridge (`csrc/bridge.c`) over libavformat/libavcodec/libavfilter/
  libavutil, called from Rust via FFI (`src/lib.rs`). Written for this project, not an
  auto-generated binding of the full FFmpeg API — keeps the C surface small and auditable.
  `build.rs` locates FFmpeg via `FFMPEG_DIR` (see Commands above). Exposes probing
  (`avbridge_probe`), export rendering (`avbridge_encode_export`: video
  passthrough-copied, audio decoded → loudnorm+limiter filter graph → AAC re-encoded, with
  progress callback and cooperative cancellation), loudness-only measurement
  (`avbridge_measure_loudness`, same decode→loudnorm shape without the encoder — its
  report has no queryable struct API, only `av_log` output during filter-graph teardown, so
  this installs a process-global log callback for the call's duration; **not thread-safe**,
  documented on the function), proxy generation (`avbridge_generate_proxy`: video
  decoded → libswscale downscale (aspect-preserving) → `libopenh264` re-encode, audio decoded
  → format-matched → AAC re-encode), and waveform peak extraction (`avbridge_generate_waveform`:
  audio decoded → downmixed to mono float → min/max amplitude accumulated into a fixed number
  of sample-index buckets, nothing written or re-encoded).
- **`core`** — project/timeline/media data model plus the media wrappers that populate
  it (`probe`, `render`, `loudness`, `proxy`, and `waveform` — all via `avbridge` FFI, no
  subprocess left in any of them), a frame sampler over the `bush` decoder (`frame_sampler`:
  the frame at a time, for the analysis features), `.ocproj` save/load (`persistence`),
  and mock sample data (`sample`) used to exercise the UI before real files
  are wired in. Locale-neutral by design: it stores data like `Recency` (an enum), never
  pre-formatted display strings — formatting is `ui`'s job.
- **`ui`** — the eframe/egui GUI (glow/OpenGL backend): `app.rs` holds all top-level
  state (`OcaApp`, which screen is active, loaded projects, export jobs) and mutation methods
  (`open_project`, `queue_export`, etc.); `screens/` has one module per of the five screens
  (home, library, editor, queue, prefs) plus shared `widgets`; `theme.rs` is the dark/teal
  palette; **all UI strings live in `i18n.rs`** (pt-BR and English) — never hardcode display
  text in a screen module, add a `Text` variant instead.

`docs/CODE_GRAPH.md` (module dependency graph + public-API index) used to be regenerated by an
`xtask` crate (`cargo run -p xtask -- graph` / `make graph`); that crate was dropped and the
Makefile/docs referencing it are stale (`make graph`, `make test-xtask` — not fixed as part of
this pass). Use `graphify` instead — see the `## graphify` section below.

Test placement follows what's reachable: `core` has a `[lib]` target, so its tests are
real integration tests in `crates/core/tests/` (one `<module>_test.rs` file per module)
exercising only the public API, plus `crates/core/benches/` for criterion benchmarks. A
handful of tests that need a private helper unreachable from `tests/` stay as
`src/<module>/<module>_test.rs` unit tests instead (wired in via `#[path = "..."] mod
tests;`, since the module is still named `tests`). `ui` is a bin-only crate (no `[lib]`), so
all its tests are `src/<module>/<module>_test.rs` unit tests. `avbridge` has a `[lib]`
target, so its FFI tests live in `crates/avbridge/tests/` (also `<module>_test.rs`) against
small checked-in media fixtures.

Planned architecture (not yet implemented, see the plan doc for phases): extracting `preview`'s
decoded frames into an egui texture and wiring that into the Editor screen's playhead, and a
background export queue where rendering runs on a worker communicating over `tokio::mpsc` so
the editing UI never blocks on an in-progress export — each queued job snapshots its render
config at enqueue time, so later edits to the active project don't affect jobs already in the
queue.

## Approach

- Read existing files before writing. Don't re-read unless changed.
- Thorough in reasoning, concise in output.
- Skip files over 100KB unless required.
- No sycophantic openers or closing fluff.
- No emojis or em-dashes.
- Do not guess APIs, versions, flags, commit SHAs, or package names. Verify by reading code or docs before asserting.
- Write all code and commit messages in English.
- Commit using Conventional Commits format (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`, etc.), always in English.

## On-demand code quality references

Use [`docs/code-quality/README.md`](docs/code-quality/README.md) as a router. Do not preload every
quality document. Load only the guide whose trigger matches the current task, and load each selected
guide once per task:

- behavior, refactor, architecture, or code review: `design-principles.md`
- Rust code or Rust-facing APIs: `rust.md`
- hot paths, media processing, caches, concurrency, I/O, or optimization: `performance.md`
- behavior or test changes: `testing.md`
- untrusted input, paths, downloads, dependencies, secrets, or FFI: `security.md`
- patch/PR review: `review-checklist.md`, followed only by the applicable topical guides

Skip this library for documentation-only and mechanical changes unless the changed document defines
one of these contracts.

## graphify

This project has a knowledge graph at graphify-out/ with god nodes, community structure, and cross-file relationships.

Rules:
- For codebase questions, first run `graphify query "<question>"` when graphify-out/graph.json exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"` for focused concepts. These return a scoped subgraph, usually much smaller than GRAPH_REPORT.md or raw grep output.
- If graphify-out/wiki/index.md exists, use it for broad navigation instead of raw source browsing.
- Read graphify-out/GRAPH_REPORT.md only for broad architecture review or when query/path/explain do not surface enough context.
- After modifying code, run `graphify update .` to keep the graph current (AST-only, no API cost).
