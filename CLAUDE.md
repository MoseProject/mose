# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

oca — a native Rust video editor for cutting gameplay footage for the PacoPaçoca YouTube
channel, shipped as a Tauri 2 desktop app (React/TypeScript webview over a Rust backend).
Headline features: automatic loudness normalization and export-that-matches-the-source-bitrate.
Full original phased plan: [`features/request.md`](features/request.md) — written against the
project's first shell, a since-retired `egui`/`eframe` GUI (`crates/ui`, deleted once the Tauri
shell reached parity; see [`spec/architecture/tauri-migration.md`](spec/architecture/tauri-migration.md)
for the migration's own history and the deletion decision).

## Status

Fase 1-8 of the original phased plan (`features/request.md`) are done — probe/export/loudness/
proxy, timeline editing, effects/keyframes/AI features, export queue, robustness, performance
work, and packaging/auto-update all shipped, originally against the egui shell. **Detailed
status now lives in `spec/`, not here** — start at [`spec/INDEX.md`](spec/INDEX.md), then
[`spec/ROADMAP.md`](spec/ROADMAP.md) to pick up the next task. `spec/matrix/*.md` are
done/needed checklists per area (engine, timeline, effects, AI features, preview pipeline,
robustness, performance, packaging); `spec/matrix/changelog.md` is the old narrative status log
(verification caveats, real bugs found, empirical discoveries) moved here verbatim, written
against the egui shell — features it describes now live in `apps/desktop`/`crates/core` instead;
`spec/architecture/*.md` covers the newly-proposed differentiator features and the
performance/caching patterns ported from nimble (sibling project). Check `features/request.md`
for the original plan's own wording before assuming a Fase-4+ feature is live — when in doubt,
`graphify query`.

## Commands

Requires Rust (stable) via rustup. `avbridge` needs `FFMPEG_DIR` set to an FFmpeg dev
build (`include/`+`lib/`); its DLLs must be on `PATH` at runtime. There is no GStreamer
dependency anymore: the preview is `bush`, and the only extra system library is the ALSA
development package on Linux (`libasound2-dev`, for `cpal`'s audio output). `core`'s
`whisper-rs` dependency needs `LIBCLANG_PATH` pointing at a libclang install (bindgen) and a
build directory that isn't under `%TEMP%` on Windows (MSVC's FileTracker fails there —
`FTK1011`).

**Ubuntu 24.04's packaged FFmpeg (6.1.1, via `apt install libavformat-dev` etc.) is too old
to build `avbridge`.** `csrc/filters.c` calls `avcodec_get_supported_config`/
`AV_CODEC_CONFIG_SAMPLE_FORMAT`/`AV_CODEC_CONFIG_SAMPLE_RATE`, added in FFmpeg 7.1 — a plain
`apt`-installed FFmpeg dev build on noble fails with "undeclared identifier" on that file
specifically, before reaching any of this project's own logic. A sandboxed session without
network access to fetch a newer FFmpeg build (or without access to `cdn.pyke.io` for `ort`'s
prebuilt ONNX Runtime binaries — set `ORT_SKIP_DOWNLOAD=1` to defer *that* failure past `cargo
check`, same idea as the GPU-encoder note below) can't get a full `cargo check`/`build`/`test`
to pass — same category of pre-existing, machine-specific build gap as the GPU encoder note
below, not a code regression. `cargo fmt --check` still works (parses each file independently,
no dependency build needed) and is a reasonable sanity check when a full build isn't possible.
For `avbridge/csrc/*.c` changes specifically, `gcc -fsyntax-only -I <ffmpeg-include-dir>
-Wall -Wextra <file>.c` run per-file from `crates/avbridge/csrc/` is a real (if partial)
compiler check that still works even when the whole crate can't build — it only needs the
headers *that file* includes, so it stays clean for every file except `filters.c` itself even
on Ubuntu's too-old packaged FFmpeg. It won't catch link-time or runtime/filtergraph-semantic
issues, but it does catch real syntax/type errors no amount of manual review guarantees.

