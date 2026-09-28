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

1. **Preview fidelity**: LUT, vignette, glitch, and per-clip deflicker now run after decode
   before JPEG encoding. The existing live pipeline properties (color balance, blur/sharpen,
   chroma key parameters, crop, pixelize, shake, and an already-instantiated mask) update without
   reopening; decoder-shape changes still reopen. Word-highlight text overlays now advance during
   playback and seeks. Hardware decode defaults on and can be changed through the preview command.
   Compound clips are still skipped in preview.
2. **Done:** roll/slip/slide tools (timeline toolbar tool switcher, repurposing edge/body drags
   into `avcore::Timeline`'s `Track::roll_edit`/`ClipInstance::slip`/`Track::slide_clip`);
   composite-block body drag (`Timeline::move_clip_with_group`) and "Mesclar em bloco composto";
   `Ctrl+C`/`X`/`V` and `Ctrl+Shift+C`/`V` clip/formatting clipboard
   (`Timeline::copy_clip_group`/`paste_clip_group`, `ClipInstance::formatting`/
   `apply_formatting`), transient per-session (not per-project) same as egui's
   `App::clipboard_clip`; layer templates (Templates popup: apply/delete saved templates, Save
   as template for >=2 selected clips), sharing `prefs.oc`'s `saved_layer_templates` key with
   the egui shell. Multi-select itself (Ctrl+click) predates this slice. Still missing: on-canvas
   ghost rendering for roll/slip/slide drags (commands only fire on release, no live preview of
   the pending edit), a visible "has clipboard" indicator on the paste/apply buttons.
3. **Keyframes, masks, transitions, crop done** (property-panel controls, matching egui's own
   `DragValue`/`Slider`-driven approach for these -- no on-canvas crop handles or mask preview
   overlay in either shell): the inspector gets crop x/y/w/h with a reset button, mask shape
   (None/Circle/RoundedRect) with corner radius, transition type/duration, blend mode (English
   names, untranslated, same as egui), and per-property keyframe list editors (position/scale/
   rotation/opacity/gain/brightness/contrast/saturation/crop_x/y/w/h) via `update_clip`'s
   extended `ClipPatch` -- one whole-list `Vec<Keyframe<T>>` write-back per add/remove/edit,
   mirroring egui's `f32_keyframe_editor`/`position_keyframe_editor` shape. Value ranges/clamps
   mirror `crates/ui/src/app/mod.rs`'s constants exactly (`CROP_MIN_SIZE`, `MASK_CORNER_RADIUS_
   RANGE`, `TRANSITION_DURATION_RANGE`, etc). Still missing: timeline keyframe markers, on-canvas
   crop/mask handles, per-effect enable indicators the properties panel's collapsible sections
   give egui. **Text and shape clips (first part) done:** toolbar
   "Add text"/"Add shape" create a clip at the playhead on an auto-created Text/Shape track via
   `core`'s `TextClip::new`/`ShapeClip::new` and select it; overlays draw on the timeline canvas
   and move/trim/snap/delete through the same drag paths as media clips; the inspector edits
   text (content, bundled-catalog font grouped by category, Bold only where the family ships
   one, size, color, alignment, position, word highlight when the clip has word timing) and
   shapes (preset via `core`'s `ShapePreset`, color, opacity, position, size), one undo step per
   commit, with the preview reopening on the changed layer. Still missing from egui: Text tool
   (T) click-to-create on the preview, direct on-canvas drag/resize, custom shape drawing and
   vertex editing, font weight (variable fonts), text direction, background box, rotation,
   stroke thickness, start/duration fields, per-property keyframes, split of overlay clips,
   add-empty-Text/Shape-track buttons.
4. Export queue parity: persistent `.ocqueue` snapshots, bounded worker pool, pause/resume/cancel/remove, loudness matching, GPU encoder preference, output conflict choices, and SRT export are implemented. The dedicated export panel still needs visual/e2e coverage.
5. AI features. **Transcription/transcript panel done:** the media library's per-asset "📝"
   button runs Whisper on a background thread (`avcore::transcribe`, bundled Whisper Base model
   or `prefs.whisper_model_path`, shared with the egui shell), creating word-highlight `TextClip`
   subtitles on an auto-created Text track and saving a `TranscriptDocument` `.octr` sidecar
   (`transcribe_asset`/`TRANSCRIBE_EVENT`). The Transcript panel (timeline toolbar) filters the
   selected clip's own transcript by default, or searches every asset with a saved transcript
   project-wide when toggled on, highlighting the word under the playhead and seeking through
   `seek_to_transcript_word` (maps a word's media-relative time back onto the first timeline clip
   using that asset, same as `ui::App::map_asset_source_to_timeline`) on click. **Silence review
   done:** the timeline toolbar's "Revisar silêncio" button scans the selected clip's track for
   waveform-derived silence gaps (`avcore::clip_silence_gaps`, cached `MediaAsset::waveform_
   peaks`, no core changes needed), lists them with a checkbox defaulting to accepted and
   click-to-seek on the timecode, and ripple-deletes whatever's still checked on Apply (one undo
   step for the batch) -- mirrors `ui::App`'s `silence_review.rs` modal, review state staying
   client-side between `detect_silence_gaps_for_clip` and `apply_silence_gaps` rather than
   server-staged. Still missing: scene detection, highlight detection, auto-reframe, background
   removal, motion tracking, privacy blur, TTS.
6. Preferences (locale switch, layout, hardware decode), autosave + crash recovery, error
   reporting consent, update check, telemetry, recent projects on Home, watch folder, YouTube
   download, sound library. **In progress:** desktop now shares `prefs.oc` with egui for locale,
   LUFS defaults, hardware decode, output folder, autosave interval and recent projects; it also
   writes/restores the shared `.autosave.ocproj` files. Layout, crash-report review, consent,
   update, telemetry, watch/download and sound-library controls remain.
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
