# Tauri migration (egui -> Tauri + React)

Status: **phase 1 landed** (2026-09-27). The egui shell (`crates/ui`) is still the shipping app;
the Tauri shell (`apps/desktop`) runs beside it and takes over screen by screen.

## Decisions

| Question | Decision | Why |
|---|---|---|
| Web framework | React + TypeScript + Vite | Largest ecosystem for the component-heavy panels; the timeline stays a hand-painted `<canvas>` like the egui one. |
| Migration strategy | New crate in parallel, egui untouched until parity | 43k lines of egui UI cannot move in one step without leaving the editor unusable for weeks. |
| Preview frames | Rust decodes, JPEG over a custom `oca://` URI scheme, `<img>` in the webview | Reuses the GStreamer pipeline and its live effects unchanged. Raw RGBA (8 MB/frame at 1080p) is too heavy for IPC; WebCodecs would duplicate decode and lose the pipeline's effects. |

## Layout

```text
apps/desktop/
├── package.json, vite.config.ts, tsconfig.json, index.html
├── src/                      React/TS webview
│   ├── ipc.ts                the ONLY module that calls invoke()/listen()
│   ├── types.ts              TS mirror of avcore's serde JSON
│   ├── store.ts              cached session + transient UI state (selection, zoom, screen)
│   ├── i18n.ts               every display string, pt-BR + en
│   ├── timelineGeometry.ts   pure timeline math (+ .test.ts, vitest)
│   ├── screens/              Home, Editor
│   └── editor/               MediaLibrary, PreviewPanel, Timeline (canvas), Inspector
└── src-tauri/                Rust crate `desktop` (bin `oca-desktop`), workspace member
    └── src/
        ├── state.rs          Session (Project + UndoStack), publish()
        ├── commands/         #[tauri::command] surface (project, media, timeline, transport, export)
        ├── preview_engine.rs playback coordinator thread driving avcore::preview_worker
        ├── protocol.rs       oca:// scheme: /frame/<seq>, /thumb/<asset_id>
        ├── frames.rs         RGBA -> downscaled JPEG
        └── export.rs         segment snapshot + render thread + progress events
```

Dependency direction is unchanged: `avbridge -> core -> {ui, desktop}`. Anything both shells
need moves into `core` rather than being copied. Phase 1 moved:

- `ui::app::preview_worker` -> `avcore::preview_worker` (plus `clip_seek_offset`).
- `default_clip_instance` -> `ClipInstance::new`.
- `create_new_track` / `resolve_or_create_track` / `next_clip_id` -> `Timeline` methods.

## Data flow

1. The webview never owns project state. Every edit is a command; the backend mutates
   `Session`, then `publish()` emits `session-changed` with the whole `SessionView` and hands
   the timeline to the preview engine. Undo stays **full-`Sequence` snapshot**
   (`commands::undoable`), one per user-visible commit; drags and sliders preview locally and
   commit on release.
2. `preview_engine` resolves the clips under the playhead (`Layers`), reopens the worker when
   they change, seeks when they do not, maps the pipeline position back to timeline time while
   playing, and wall-clocks gaps/frozen clips. It emits `preview-tick` (<= 60 Hz) with a
   `frameSeq`; the webview loads `oca://.../frame/<seq>`.
3. Export resolves segments exactly like `ui::screens::queue` and renders on a `std::thread`,
   emitting `export-jobs`.

On Windows the custom scheme is served as `http://oca.localhost/...` (WebView2); elsewhere as
`oca://localhost/...` -- `ipc.ts` picks the base.

## Parity

Done in phase 1: new/open/save `.ocproj`; import (dialog + OS drag-drop) with the two-phase
probe -> loudness/proxy/waveform enrichment and proxy rehydration on open; media library with
thumbnails; timeline canvas (ruler, tracks, clips, waveforms, playhead, scrub, move across
tracks, trim, snap to edges/playhead, zoom around pointer, lock/visibility); split, delete,
ripple delete, add track; undo/redo; composited preview with play/pause/step and audio meter;
inspector for gain/speed/color balance/freeze/flip/voice cleanup; per-sequence LUFS target and
aspect ratio; sequence tabs (switch/add/duplicate/rename/remove/reorder, compound-reference
delete guard); markers (add at playhead with M, label/kind/completion, ruler draw/click, and
previous/next navigation); export with progress and cancel; keyboard shortcuts (Space, S, M, Del, Shift+Del,
arrows, Home, Ctrl+Z/Y/S/Shift+S/I).

Known gaps versus the egui shell, in migration order:

1. **Preview fidelity**: post-decode CPU effects (LUT, vignette, glitch, deflicker) and live
   pipeline updates (`set_live_*`) are not applied -- any clip change at the playhead reopens
   the pipeline instead. Word-highlight text overlays do not advance during playback
   (`update_text_overlays`). Compound clips are skipped in preview.
2. Multi-select drag, composite blocks, roll/slide/slip edits, copy/paste, layer templates.
3. Text and shape clips (creation and editing), keyframes, masks, transitions, crop.
4. Export queue persistence (`.ocqueue`), pause/resume, GPU encoder preference, SRT export.
5. AI features: transcription/transcript panel, silence review, scene detection, highlight
   detection, auto-reframe, background removal, motion tracking, privacy blur, TTS.
6. Preferences (locale switch, layout, hardware decode), autosave + crash recovery, error
   reporting consent, update check, telemetry, recent projects on Home, watch folder, YouTube
   download, sound library.
7. Packaging: replace `crates/ui` in the installer and auto-update flow, port `e2e/` from
   pywinauto to WebDriver (`tauri-driver`), then delete `crates/ui`.

Each gap migrates as its own slice: move any egui-free logic from `crates/ui/src/app/` into
`core` first (commit), then add the command(s) (commit), then the React UI (commit), then tests.

## Running

```bash
make desktop-install   # once
make desktop-dev       # Vite + Tauri with hot reload
make test-desktop      # tsc + vitest + cargo test -p desktop
```

Build prerequisites are the same as the egui shell (`FFMPEG_DIR`, GStreamer `PKG_CONFIG_PATH`,
`LIBCLANG_PATH`, FFmpeg/GStreamer DLLs on `PATH`), plus Node/npm and the WebView2 runtime.