**One of this gap's usual co-blockers is actually fixable in a sandboxed session, worth trying
before assuming a full build is out of reach**: a too-old bundled `rustc` (this repo's
`Cargo.lock` can need a newer one than a session starts with) — try `rustup update stable` first,
it may just work with no network restrictions beyond crates.io/static.rust-lang.org. It does not
fix the FFmpeg-too-old `filters.c` gap itself, but it gets a `cargo check -p core` past every
*other* failure first, so the one true remaining blocker is confirmed in isolation rather than
assumed.
A pure-logic module with no `avbridge` dependency can still be fully verified despite
all of this: copy just that file (plus its `#[cfg(test)]` module) into a throwaway scratch crate
with no dependencies and `cargo test` it there — real execution, not just a parse check.

Going one step further: `cargo check` (type/borrow-check, no linking) for the *entire* workspace
can be made to pass even with the FFmpeg-too-old `filters.c` gap in place, by locally (never
committed) `#define`-shimming its two missing 7.1 symbols to dummy no-op implementations just so
`cc` accepts the file; `git checkout -- crates/avbridge/csrc/filters.c` throws the shim away
afterward with zero trace. This validates every type, borrow, and trait bound across a whole
session's Rust changes — far stronger than `cargo fmt --check` alone — while still being honest
that it's not a real build: the shimmed function bodies are wrong (a real filters.c fix needs a
genuine FFmpeg-version-aware implementation, not this), and `cargo test`'s final link step
separately needs `libonnxruntime` (`ort`'s `download-binaries` feature needs `cdn.pyke.io`,
blocked in this sandbox same as before) — so tests still can't *run* here, only type-check.

`avbridge/build.rs` copies FFmpeg's Windows import libs into `OUT_DIR` under unique names. That
step existed because GStreamer's SDK bundled its own FFmpeg (gst-libav) with identically named
import libs; GStreamer is gone, and the step is kept because it is harmless and keeps the link
independent of whatever else is on the linker search path.

```bash
make build     # debug build, whole workspace (core, avbridge, ytbridge, desktop, xtask)
make test      # run every crate's test suite
make test-core # cargo test -p core only
make desktop-dev   # Vite + Tauri dev server with hot reload -- the app itself
make test-desktop  # desktop frontend typecheck + unit tests, then cargo test -p desktop
make test-e2e  # pytest + selenium against a built target/release/oca-desktop.exe
make bench     # criterion benchmarks -> target/criterion/report/index.html
make fmt       # cargo fmt --all
make lint      # cargo clippy --workspace --all-targets
make check     # cargo check --workspace --all-targets (fast compile-only loop)
```

Single test: `cargo test -p core --test probe_test measure_loudness` or
`cd apps/desktop && npx vitest run`.

**Git hooks**: `npm install` (Node, one-time) sets up a husky pre-commit hook that runs
`rustfmt`/`clang-format` on staged `.rs`/`.c`/`.h` files via `lint-staged` — see `package.json`
and `.clang-format`. Skipping `npm install` just means no hook runs; `make fmt`/`cargo fmt`
still work standalone.

