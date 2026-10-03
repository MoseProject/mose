# Architecture

Ground truth against the workspace `Cargo.toml`, `crates/*/Cargo.toml`, and
`crates/ui/src/app/mod.rs`.

## Crates and dependency direction

`avbridge` (FFmpeg FFI) → `core` (package `core`, `[lib] name = "avcore"` — always imported as
`avcore`) → `ui` (eframe/egui). One-way, enforced by Cargo dependency edges, not just convention.
A fourth crate, `ytbridge` (standalone yt-dlp downloader via `pyo3`), is deliberately isolated
from this chain — do not add a dependency from `core`/`ui` onto it or vice versa beyond
process-level invocation.

## Layers, as actually implemented

- **Domain** (`core`, package `avcore`) — `Project`/`Sequence`/`Timeline`/`Track`/`ClipInstance`/
  `MediaAsset`, plus one module per analysis/AI feature. Locale-neutral by convention — stores
  enums, never pre-formatted strings (UI text lives entirely in `ui::i18n`).
- **Application services** — split across `core` (blocking APIs: `probe_media`, `render`,
  `loudness`, `proxy`, `preview::Preview`) and `ui/src/app/*.rs` (thread-spawning orchestration
  around those blocking calls — see Async below). There is no separate "application services"
  crate; the boundary is a convention (blocking logic in `core`, thread/channel orchestration in
  `ui`), not a compiler-enforced one.
- **UI** — `ui/src/app/` (state + per-feature `impl App` business logic, ~30 files, no drawing)
  and `ui/src/screens/` (drawing, dispatch to `app/` methods). This split is real and load-bearing
  — a new feature typically needs both an `app/<feature>.rs` (state, mutation methods, undo) and
  either a `screens/` addition or a call from an existing screen.
- **Rendering/cache** — thumbnail texture cache (`App.thumbnail_state`), waveform peaks
  (persisted on `MediaAsset`, no separate cache), value-equality caches (`ExportPreviewCache`,
  `NestedSequenceCache`) — see below.

## Dependency direction in practice

`screens/` calls into `app/` methods on `&mut App`; `app/` calls into `avcore` (blocking) or
spawns a thread that calls `avcore` then reports back over a channel. `avcore` never depends on
`ui` or knows about egui. This is the real, enforced version of "UI should not become the owner
of core editing rules" — mutation primitives (`Track::ripple_delete_range`,
`Track::split_clip_at`) live in `avcore`, `app/` orchestrates and adds undo snapshots around them.

## Undo/redo — full-snapshot, not command-pattern

`crates/core/src/undo.rs`: `UndoStack { capacity, undo: Vec<Sequence>, redo: Vec<Sequence> }`.
Scoped to **one `Sequence`**, not the whole `Project`; cleared on sequence switch. This is an
explicit, documented design choice (see the module's own doc comment), not a placeholder to
"eventually" replace with commands. `App::push_undo_snapshot()` clones the current sequence
before a mutation — call it once per user-visible commit. `App::push_undo_snapshot_for_drag()`
coalesces a continuous drag/slider gesture into one push. Do not design a feature that assumes
finer-grained (per-field) undo exists.

## Async work — threads + mpsc, not tokio tasks

`ui`'s `tokio` dependency enables **only the `sync` feature** — there is no tokio runtime, no
`tokio::spawn`. The real pattern, consistent across `app/transcribe.rs`, `youtube_download.rs`,
`motion_tracking.rs`, `scene_detection.rs`, `background_removal.rs`, `import.rs`, `export.rs`,
and others:

1. A `*State` struct (e.g. `TranscribeState`) holds a
   `tokio::sync::mpsc::UnboundedSender<XEvent>`/`UnboundedReceiver<XEvent>` pair plus any
   in-flight job bookkeeping.
2. `spawn_x(...)` clones the sender and calls `std::thread::spawn(move || { ...blocking avcore
   call...; tx.send(XEvent::Done { .. }) })`.
3. `pump_x(&mut self)` is called once per frame from `eframe::App::ui` (there are ~12 `pump_*`
   calls listed at the top of `ui()`), drains `rx.try_recv()`, and applies results to `App` state
   (toasts, mutating the project, closing a modal).

Reuse this pattern for any new FFmpeg/Whisper/network-bound work. Do not block inside
`update()`/`ui()`, and do not reach for `tokio::spawn` — it will not compile without adding the
`rt` feature, which would be a real, deliberate dependency change, not a drop-in fix.

## Caching convention — value-equality, not version counters

`ExportPreviewCache` (`ui/src/app/export.rs`) and `NestedSequenceCache`
(`core/src/nested_sequence.rs`) both invalidate by `#[derive(PartialEq)]` value comparison against
the relevant domain data (a `Sequence`, a set of `Track`s), not a dirty flag or version counter.
Every domain type involved already derives `PartialEq`, which is what makes this cheap to check.
Follow this convention for new caches rather than introducing a version-counter/dirty-flag scheme
— it would be an inconsistent pattern in an otherwise-consistent codebase.

## Persistence framing

Shared magic-bytes + version + gzip-MessagePack (struct-map mode) framing
(`to_framed_bytes`/`from_framed_bytes` in `persistence.rs`), reused by `.ocproj` (`OCPJ`),
`.ocqueue` (`OCQU`), and `.octr` (`OCTR`, transcript sidecars). A new persisted format should
reuse this, not invent its own byte layout. Struct-map mode plus `#[serde(default)]` is the real
forward-compatibility mechanism — a new field on a persisted struct must be `#[serde(default)]`.
