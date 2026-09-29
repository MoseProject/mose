# Tauri migration (egui -> Tauri + React)

Status: **feature parity is substantially complete** (2026-09-28). The egui shell (`crates/ui`)
still ships beside the Tauri shell (`apps/desktop`) until the WebDriver migration and release
bundle validation allow its retirement.

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
   RANGE`, `TRANSITION_DURATION_RANGE`, etc). **On-canvas layer transform done:** the preview
   panel (`PreviewPanel.tsx`) draws the selected clip's position/scale box directly over the
   frame, matching `layer_transform_preview`'s model exactly -- drag the box to write a single
   `position_keyframes` entry (disabled once more than one keyframe exists, same "animated,
   edit via the list" fallback egui shows), drag the bottom-right handle to scale via the new
   `layer_scale_x`/`_y` `ClipPatch` fields (not wired in desktop until now, though `avcore`
   already had them). Verified live end to end (`e2e/test_layer_transform.py`, both drag and
   resize), including a real bug the resize handle's small hit target surfaced: `.export-panel`
   floats `position: fixed` over the whole viewport at `z-index: 5` and can overlap the
   preview's bottom-right corner, making a clip positioned there ungrabbable -- fixed by giving
   `.preview-layer` `z-index: 6`. Still missing: timeline keyframe markers, on-canvas crop/mask
   handles. **Collapsible property sections done:** `Inspector.tsx`'s new `Section` component
   ports `property.rs`'s `property_section` (a plain `<details>`/`<summary>`, closed by default
   unless the clip's own values differ from default -- Gain/Speed/Color Adjust/Crop/Mask/
   Transition/Keyframes each get one). Verified live (`e2e/test_properties_panel_disclosure.py`):
   a fresh clip's Gain section starts collapsed, expands on click, collapses again on a second
   click. **Speed ramp done:** the inspector's two preset buttons (Slow -> Fast, Fast -> Slow) call
   `apply_speed_ramp_to_clip`, a direct port of `ui::App::apply_speed_ramp_to_selected_clip`
   (same split-then-reflow algorithm and `SPEED_FACTOR_RANGE` clamp) -- no Clip menu exists in
   desktop, so this lives as inspector buttons instead of a menu item, same relocation pattern
   every other egui-menu-only action already got. **Text and shape clips (first part) done:** toolbar
   "Add text"/"Add shape" create a clip at the playhead on an auto-created Text/Shape track via
   `core`'s `TextClip::new`/`ShapeClip::new` and select it; overlays draw on the timeline canvas
   and move/trim/snap/delete through the same drag paths as media clips; the inspector edits
   text (content, bundled-catalog font grouped by category, Bold only where the family ships
   one, size, color, alignment, position, word highlight when the clip has word timing) and
   shapes (preset via `core`'s `ShapePreset`, color, opacity, position, size), one undo step per
   commit, with the preview reopening on the changed layer. **Custom shape drawing done:** the
   toolbar's "Draw shape" button enters a click-to-place-vertex mode (`uiStore.
   drawingShapePoints`, purely frontend state until committed) -- each click on the preview
   canvas appends a canvas-fraction point, Enter with >=3 points calls the new
   `add_custom_shape_clip` command (a direct port of `ui::App::finish_drawing_custom_shape`'s
   bounding-box normalization into `ShapeKind::Polygon`), Escape cancels. Verified live
   (`e2e/test_custom_shape.py`). **Add-empty-Text/Shape-track buttons done:** two more toolbar
   buttons next to the existing video/audio ones, calling `add_track` with the already-supported
   `Text`/`Shape` kinds. Adding these surfaced a real pre-existing bug: `.timeline-toolbar` had
   no `flex-wrap`, so once enough buttons accumulated they silently overflowed off the right
   edge of the default 1280px window width with no way to reach them (confirmed live: `Draw
   shape` rendered at `x=1429`, `display: none` per WebDriver) -- fixed with `flex-wrap: wrap`.
   **Text tool done:** T switches to a click-to-create mode (`uiStore.tool === "text"`,
   `PreviewPanel.tsx`'s `onCanvasClick`), calling `add_text_clip` with the clicked canvas
   fraction as `pos_x`/`pos_y` (port of `ui::App::add_text_clip_at`), selecting the new clip and
   returning to the Select tool. **Custom shape vertex editing done:** `ShapeInspector.tsx`
   renders per-vertex X/Y fields for `ShapeKind::Polygon` clips (remove disabled below 3
   vertices), committed via the new `polygonVertices` patch field (`patch_shape` gives it
   priority over `shape_kind`). **Text/shape property fields done:** font weight (only shown for
   variable fonts, via new `FontOption.is_variable`), text direction, background box (color/
   padding/corner radius), shape rotation, stroke thickness, start/duration, and
   position/scale/rotation/opacity (text) or center/width/height/rotation (shape) keyframes are
   all in `TextClipPatch`/`ShapeClipPatch` now, surfaced through collapsible `Section` +
   `F32KeyframeList` blocks reused from the earlier keyframe-editor work. Verified live
   (`e2e/test_text_tool.py`, `e2e/test_overlay_properties.py`). **Confirmed NOT real gaps** (grepped
   `crates/ui`, feature absent from both shells): direct on-canvas drag/resize for text/shape
   overlays, split of overlay clips — neither exists in egui either, so these are shared
   limitations, not migration parity gaps, matching the earlier finding for on-canvas crop/mask
   handles, ghost rendering, and the clipboard indicator.
4. Export queue parity: persistent `.ocqueue` snapshots, bounded worker pool, pause/resume/cancel/remove, loudness matching, GPU encoder preference, output conflict choices, and SRT export are implemented. **Visual coverage done:** `ExportPanel.test.tsx` (jsdom + `@testing-library/react`, `../ipc` mocked) covers empty/queued/rendering/paused render states, the loudness-match shortcuts' >=2-queued-jobs gate, and action-button wiring. True end-to-end coverage still needs the WebDriver harness gap 7 will port from `e2e/`'s current pywinauto suite.
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
   server-staged. **Scene/highlight detection, motion tracking, static and dynamic auto-reframe, background
   removal, privacy blur and TTS done:** each ports its `ui::App` background-thread counterpart
   command-for-command (same sampling rates/caps, same `avcore` pipelines -- scene cuts and
   highlights land as `Chapter`/`Highlight` markers, motion tracking writes `position_keyframes`,
   static auto-reframe writes `crop_x/y/w/h`, while dynamic auto-reframe samples at 2 Hz with
   subject continuity and writes sparse crop keyframes; background removal/privacy blur write a cached matte
   `.mp4` path + enable flag, TTS imports the synthesized WAV like any dropped file); each has a
   single-flight `AppState` guard and its own Tauri event for the async result. Region params
   (motion-track/privacy-blur center+size) are plain number inputs on the inspector rather than
   on-canvas pickers.
6. Preferences (locale switch, layout, hardware decode), autosave + crash recovery, error
   reporting consent, update check, telemetry, recent projects on Home, watch folder, YouTube
   download, sound library. **In progress:** desktop shares `prefs.oc` with egui for locale, LUFS
   defaults, hardware decode, output folder, autosave interval, recent projects, and every AI
   feature's model path; it also writes/restores the shared `.autosave.ocproj` files. **Sound
   library, watch folder and YouTube download done:** `list_sound_library` scans
   `prefs.sound_library_path`'s `music`/`sfx` subfolders into a popup; the new "watch" screen
   drives `avcore::process_watched_file` through a background poll loop
   (`start_watching_folder`/`stop_watching_folder`, `watch-folder-event`), same
   `StabilityTracker` cadence as `ui::App::watch_folder.rs`; the media library's YouTube modal
   downloads via `avcore::download_youtube`/`ytbridge` (`youtube-download-event`) -- all three
   import through the existing `import_media` pipeline on success.

   **Crash review, error-reporting consent/telemetry, and update check done:** ported as
   `apps/desktop/src-tauri/src/error_reporting.rs` (orchestration only -- the schema/sanitization/
   validation it depends on, `avcore::ErrorReportBuilder`/`validate_report`/`validate_envelope`,
   was already shared from `core`, not duplicated), `telemetry.rs` (local-only CPU/RAM/GPU
   sampling via `avcore::record_event`, never sent), and `commands/update_check.rs` (entirely
   through existing `avcore::update_check` functions -- release-asset naming is already generic
   to the whole app, so no packaging change was needed; auto-install stays Linux-only, same
   platform limitation the egui shell already has). A panic hook mirroring `ui::main.rs` writes
   `crash_<unix>.txt` to the same `platform_log_dir` both shells share, so a crash from either
   surfaces through the same post-crash "Send once / Always send / Do not send" review
   (`CrashReviewModal.tsx`). Consent stays two-state (`Disabled`/`AlwaysSend`, default
   `Disabled`): with no reporter held, nothing is built, validated, queued, or sent. Shared
   `prefs.oc` keys: `error_reporting_consent`, `last_reviewed_crash_unix`, `telemetry_enabled`.
   Not ported: per-call-site `report_error` instrumentation (ER-01A's fine-grained wiring across
   every command error path -- a much larger, separate refactor than this pass's scope). Editor
   layout persistence is done per project: the resizable library, inspector, and timeline
   dimensions write `Project::panel_layout` and restore when that project is opened.
7. Packaging: **in progress.** Windows/Linux/macOS `packaging/*` scripts, `windows-installer.iss`,
   the Debian/AppImage/desktop-entry metadata, and `validate_bundle.py`/`validate_deb.py` now
   ship `oca-desktop(.exe)` instead of `ui(.exe)`; `windows-installer.iss` warns if WebView2
   Runtime is missing instead of bundling an unpinned bootstrapper.

   **Critical packaging bug found and fixed:** `release.yml` originally built `desktop` via a
   plain `cargo build --workspace --release`. Verified live (2026-09-28) that this embeds the
   dev server URL (`http://localhost:5173`) into the binary instead of the built frontend --
   WebView2 shows a "can't reach this page" error on any machine without a Vite dev server
   running, i.e. every real install. Only building through the Tauri CLI
   (`npm run tauri build -- --no-bundle`) makes it embed the actual `frontendDist` (confirmed:
   the CLI-built binary loads `http://tauri.localhost/` with the real `<title>oca</title>` and
   JS/CSS assets; the plain-cargo one doesn't). `release.yml` now builds `ui`/`ytbridge` via
   `cargo build --release -p ui -p ytbridge` and `desktop` separately via
   `cd apps/desktop && npm run tauri build -- --no-bundle`, matching `make desktop-build`'s
   existing (already-correct) invocation -- this was a real ship-blocking regression this gap's
   own packaging slice introduced, not present in `crates/ui`'s build.

   **e2e harness ported and proven working end-to-end -- 9/14 real passing tests, 1 skipped, 4
   left blocked on missing desktop features (not testing gaps).** `e2e/conftest.py` drives
   `oca-desktop.exe` through `tauri-driver`/Selenium instead of pywinauto, invoking Tauri
   commands directly via `window.__TAURI_INTERNALS__.invoke` (`e2e/helpers.py`) rather than
   fighting native OS dialogs WebDriver can't reach. Ported and passing for real:
   `test_launch.py`, `test_navigation.py` (rewritten against the desktop shell's actual
   topbar/screen model, which never had egui's five-destination nav rail), `test_import.py`,
   `test_undo_redo.py`, `test_export.py` (a real FFmpeg render), `test_silence_review.py` (real
   waveform detection + ripple-delete), `test_clip_selection.py` (new -- the one thing that
   still needs a real canvas click, since the timeline has no per-clip DOM element).
   `test_auto_reframe.py` ports the same way but is skipped here (no bundled ONNX model in this
   tree, same condition the retired version used). `data-testid` selectors added to `App.tsx`/
   `Home.tsx`/`Editor.tsx`/`Prefs.tsx`/`WatchFolder.tsx`/`Timeline.tsx`/`Inspector.tsx`.

   The other 5 files stay quarantined in `e2e/legacy_pywinauto/` (excluded from collection) --
   not because porting them is hard, but because each tests a feature or an egui implementation
   detail the desktop shell doesn't have yet, confirmed by reading the actual current source
   rather than assumed: on-canvas layer drag/resize (gap 3, already documented missing), the
   speed-ramp preset menu (no command or menu wiring exists in `apps/desktop/src-tauri/src/
   commands/` at all), collapsible property sections (gap 3, already documented missing;
   `Inspector.tsx` renders everything flat), viewport-culling drag continuation and specific
   historical layout bugs (both pure egui-immediate-mode implementation details with no React/
   CSS-grid equivalent to test). `e2e/legacy_pywinauto/README.md` has the full per-file
   reasoning. Porting these needs the missing desktop features built first (a separate,
   substantial slice of work, not part of "port e2e to WebDriver"), or, for the two
   egui-specific ones, a bespoke test designed against desktop's actual drag/layout code.

   **Debug-build launch crash diagnosed, confirmed not a shipping blocker.** A `cargo build -p
   desktop` (debug profile) binary aborted at startup with an unlogged "Microsoft Visual C++
   Runtime Library" dialog. Captured a real crash dump (Sysinternals `procdump -ma -x`, no admin
   needed) and analyzed it with `cdb.exe` (extracted from the Windows SDK installer via
   `msiexec /a ... TARGETDIR=...`, also no admin -- WinDbg Preview's GUI `-c`/`-cf` automation
   did not work headlessly in this environment). Symbolized stack: `ucrtbased!_free_dbg` ->
   `_CrtDbgReportW` -> the observed dialog, called from `oca_desktop!std::_Tree_val<...
   std::pair<enum asr_tensor const, char const*>...>::_Erase_tree` -- the debug CRT's heap
   validator catching a destructor call on a `std::map` owned by a statically-linked C++
   dependency (most likely whisper.cpp via whisper-rs-sys), during static initialization. A
   `--release` build (non-debug `ucrtbase.dll`, no such validation) launches fine -- confirmed
   directly, and it's what both the installer and `e2e/` now use by default. `scripts/
   diagnose-desktop-crash.ps1` reproduces this capture+analysis on another machine.
   `e2e/conftest.py`'s `OCA_DESKTOP_EXE` env var can still point at a debug build if it ever
   launches cleanly elsewhere.

   **`crates/ui` is not yet deleted.** Not because of a new blocker -- the packaging bug is
   fixed, the launch crash is diagnosed and non-blocking, and the WebDriver harness is proven
   working -- but because the migration's own top-level strategy ("New crate in parallel, egui
   untouched until parity") was never actually met: on-canvas layer transform, the speed-ramp
   preset, and collapsible property sections are real, confirmed-missing desktop features, not
   e2e-porting gaps. Deleting the egui shell before those are built would be a genuine feature
   regression for real users, independent of how much of gap 7 itself is done. That feature work
   is its own slice, not part of "port e2e" or "replace the installer."

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