**End-to-end tests** (`e2e/`) drive the real `oca-desktop.exe` via WebDriver
(`pytest` + `selenium`, through `tauri-driver`'s WebView2/msedgedriver bridge). One-time setup:
`cargo install tauri-driver` and `python -m pip install -r e2e/requirements.txt`. **Build via
the Tauri CLI, never plain `cargo build`**: `cd apps/desktop && npm run tauri build --
--no-bundle` (or `make desktop-build`) — a binary from a bare `cargo build -p desktop` (even
`--release`) embeds `devUrl` instead of the built frontend and shows a WebView2 error page with
no dev server running; this was a real ship-blocking bug in `release.yml` until diagnosed (see
`spec/architecture/tauri-migration.md` gap 7). `OCA_DESKTOP_EXE` (`e2e/conftest.py`) picks which
built binary to launch, `TAURI_NATIVE_DRIVER` points at a specific `msedgedriver.exe` when
needed, and both plus the FFmpeg DLL dir must be on `PATH` for the launched process. All commands are invoked directly through
`window.__TAURI_INTERNALS__.invoke` (`e2e/helpers.py`) rather than fighting native OS dialogs
WebDriver can't reach. **Use a real Python, not a Windows Store alias** — a Store-packaged
`python.exe` carries MSIX package identity that breaks DLL loading for spawned child processes
even with `PATH` set correctly, while the identical binary launched directly from PowerShell
works fine.

## Architecture

Dependency direction: `avbridge` → `bush` → `core` → `desktop` (`apps/desktop`), plus an isolated
`ytbridge`. `bush` is the native preview engine (decode, composite, audio) and depends only on
`avbridge`; `core` uses its decoder for frame sampling and `desktop` runs its playback loop —
see [`spec/matrix/bush.md`](spec/matrix/bush.md); `make test-bush`.

- **`avbridge`** — thin C bridge (`csrc/bridge.c`) over libavformat/libavcodec/libavfilter/
  libavutil, called via FFI. Exposes probing, timeline export rendering, loudness
  measurement, proxy generation, and waveform extraction.
- **`core`** — package name `core`, `[lib] name = "avcore"` (Rust code imports it as `avcore`,
  never `core`, since that would shadow libcore). Project/timeline/media data model plus
  wrappers (`probe`, `render`, `loudness`, `proxy`, `waveform`), a frame sampler over Bush's
  decoder (`frame_sampler`), and `.ocproj` save/load (`persistence`) — gzip-compressed
  MessagePack (struct-map mode, so `#[serde(default)]` still lets an older-saved project load
  after a new field is added). Locale-neutral — stores enums, never pre-formatted strings. No
  mock/sample data anywhere.
- **`apps/desktop`** — the app itself: a Tauri 2 shell, React/TypeScript webview (`src/`, Vite)
  over a Rust crate `desktop` (`src-tauri/`, bin `oca-desktop`) that owns the session, the Bush
  preview (`bush_adapter/`: timeline-to-scene mapper and playback loop), import and export via
  `core`. Webview talks to Rust only through `src/ipc.ts`. `tokio`
  is used with only the `sync` feature (channel primitives, not a task runtime — background work
  is plain `std::thread::spawn`). Design history, decisions, and the now-closed migration gaps
  that got it here: [`spec/architecture/tauri-migration.md`](spec/architecture/tauri-migration.md).
  `make desktop-dev` / `make test-desktop`.
- **`ytbridge`** — standalone binary, deliberately isolated from `core`/`desktop`'s dependency
  graph; vendors its own Python runtime via `pyo3` for yt-dlp.

Test placement: `core` and `avbridge` have `[lib]` targets → integration tests in
`crates/<crate>/tests/`. `desktop` (`apps/desktop/src-tauri`) is bin-only → unit tests in
`src/<module>/<module>_test.rs`.

## Approach

- Read existing files before writing. Don't re-read unless changed.
- Thorough in reasoning, concise in output.
- Skip files over 100KB unless required.
- No sycophantic openers or closing fluff. No emojis or em-dashes.
- Do not guess APIs, versions, flags, or package names. Verify by reading code or docs.
- Write all code and commit messages in English.
- Commit using Conventional Commits format (`feat:`, `fix:`, `refactor:`, etc.).
- Keep commits small and split by crate/layer — never bundle `avbridge` (C/FFI), `core`,
  `desktop` (Rust command + React frontend), and test changes for one feature into a single
  commit. Commit each layer separately, in dependency order (`avbridge` → `core` → `desktop`
  backend → `desktop` frontend → tests), even when they land in the same session for the same
  feature. A commit that only adds/changes tests for already-committed code gets its own
  `test:`-prefixed commit rather than being folded into the `feat:` commit it covers.

## graphify

This project has a knowledge graph at `graphify-out/` with god nodes, community structure,
and cross-file relationships.

- For codebase questions, first run `graphify query "<question>"` when `graphify-out/graph.json`
  exists. Use `graphify path "<A>" "<B>"` for relationships and `graphify explain "<concept>"`
  for focused concepts.
- If `graphify-out/wiki/index.md` exists, use it for broad navigation.
- Read `graphify-out/GRAPH_REPORT.md` only for broad architecture review or when
  query/path/explain don't surface enough context.
- After modifying code, run `graphify update .` to keep the graph current.
