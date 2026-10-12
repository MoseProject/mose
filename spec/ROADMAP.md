# Roadmap — Actionable Queue

**Start here to pick the next task.** Reconciled against actual current status (matrix files +
old `CLAUDE.md` Status section, now `matrix/changelog.md`) as of 2026-08-28 — `features/
request.md`'s original phased plan (Fase 1-8) is effectively done; this queue is what comes
after it. Status markers: `[x]` done, `[~]` partial, `[ ]` not started.

**Before picking up any `[ ]`/`[~]` item: read [RULES.md](RULES.md) first.** Short — mandatory
rules + definition of done.

---

## P0 — Editing Foundations

These undermine trust in *everything* built on top — an editor without undo is one mistake away
from losing work, and dragging without snap makes frame-precise cuts tedious for every future
feature that touches the timeline.

1. `[x]` **Undo/redo.** `core::undo::UndoStack` wired into `ui` — `Ctrl+Z`/`Ctrl+Y`, toolbar
   buttons, covers clip add/split/delete/cut/copy/paste, trim/move drags, track/text/shape-
   track add, composite merge, paste-formatting, text-color-modal confirm, and every effect-
   property slider/keyframe editor (gain/crop/mask/color/blur/etc., via a drag-coalescing
   `push_undo_snapshot_for_drag` so a held slider is one undo step, not one per frame);
   `undo_stack.clear()` on sequence/project switch. **Verified**: `cargo test -p core --test
   undo_test` (7/7) and `cargo test -p ui` (239/239, including the drag-coalescing tests) pass;
   the track-add/remove path is additionally covered by a live e2e smoke test against the real
   compiled binary (`e2e/test_undo_redo.py`). See
   [architecture/undo-redo.md](architecture/undo-redo.md) for the design.
2. `[x]` **Magnetic snap** while dragging — playhead scrubbing on the ruler, clip trim (either
   edge), and clip body move (single or composite-group) all snap to the nearest other clip's
   start/end edge or the playhead, within a fixed pixel threshold at the current zoom
   (`screens::editor::timeline_panel`'s `snap_to_nearest`/`snap_move_start`, `SNAP_THRESHOLD_PX`
   = 8px). Hold `Alt` to temporarily disable snapping, the common editor convention. Pure
   functions, unit tested (8 cases: within/outside threshold, no targets, leading-edge snap,
   trailing-edge snap, whichever edge needs less adjustment, out-of-range no-op) — not yet
   driven through a live/e2e build (this is UI-only interaction logic, no avbridge/GStreamer
   pipeline involved to verify against). Timeline markers aren't a snap target — no markers
   feature exists yet (P2 item 9); revisit when it lands. Unblocks D5 (beat-aligned snap, P3),
   which extends this with waveform low-energy points as an additional target.

## P1 — Performance Infrastructure

Read [architecture/performance-and-caching.md](architecture/performance-and-caching.md).

3. `[~]` Dirty-flag mutation classification for timeline edits (position vs. content vs.
   effect vs. track). Confirmed the premise didn't hold as stated — position/effect edits
   already skip a reopen (they just don't live-update the pipeline either, a separate gap);
   the one real hot-path violation found (text-clip property panel forcing a full pipeline
   reopen on every dragged-slider frame, bundling position/content with start/duration) is
   fixed via a cheap `appsrc` buffer refresh. **Partial follow-up**: color-balance sliders
   (brightness/contrast/saturation — the confirmed hot-path violation's own motivating
   example) now update live too, via `Preview::set_live_balance` pushing directly to the
   already-built `videobalance` element (`gst::Bin::by_name`, named `oca_balance_{clip_id}`)
   instead of waiting for an incidental reopen — deliberately scoped to just this one
   property group, not the full "every effect setter" scope, which stays a materially
   bigger lift (most effects' elements are only conditionally present at all — e.g. no
   `gaussianblur` exists in the pipeline until `blur_intensity` first goes non-zero — so a
   live update for those needs restructuring the running filter graph, not just a property
   push; color balance was tractable specifically because brightness/contrast/saturation all
   share one element that, once built, stays present across any further change among the
   three). Verified against a real GStreamer pipeline in a scratch crate (this sandbox's
   `core` test binary can't link — the ONNX Runtime gap — but `avbridge` alone can, so a
   scratch crate depending on real `avbridge` + `gstreamer` proved the negative-path logic
   for real): `set_live_balance` correctly declines when no element was built, and correctly
   declines for a mismatched clip id. **Not verified**: that it finds/updates the element
   once one exists, or that a push actually changes decoded output — this sandbox's `playbin`
   never constructs a working video output branch at all (`current_frame()` returns `None`
   for every clip, confirmed against an unmodified copy of `preview_test.rs`'s own
   pre-existing test), the same GUI/hardware-dependent-verification limitation this codebase's
   test suite already carries elsewhere.

   **Second follow-up**: blur/sharpen sliders also update live now, via `Preview::set_live_blur`
   pushing to the already-built `gaussianblur` element (named `oca_blur_{clip_id}`, same naming
   convention `set_live_balance`'s `videobalance` element uses) — the one other case where a
   single named element, once built, stays present across any further change to the property
   that built it: `gaussianblur`'s single signed `sigma` covers both `blur_intensity` (positive)
   and `sharpen` (negative) via one derived `net_sigma`, so there is no "which of several
   properties" ambiguity color balance's three-property case has to resolve either. Wired
   through the same `App::with_selected_clip_mut` shared dispatch path color balance's own push
   already goes through, so every effect setter gets the cheap no-op push for free without its
   own call site. Verified via `cargo check`/`clippy` for the whole workspace (clean) and two
   negative-path tests (`set_live_blur_returns_false_when_no_element_was_built`/
   `_for_a_mismatched_clip_id`, mirroring `set_live_balance`'s own two exactly) — same "type-
   checked but not run" status as those, since `core`'s own test binary still can't link in this
   sandbox (the ONNX Runtime gap) and reproducing `preview.rs`'s full `Preview`/`ClipInstance`
   surface in a throwaway scratch crate (unlike `set_live_balance`'s own verification, which
   only needed `avbridge` + `gstreamer`, not the two-thousand-line `preview.rs` module itself)
   wasn't attempted this round.

   **Third follow-up**: chroma-key color/tolerance also update live now, via
   `Preview::set_live_chroma_key` pushing to the already-built `alpha` element (in
   `method=custom` mode, named `oca_chromakey_{clip_id}`) — a third case fitting the same
   "single element, once built, stays present across further edits to the property that built
   it" shape. Unlike balance/blur, this element is only ever built for a composited-overlay
   branch gated on `chroma_key_enabled`, a plain on/off toggle rather than a gradually-
   approached intensity — toggling chroma key on for the first time still needs the existing
   incidental-reopen fallback to build the element at all; only color/tolerance edits made
   *after* that go live. Wired through the same shared dispatch path. Two negative-path tests
   mirror the blur ones exactly, one confirming a plain (non-composited) pipeline never builds
   this element at all, the other a mismatched clip id against a real chroma-key-enabled
   overlay clip — same "type-checked, not run in this sandbox" status.

   **Fourth follow-up**: crop, pixelize, shake, and (layer) mask all update live now too,
   closing out this item's own "genuinely a bigger lift" note above — each needed a materially
   different mechanism, not a copy-paste of `set_live_balance`'s plain property push:
   `Preview::set_live_crop` names the static-crop `videocrop` element (`oca_crop_{clip_id}`) and
   redoes the fraction-to-pixel math using a new `Preview::clip_resolutions` cache (populated at
   build time) so `ui`'s `App` never needs to track decoded frame dimensions itself;
   `Preview::set_live_pixelize` renegotiates the downscale `capsfilter`'s caps live (block-size
   caps changes propagate as a real renegotiation, unlike shake/crop's window shape) rather than
   rebuilding the two-`videoscale` chain; `Preview::set_live_shake` writes into a shared
   `ShakeMarginHandle`'s atomics the buffer probe already reads every frame — no property push
   or renegotiation needed at all, just new numbers; `Preview::set_live_mask` reuses the
   `refresh_text_overlay` pattern (`appsrc ! imagefreeze(allow-replace=true)`) to push a freshly
   rasterized `GRAY8` buffer. Deflicker/stabilization remain the genuinely open gap — both need
   *temporal* state across multiple frames, a different problem shape entirely. Wired through the
   same `with_selected_clip_mut` shared dispatch path as balance/blur/chroma-key. See
   `matrix/performance.md` for the full findings.

   **Fifth follow-up**: shape overlay clips (`TrackKind::Shape`, decorative rectangles/ellipses/
   arrows/etc., not to be confused with the background-removal `mask_shape` [`MaskShapeBranch`]
   already covered by `set_live_mask` above) now update live too, closing this item's last
   remaining gap. `open_composited`'s shape-overlay loop previously called
   `build_static_overlay_branch` and discarded the returned `appsrc` — no live-update path
   existed at all, so every shape-clip properties-panel edit fell through to a full pipeline
   reopen. Added `ShapeOverlayBranch` (`{ clip_id, appsrc, canvas_width, canvas_height }`,
   mirroring `MaskShapeBranch`'s shape — a shape clip has no time-dependent rasterization input,
   unlike `TextOverlayBranch`'s word-highlight state) and `Preview::refresh_shape_overlay`,
   copying `refresh_text_overlay`'s exact re-rasterize-and-push shape via
   `render_shape_clip_rgba`. On the `ui` side, `App::refresh_preview_shape_content` mirrors
   `refresh_preview_text_content` (checks the pre-existing `preview_shape_clip_ids` reopen-
   detection list, which had been tracked all along for diffing but never used for a live push),
   wired into `shape_clip_properties`' apply block the same way `text_clip.rs` splits
   `structural_changed` (start/duration, which can change which clips cover the playhead) from
   every other field (kind/color/position/size/rotation/stroke, content-only). Verified via
   `cargo check`/`clippy --all-targets` across the whole workspace (a local, never-committed
   FFmpeg-7.1-symbol shim plus an `FFMPEG_DIR`/`lib`-symlink workaround for this sandbox's
   multiarch package layout got a real type/borrow-check pass past the pre-existing FFmpeg-too-
   old gap and the GStreamer pkg-config gap) — no errors, no new clippy warnings; GStreamer
   pipeline behavior itself is untested in this sandbox (no display, no real playback), same
   caveat as every other `preview.rs` live-update path above.
4. `[x]` Versioned cache for the timeline→avfilter-graph resolution
   (`resolve_timeline_segments_multi`). The confirmed hot spot was `screens::queue::show`
   recomputing it every UI frame the Fila screen is open, just for a size estimate — fixed via
   `App::resolved_active_sequence_export_preview`'s value-equality cache. See
   `matrix/performance.md` for what is/isn't covered.
5. `[x]` Extract a shared `FrameSampler` primitive — `avcore::FrameSampler`
   (`crates/core/src/av/frame_sampler.rs`) now backs auto-reframe, motion tracking,
   background-removal matte generation, and (found during the same pass) thumbnail extraction,
   which had the identical shape. Session-reuse for `background_removal::segment_person`'s ONNX
   session is a related but separate gap, left open — see `architecture/performance-and-
   caching.md` §5.

## P2 — High-Impact Parity

Read [matrix/effects-and-color.md](matrix/effects-and-color.md),
[matrix/robustness.md](matrix/robustness.md), [matrix/competitor-parity.md](matrix/competitor-parity.md).

6. `[x]` Audio ducking (auto-lower music under speech). Reuses the existing `AudioRole`
   (`Mic`/`Music`/`GameAudio`/`Unspecified`) rather than new per-track metadata:
   `AudioRole::to_duck_role_code()` (core) maps it to a raw `u8` crossing the FFI boundary as
   `AudioSegment::duck_role`, and `build_mix_graph` in `avbridge/csrc/audio_mix.c` routes every
   `Music`-tagged branch through a `sidechaincompress` keyed by the mixed `Mic`-tagged branches
   whenever both roles are present on the timeline — opt-in and additive, byte-identical to the
   old flat `amix` otherwise. A trigger (mic) branch needs its audio in two places at once (the
   sidechain control input *and* still audible in the final mix) but a filter output pad can
   only be consumed once, so each trigger branch gets its own `asplit` feeding both consumers —
   the bug that produced `AVERROR(EINVAL)` on the first attempt, found by real
   `avfilter_graph_config` runs, not by re-reading the C. Verified for real, not just
   syntax-checked: `crates/avbridge` has zero heavy dependencies (no ONNX/whisper/GStreamer), so
   `cargo test -p avbridge --test audio_mix_test` fully links and runs in this sandbox against
   real fixture media — three new integration tests cover single-branch-per-role, multiple
   branches per role (the `music_mix`/`trigger_mix` sub-`amix` paths), and a target with no
   trigger falling back to the original flat mix. `resolve_audio_segments` wiring covered in
   `core`'s own test suite. The prior skip note (no FFmpeg dev headers, `pkg-config` found
   nothing) no longer applies in this sandbox — `pkg-config --cflags/--modversion libavfilter`
   both resolve now.
7. `[x]` Color scopes (waveform/vectorscope) for calibrated grading. `avcore::scopes`
   (pure pixel analysis, no new avfilter/GStreamer element) + an opt-in "📊" toggle on the
   Editor preview panel. Grayscale-intensity simplification, not a calibrated-graticule
   broadcast scope — see `matrix/effects-and-color.md` for the exact scope (pun intended) of
   what shipped.
8. `[x]` Export presets per platform (YouTube Shorts / Instagram Reels / TikTok — resolution +
   aspect + LUFS target bundled under one name). `avcore::PlatformExportPreset`
   (`crates/core/src/project_io/export.rs`) + `App::apply_platform_export_preset` + a one-click button row
   on the Fila screen, above the existing aspect-ratio/LUFS pickers (which stay live afterward
   for fine-tuning — a preset isn't a lock). All three presets currently resolve to the same
   numbers (1080x1920, -14 LUFS, matching this codebase's own existing "YouTube" LUFS profile)
   — a real current fact about these platforms' delivery specs, not a shortcut: each preset
   still carries its own independent mapping, ready to diverge without a shape change. Pure
   Rust/UI, no `avbridge` C changes — picked deliberately over item 6 for that reason.
9. `[x]` Review/comment markers on the timeline — `avcore::timeline::Marker`/`MarkerKind`
   (Standard/ToDo/Chapter, FCP's typed-marker model) + a searchable Timeline Index panel
   (text search, click-to-seek, inline edit). Not done: markers as a magnetic-snap target, and
   ruler tick-mark rendering — see `matrix/timeline-and-editing.md` for the exact scope.
10. `[x]` **Multicam editing** — sync footage from multiple sources (game capture, webcam, mic),
    switch angles dynamically. Scoped to the smallest end-to-end vertical slice that's honestly
    "shipped," not half-wired: angles are ordinary `Video` tracks (no new `TrackKind`), grouped
    by a new `avcore::timeline::MulticamGroup` sidecar record (member tracks, program/active
    track, per-track sync offsets) — same non-invasive shape `Marker` uses.
    - **Sync**: audio-waveform only (not timecode — no code in this repo reads embedded
      timecode/creation-time metadata, and waveform sync alone satisfies "sync by audio").
      `avcore::multicam_sync` cross-correlates each track's first clip's audio (via the
      already-existing `avbridge::extract_pcm_16k_mono`, previously whisper.cpp-only) against a
      reference track — a coarse RMS envelope + bounded lag search, not full-resolution
      correlation (too slow over a multi-minute recording). Pure Rust, no new `avbridge`/C code.
    - **Switching**: `Timeline::switch_multicam_angle` splits the program track's clip at the
      playhead (`Track::split_clip_at`, already used by D1/D6) and retargets the new piece's
      `asset_id`/`source_in_secs`/`source_out_secs` to the target angle's footage at the
      sync-offset-adjusted equivalent time — reuses existing editing primitives end to end,
      no new avfilter/C wiring. `App::create_multicam_group_from_video_tracks` (Editor toolbar's
      "Sync Multicam" button) builds the group; number keys 1-9 at the playhead switch angles.
    - **Export correctness falls out for free**: once a switch is materialized as an ordinary
      same-track clip split, `resolve_timeline_segments_multi`/`resolve_audio_segments` already
      produce correct segments — no `render.rs` changes needed.
    - **Explicitly not done** (documented, not silently missing): live multi-feed preview during
      scrub/playback — still plays one decoded source, same "no live preview effect yet" gap
      every other per-clip effect has; a real multicam monitor needs GStreamer multi-branch
      preview-pipeline work, a separate and larger task. Verified via a real-execution scratch-
      crate (this sandbox's `core` test binary can't *link* — the pre-existing ONNX Runtime gap
      — but `timeline.rs`/`keyframe.rs`/`multicam_sync.rs` have zero heavy deps, so copying them
      into a throwaway crate gives genuine `cargo test` runs, not just type-checking) — 15 new
      tests across sync-offset correlation and group/switch semantics, all passing for real.
11. `[x]` **Named trim modes: Ripple / Roll / Slip / Slide.** Confirmed: oca's existing trim/move
    (`ClipInstance::trim_start`/`trim_end`, `Track::move_clip`) matched none of the four —
    trimming an edge never touched neighboring clips at all (no ripple, no roll), and there was
    no way to change source-in/out without moving the clip or changing its duration (no slip).
    All four now implemented as their own `EditorTool` toolbar modes — see
    `matrix/timeline-and-editing.md` for the exact scope and what's still unverified.
12. `[x]` **D3 — series-level loudness consistency** across an export-queue batch
    (`architecture/differentiators.md`). `App::match_loudness_across_queued_jobs` + a button row
    on the Fila screen (shown once ≥2 `Queued` jobs exist) sets one target LUFS across every
    not-yet-started job in the batch — each queued job otherwise keeps whatever `target_lufs`
    its own sequence/tab happened to have when it was queued, so episode 1 and episode 5 of a
    series could silently end up with mismatched targets. Pure orchestration, no new DSP —
    reuses the existing per-job `target_lufs` field and `LUFS_PROFILES` picker.
13. `[x]` **Multi-file drop lands on parallel tracks, not one stacked track.** Reported gap:
    dropping/dragging several files onto the Editor at once always landed them concatenated,
    one after another, onto the same single track (`App::add_asset_to_timeline`'s
    `resolve_or_create_track(_, kind, None)` always reuses the first existing track of that
    kind) — the only existing way to get simultaneous parallel tracks was the dedicated
    Multicam feature or manually dragging each file to its own track row one at a time. Fixed:
    `App::handle_dropped_files` now treats a batch of 2+ dropped files as simultaneous material
    (e.g. multiple camera angles, or a video-plus-narration pair) and routes each through the
    new `App::add_asset_to_new_track` (always `create_new_track`, never reuse) instead of
    `add_asset_to_timeline` — every file in the batch lands on its own fresh track, each
    starting at `0.0` since a brand-new track is always empty. A single dropped file keeps the
    original append-to-existing-track behavior unchanged. Does not touch the "Importar
    arquivos" button path (library-only import, no auto-add-to-timeline for either single or
    multi-file selection) or the Multicam feature, which remains the deliberate flow for
    sync-aligned (non-zero-offset) multi-angle grouping.

## P3 — Differentiators

Full spec: [architecture/differentiators.md](architecture/differentiators.md). Ordered by
effort and dependency, not by the doc's own numbering. CapCut's "Long Video to Shorts" and
Premiere's Auto Ducking independently confirm D2/D6 and P2 item 6 are competitively expected,
not novel guesses — see `matrix/competitor-parity.md`.

13. `[x]` **D1 — automatic silence/dead-air cut** (`architecture/differentiators.md`). Built on
    `avcore::waveform`'s per-bucket `(min, max)` peaks, not `avcore::loudness` as the doc
    originally assumed — `measure_loudness` is a single-pass whole-file aggregate (one
    LUFS/TruePeak/LRA report), not a time series, so it can't say *where* in a clip a silence
    falls; `generate_waveform`'s peaks already are windowed amplitude data, and most assets have
    them cached (`MediaAsset::waveform_peaks`) with no new decode pass needed.
    `avcore::silence_detection::{detect_silence_gaps, clip_silence_gaps}` are pure, fully unit-
    tested functions; `Track::ripple_delete_range` is the apply side (splits any clip straddling
    a gap's boundaries, drops what's fully inside, ripples the rest left). `ui`'s toolbar
    "Detect Silence" button + a review modal (`App::begin_silence_review`/
    `toggle_silence_gap_accepted`/`apply_silence_review`/`close_silence_review`) lists every
    detected gap with an accept/reject checkbox defaulting to accepted — nothing touches the
    timeline until "Apply selected cuts" is pressed, per the doc's explicit "not a silent
    auto-apply" requirement. Not yet run against a real GUI session (this environment can't
    launch the eframe app) — verified via `cargo check --workspace --all-targets` (temporary
    local FFmpeg-7.1 shim, see `CLAUDE.md`) and unit/integration tests covering the detection
    math, the ripple-delete edit, and every App-level review method.
14. `[x]` **D7 — lightweight collaboration package** (`architecture/differentiators.md`).
    `avcore::collab_bundle::{export_collab_bundle, import_collab_bundle}` package a project's
    `.ocproj` snapshot plus whatever editing proxies already exist in its proxy cache
    (`avcore::proxy`) into one portable `.zip` — never the multi-GB source media. On import, each
    asset's `proxy_path` is resolved by matching its `source_path`'s filename stem against the
    unpacked proxy dir across every `PreviewQuality`, so the recipient's preview works
    immediately even though the original source almost certainly isn't at that `source_path` on
    their machine at all. `zip` added as a `default-features = false` dependency —
    `CompressionMethod::Stored` is used explicitly since every entry (already-gzip-compressed
    project bytes, already-encoded proxy `.mp4`s) is incompressible in practice, so no optional
    codec needs linking. `ui`: Editor toolbar's "📦 Export Collaboration Bundle..." (save-file
    dialog) and Início's "📦 Import Collaboration Bundle..." (pick `.zip`, then a destination
    folder). **Not run against a live GUI session** — same verification ceiling as D1 above
    (`cargo check --workspace --all-targets` via the temporary local FFmpeg shim, plus
    `cargo check -p core --lib` actually linking and one `cargo test` attempt confirming the
    ONNX Runtime link gap is the only remaining blocker to real execution here); the round-trip
    (export → import, including a fake proxy file surviving intact and resolving to a *new* path
    under the recipient's own cache dir) is covered by `core`'s integration tests and mirrored at
    the `App` level in `ui`.

    **Security follow-up: fixed a real zip-slip (CWE-22) in `import_collab_bundle`.** A `.zip`
    bundle is attacker-controllable data — shared by a collaborator, or downloaded from
    anywhere — and `import_collab_bundle` was joining each proxy entry's *embedded* path
    directly onto `proxy_dir` with no sanitization (`proxy_dir.join(file_name)`, `file_name`
    taken verbatim from the zip entry name after the `proxies/` prefix). A malicious entry named
    e.g. `proxies/../../../../etc/cron.d/evil` climbs out of the proxy cache dir entirely via
    `PathBuf::join`'s own `..`-following behavior, and one named `proxies//etc/passwd` (an
    absolute path as the "filename") replaces the whole base path outright, since `Path::join`
    with an absolute-looking component discards everything before it — this is exactly this
    codebase's own cited CWE-22 weakness class (`architecture/competitive-feature-plan.md`'s
    security section), not a hypothetical: this session confirmed it for real, not just by
    reading the code, by building an actual malicious `.zip` with the real `zip` crate and
    watching the unfixed code overwrite a real file it had no business touching (a canary file,
    and separately a real `/etc/should-not-exist` write in this sandbox, both reproduced before
    the fix and gone after it). Fixed via `safe_proxy_entry_filename`, which reduces an entry's
    embedded path down to [`Path::file_name`]'s own last-component result before ever joining it
    onto `proxy_dir` — `file_name()` already discards any `..`/root/prefix components, so a
    traversal or absolute-path entry degrades to just its basename inside the cache dir instead
    of escaping it; an entry with no valid basename at all (bare `..`, `.`, or a trailing
    separator) is skipped, matching this function's own existing "known gap, not silently wrong"
    tolerance for other malformed bundle contents, rather than erroring out the whole import.
    Verified for real, not just type-checked: this session's sandbox turned out to have a working
    path to a fully-linked `core` (`rustup update stable` past the too-old-bundled-rustc block,
    then `apt-get install libavfilter-dev`/`libgstreamer*-dev` past the missing-headers/
    pkg-config gaps, plus the documented temporary local `filters.c` shim, discarded before
    commit) — `cargo check -p core --lib`, `cargo check --workspace --all-targets`, `cargo fmt`,
    and `cargo clippy -p core --lib --no-deps` all stayed clean, but `core`'s own test binary
    still can't *link* here (the pre-existing ONNX Runtime `download-binaries` network gap), so
    `collab_bundle.rs` (zero heavy deps beyond `persistence`/`proxy`/`project`, none of which
    touch `avbridge`/GStreamer/ONNX for the functions it actually calls) was copied into a
    throwaway scratch crate alongside real `zip`/`serde`/`rmp-serde`/`flate2` and stripped
    stand-in `Project`/`MediaAsset`/`proxy` types, then `cargo test`ed there for real: 11/11
    passing, including a dedicated regression test that hand-builds a malicious `.zip` (both a
    relative `../../../../` traversal and an absolute-path entry) and confirms neither escapes
    the recipient's proxy cache dir — confirmed meaningful by temporarily reverting the fix in
    the scratch crate and watching that exact test fail (and, the first time, actually write to
    `/etc/should-not-exist` for real) before restoring it. 7 new unit tests on
    `safe_proxy_entry_filename` itself (plain filename, relative traversal, absolute path, bare
    `..`/`.`, a trailing bare traversal, a nested-but-ordinary path) plus a new integration test
    mirroring the scratch-crate regression case were added to the real `crates/core/src/
    collab_bundle.rs`/`crates/core/tests/collab_bundle_test.rs` themselves.
15. `[x]` **D4 — automatic chapter markers from scene cuts** (`architecture/differentiators.md`).
    `avcore::scene_detection::detect_scene_cuts` scores consecutive sampled-frame pairs by mean
    absolute luma difference (reuses `motion_tracking::rgba_to_gray` for grayscale conversion,
    `avcore::FrameSampler` for sampling — same primitives motion tracking's own background
    thread already uses, not a new decode pass). Detected cuts become non-destructive
    `MarkerKind::Chapter` markers (P2 item 9) rather than a separate accept/reject modal — the
    existing Timeline Index panel's rename/delete already is the review step, since a marker
    (unlike D1's ripple-delete) never mutates the timeline itself. `ui`: Editor toolbar's
    "Detect Chapters" button (background thread, mirrors `motion_tracking.rs`'s split exactly)
    and "Export chapters (.txt)" (plain-text `H:MM:SS Label` list, YouTube's own chapter
    format). Verified the same way as D1/D7 above (`cargo check --workspace --all-targets` via
    the temporary local FFmpeg shim); `core`'s frame-diff scoring and every `App`-level chapter
    method are unit tested, but the actual FrameSampler-driven background thread isn't run
    against real video in this sandbox.
16. `[x]` **D5 — beat-aligned cut snapping** (`architecture/differentiators.md`).
    `waveform_snap_points_for_clip` (`screens::editor::timeline_panel`) reuses
    `avcore::clip_silence_gaps` (built for D1) purely as a "quiet moment finder": each detected
    gap's midpoint is a candidate snap target, with a much shorter minimum gap (0.05s) than D1's
    own cuttable-gap threshold (0.5s) — D5 wants any brief natural pause, not just a length worth
    actually cutting. Wired into the two trim-edge (cut-point) drag handlers only, not the
    whole-clip body-move handler — moving a clip doesn't cut audio. Every trim-driven
    `EditorTool` (plain trim, Ripple, Roll) shares the same snapped value downstream, so one
    change benefits all of them. Verified via `cargo check --workspace --all-targets` (temporary
    local FFmpeg shim) plus new unit tests for the pure mapping function; the actual drag
    interaction isn't run against a live GUI session, same caveat every other timeline-panel
    interaction change in this file already carries.
17. `[x]` **D2 — highlight detection from audio spikes** (`architecture/differentiators.md`).
    Resolved the design fork this item's own investigation note (2026-08-27, preserved below)
    found — per-track role metadata, not the scoped-down independent-scoring fallback, since it
    matches the doc's actual "simultaneous" premise and doubles as groundwork for the
    already-queued Multicam item (P2 item 10).
    - `avcore::timeline::AudioRole` (Unspecified/GameAudio/Mic/Music) on `Track`, user-set via a
      small icon `ComboBox` on the timeline track header (`App::set_track_audio_role`, not
      undo-tracked — metadata, not content).
    - `avcore::highlight_detection::{clip_amplitude_samples, detect_highlight_candidates}`:
      `clip_amplitude_samples` maps one clip's asset waveform into timeline-relative amplitude
      samples (same source-to-timeline mapping `clip_silence_gaps`/D1 uses, but every bucket's
      peak, not just below-threshold runs — a highlight cares about loud moments).
      `detect_highlight_candidates` correlates two independently-sampled, irregularly-spaced
      series onto a common coarse time grid and flags windows where both channels spike at once.
    - `MarkerKind::Highlight` — auto-detected candidates become non-destructive markers, same
      "Timeline Index panel's rename/delete is the review step" shape D4's Chapter markers
      already established, not a separate accept/reject modal.
    - `ui`: `App::detect_highlights` runs synchronously (no background thread — only reads
      `MediaAsset::waveform_peaks`, already cached in memory, same reasoning D1's
      `begin_silence_review` relies on) from the Editor toolbar's "Detect Highlights" button.
    - Verified via `cargo check --workspace --all-targets` (temporary local FFmpeg shim);
      `core`'s own math was independently confirmed by extracting `highlight_detection.rs` into
      a throwaway no-dependency scratch crate and running its tests for real (this sandbox's
      `core` crate itself only type-checks — the ONNX Runtime link gap blocks `cargo test`
      outright, see `CLAUDE.md`), plus new `App`-level unit tests for the full detect-and-mark
      flow.
    <details><summary>Original investigation note (2026-08-27), preserved for context</summary>

    Not started as of that note: the doc's premise is "simultaneous game-audio + mic spikes,"
    but `Project`/`Timeline` had no structural game-audio-vs-mic distinction —
    `create_new_project` starts with zero tracks, and Video/Audio track *kind* alone doesn't say
    which audio track is the mic and which is a video asset's embedded game audio (the "V1"/
    "A1"/"A2" names seen in test fixtures were just convention, never an enforced or even
    UI-surfaced role). Scoring "two streams at once" needed *some* answer to "which track is
    which" before any DSP could be written — either new per-track metadata (a "role" tag the
    user sets) or a scoped-down v1 that scores every audio-bearing track independently and OR's
    the results (loses the "simultaneous" cross-correlation the doc specifically calls out). Left
    open rather than guessed at; resolved above once asked.
    </details>
18. `[x]` **D6 — one-click shorts pack** (`architecture/differentiators.md`). Confirmed three
    scope decisions with the user before writing this — the biggest single item in P3, several
    real design forks, not a blind-implementable reuse:
    - **Highlight window size**: D2 only stores a single marker position per highlight, not a
      range, so D6 needed its own heuristic — a fixed 5s lead-in + 10s reaction (15s total),
      clamped to the sequence's own bounds, over a user-configurable setting (no new UI surface
      needed).
    - **Auto-reframe**: reuses whatever `position_keyframes` the windowed clip already carries
      rather than running a fresh face-detection pass per short — a short whose source clip was
      never auto-reframed just exports centered, a real documented gap, not a silently-forced
      extra pipeline run.
    - **Subtitles**: reuses existing transcribed `TextClip`s that fall inside the window rather
      than triggering a fresh Whisper run — a never-transcribed source just exports without
      subtitles, same reasoning as auto-reframe above.

    `avcore::extract_timeline_window` (new `core` primitive, `timeline_window.rs`) turns one
    `[start, end)` slice of a `Timeline` into its own standalone, rebased-to-zero `Timeline`:
    Video/Audio clips straddling a boundary are split precisely via the existing
    `Track::split_clip_at`; Text/Shape overlays skip splitting entirely (word-highlight timing is
    relative to a `TextClip`'s own `start_secs`, so approximating a mid-clip split risks
    desyncing captions from audio) — only overlay clips *entirely* inside the window are kept.
    `App::spawn_shorts_pack` loops every `MarkerKind::Highlight` marker, extracts its window,
    resolves it exactly the way "Adicionar exportação" resolves the whole sequence (forced to
    `ExportAspectRatio::Portrait`), and queues it — a window that resolves to zero clips (e.g. it
    landed entirely in a gap) is skipped, not treated as a hard error, and the toolbar's "Shorts
    Pack" button reports a queued-vs-skipped summary toast.

    Verified via `cargo check --workspace --all-targets` (temporary local FFmpeg shim);
    `extract_timeline_window`'s split/boundary math was independently confirmed for real by
    extracting `timeline.rs`/`keyframe.rs`/`timeline_window.rs` into a throwaway no-dependency
    scratch crate and running its tests (this sandbox's `core` crate itself only type-checks —
    the ONNX Runtime link gap blocks `cargo test` outright, see `CLAUDE.md`), plus new `App`-level
    unit tests for the full extract-resolve-queue flow. **Not run against a live GUI session** —
    the actual exported `.mp4` files (correct framing/timing/audio for a real windowed short)
    haven't been visually verified, same caveat every UI-only change in this sandboxed
    environment already carries.

## P4 — Hardware-Dependent / Confirmed Hard Walls / Lower-Priority Parity

Not blocked on design decisions — blocked on hardware/tooling this dev environment doesn't
have, or genuinely lower value for a small/single-editor channel. Pick up opportunistically,
not by default priority.

19. `[~]` GPU encode real-hardware verification (NVENC/Quick Sync/AMF/VAAPI/VideoToolbox) —
    `matrix/engine.md`. **NVENC confirmed working on real hardware** this session, against a
    real NVIDIA GeForce RTX 4070 (driver 610.74) — ground-truthed two ways: (1) a direct
    `ffmpeg -c:v h264_nvenc` CLI smoke test, bypassing oca entirely, confirms this exact
    `FFMPEG_DIR` build's `h264_nvenc` opens and encodes on this hardware at all; (2) added one
    purely-additive `av_log` line to `open_video_encoder` (`gpu_encoder.c`) — no signature/ABI
    change, since `avcodec_open2` was previously silent on success, making it genuinely
    impossible to tell "opened real GPU hardware" from "silently degraded to CPU" from outside
    the function. With that log line in place, `cargo test -p avbridge --test encode_test
    every_gpu_encoder_preference_falls_back_to_a_working_export` (the test built specifically to
    exercise every `GpuEncoderPreference` variant) now prints `oca: opened video encoder
    h264_nvenc (hardware)` for both the `Auto` and explicit `Nvenc` cases — direct proof oca's
    own code path opened real NVENC, not just that a valid file happened to come out the other
    end. Quick Sync and AMF still correctly fall back to CPU on this machine (no Intel iGPU/AMD
    GPU present) — their own failure paths log clear driver errors (`Error creating a MFX
    session`, `DLL amfrt64.dll failed to open`), consistent with past sessions' findings. VAAPI
    is Linux-only (`#ifdef __linux__` in `gpu_encoder.c`) and stays unverified — no Linux
    machine with a VAAPI-capable GPU available in any session so far. **Still not verified**:
    Quick Sync/AMF's *positive* path (this machine genuinely has neither), and VAAPI at all.
    **VideoToolbox confirmed working on real Apple hardware** this session: a MacBook Pro with
    Apple M4 on macOS 26.6.2, using Homebrew FFmpeg 9.0 with `--enable-videotoolbox`. A direct
    `ffmpeg -c:v h264_videotoolbox` encode produced a probeable H.264 MP4, then oca gained the
    persisted `VideoToolbox` preference and macOS-only automatic candidate. The focused native
    integration test, `cargo test -p avbridge --test encode_test
    videotoolbox_encodes_a_real_timeline_on_macos -- --nocapture`, completed a timeline export
    and printed `oca: opened video encoder h264_videotoolbox (hardware)`. This proves the
    application's own FFI/filter/mux path used Apple hardware acceleration, not merely the
    standalone CLI. The Homebrew build lacks `libopenh264`, so CPU-fallback testing is not
    meaningful on this machine; distribution builds must still supply it for the CPU fallback.
20. `[x]` GPU usage telemetry — `matrix/performance.md`. No cross-platform reader exists, so
    this went vendor-specific: `avcore::GpuSampler` via `nvml-wrapper` (NVML), which dynamically
    loads `libnvidia-ml.so`/`nvml.dll` at runtime rather than link-time linking against it — a
    machine with no NVIDIA GPU/driver at all (confirmed against this sandbox's own dev machine)
    just gets `None` back from `GpuSampler::new()`, never a build-time requirement. AMD/Intel
    stay out of scope, same single-vendor call the GPU encoder ladder (item 19) already made.
    Positive-path (a real non-`None` sample against actual NVIDIA hardware) stays unverified in
    this sandbox — same category as item 19's own hardware-verification gap.
21. `[~]` Preview support for vignette/glitch/deflicker/3D-LUT/stabilization —
    `matrix/effects-and-color.md`. Confirmed no matching GStreamer element on the dev machine for
    any of the five; a custom-coded element was never attempted (no way to visually verify a
    GStreamer plugin in this sandbox). Instead, **partial**: `avcore::preview_effects` covers
    3D LUT and vignette as CPU-side post-processing of the already-decoded preview frame — same
    pattern `avcore::scopes` established for the waveform/vectorscope overlays. `Lut3D::parse`/
    `load` read the standard `.cube` format with real trilinear interpolation (precise,
    unit-tested, no visual-verification risk); the vignette is a simple radial-falloff
    *approximation*, explicitly not FFmpeg's own cosine-based formula (reproducing that exactly
    from `libavfilter` C source without being able to A/B it visually against export wasn't a
    risk worth taking). `App::pump_preview_frame` applies both to the live preview texture only
    — export is untouched, still the real `lut3d`/`vignette` `avfilter`s.

    **Follow-up**: glitch is covered too now. The "no single well-specified algorithm" objection
    dissolved once actually checked — export's own `glitch_intensity` already resolves to a
    concrete, specific avfilter (`noise=c0s=...:c0f=t:c1s=...:c1f=t:c2s=...:c2f=t`, temporal
    luma/chroma noise, not an RGB-shift/block-displacement "datamosh" look), so this isn't a
    from-scratch judgment call, just mirroring what this codebase's own glitch effect already
    *is*. `avcore::apply_glitch_to_rgba` adds per-pixel, per-channel additive noise (RGB, not a
    luma/chroma split — no YUV conversion available on an already-decoded RGBA frame) via a
    xorshift64* PRNG seeded per pixel from a caller-supplied `seed`; `App::pump_preview_frame`
    passes elapsed wall-clock time (bit-cast to `u64`) as that seed, so the pattern actually
    looks like temporal corruption frame to frame rather than a static grain overlay baked onto
    the image. Still a **preview approximation**, not a reproduction of
    `libavfilter/vf_noise.c`'s own PRNG — same "not bit-exact" caveat vignette already carries.

    **Second follow-up**: deflicker is covered too now. `avcore::DeflickerHistory` keeps a
    caller-owned 5-frame rolling window of mean luma (matching export's own
    `deflicker=mode=am:size=5` window exactly), and `apply_deflicker_to_rgba` shifts each frame's
    R/G/B uniformly by `(rolling average - current mean)` — an additive correction, not
    multiplicative, so a near-black frame doesn't blow up the way a `target/current` gain would.
    `App::pump_preview_frame` resets the history whenever the previewed clip id changes (a new
    `preview_deflicker_history: (Option<u64>, DeflickerHistory)` field on `PreviewState`), the
    same "one clip's temporal state must never leak into the next" concern a seek/clip-change
    could otherwise create. Verified for real this time, not just type-checked: `core`'s test
    binary links and runs fully on this dev machine (the ONNX/whisper-linking gap other notes in
    this doc describe is specific to network-restricted Linux sandboxes, not this environment) —
    `cargo test -p core --lib preview_effects` passes all 25 tests (18 original + 7 new for
    deflicker: no-op on the first frame, darkens-a-brighter-than-history frame, brightens-a-
    dimmer-than-history frame, the 5-frame window cap, byte-bounds safety, empty-buffer no-op).
    `cargo test -p ui` (418/418) confirms the `PreviewState`/`pump_preview_frame` wiring compiles
    and the rest of the suite is unaffected. **Explicitly still not done**: stabilization —
    motion estimation between frames (optical flow or equivalent) is a fundamentally different,
    materially larger problem than a rolling scalar average, with its own seek/scrub edge cases
    (a jump-cut in scrub position shouldn't try to "stabilize" against a now-irrelevant previous
    frame) — not a natural extension of this module's per-frame or simple-rolling-window shape.

    Verified via a real-execution scratch crate for the earlier glitch follow-up
    (`preview_effects.rs` has zero heavy deps) — 19 tests (12 original + 7 new for glitch: no-op
    at zero intensity, empty-buffer no-op, alpha untouched, byte-bounds safety at full intensity,
    determinism for a fixed seed, divergence across seeds, and an actual-perturbation sanity
    check), one of which caught a real bug in a *test's own* expected value (a coarse 2-point LUT
    interpolates rather than reproducing the exact original channel value) before it could pass
    silently.
22. `[x]` Smart bins (rule-based media-pool auto-organization) — real in DaVinci Resolve, but
    lower priority for a small/single-editor workflow than for a studio pipeline. The one P4 item
    tractable in this sandbox without special hardware or a missing GStreamer element (unlike 19-
    21) — pure filtering over `Project::media_library`, no `avbridge`/GStreamer/GPU dependency.
    `avcore::SmartBin` (`kind_filter`/`name_contains`/`requires_audio`, every set criterion
    ANDed) + `Project::add_smart_bin`/`remove_smart_bin`/`smart_bin_mut`. Membership isn't
    stored — `SmartBin::matches` is evaluated fresh against the live `media_library` every time,
    same "recompute, don't cache" shape `Marker`/`MulticamGroup` already follow. Editor Library
    panel: a filter-chip row above the asset list ("All" + one chip per bin, click-to-select/
    double-click-to-edit) plus "+ New Bin", backed by a create/edit modal (name, kind Any/Video/
    Audio, file-name-contains text, has-audio Either/Yes/No, Save/Cancel/Delete). Verified via a
    real-execution scratch crate (same ONNX-link-gap workaround as multicam) — 5 passing tests on
    `SmartBin::matches`.
27. `[x]` Clip/track color labels — `matrix/competitor-parity.md`'s 2026-08-27 update. Present
    in Premiere (clip), DaVinci Resolve (clip *and* track), FCP (clip). The cheapest gap in
    that update: pure data (`ClipInstance::color_label`/`Track::color_label`, `Option<[u8;3]>`)
    + timeline-widget rendering, no `avbridge`/GStreamer work — same cost tier as `Marker`/
    `SmartBin`, both already shipped. A fixed 6-swatch palette (matching Premiere/DaVinci/FCP's
    own fixed-palette convention, not a free color picker) offered via a right-click context
    menu on a timeline clip or the track-header name; clears via a "Limpar rótulo" entry.
    Overrides the clip/track's usual kind-based fill color when set. Carried across
    `Track::split_clip_at` (both halves keep the label, same as `transition_in`). Deliberately
    excluded from `ClipFormatting` — an organizational tag, not a rendering style, same
    reasoning `background_removal_mask_path` is excluded for a different reason.
28. `[x]` Detach/unlink audio from a clip (the mechanical precondition for J-cuts/L-cuts) —
    `matrix/competitor-parity.md`. `App::detach_audio_from_selected_clip` mutes the video
    clip's own audio (`gain_db` set to `GAIN_DB_RANGE`'s floor — no separate "muted" flag exists,
    so muting reuses the existing gain primitive as scoped) and places a new clip on an Audio
    track pointing at the same asset, with the same trim range and timeline placement, at unity
    gain — both then independently trimmable. Reused a factored-out `default_clip_instance`
    helper (previously duplicated between `add_asset_to_timeline`/`add_asset_to_timeline_at`)
    rather than adding a third copy. No new render/preview pipeline work — per-track independent
    clips already mix correctly. Triggered via a "Destacar áudio" entry in the timeline clip's
    context menu, enabled only for Video-track clips.
29. `[x]` Speed ramping — `App::apply_speed_ramp_to_selected_clip` ships a **stepped**
    approximation, not the smooth continuous curve CapCut/Premiere/DaVinci/FCP all have. A
    deliberate scope decision (raised to and confirmed by the user, 2026-08-27): the smooth
    version needs the export-side `setpts` filter's output PTS to be the *integral* of
    `1/speed` over time, which for a piecewise-linear speed curve has no simple closed form
    (needs a `log()` term per segment) — a real, easy-to-get-subtly-wrong derivation with no
    way to render/verify it in this sandbox (no decode capability), unlike every other
    `Keyframe<T>`-reusing item in this list. Shipped instead: splits the selected clip into N
    equal-timeline-duration pieces (reusing the already-correct, already-tested
    `Track::split_clip_at`, unchanged) and assigns each piece a constant `speed_factor`
    linearly interpolated between a start/end speed (reusing the existing field, unchanged) —
    a real, visible "staircase" speed ramp built entirely from primitives that were already
    correct before this item, with zero new avfilter/geq/setpts math to get wrong. Since
    `duration_secs()` depends on `speed_factor`, each piece's `start_secs` is reflowed left to
    right after the speed assignment so the pieces stay contiguous. Triggered via a "Rampa de
    velocidade" submenu in the timeline clip's context menu, with two fixed presets (0.5x→2x
    slow-to-fast, 2x→0.5x fast-to-slow, 4 steps each) rather than a custom-curve dialog — also
    deliberately out of scope for this pass.

    **Follow-up**: a custom start/end speed and step count dialog also ships now, via a
    "Personalizada…" entry in the same context-menu submenu (`App::speed_ramp_dialog` staged
    state, `App::show_speed_ramp_modal`, same `egui::Modal` pattern as `renaming_project`'s own
    dialog). UI-only — `App::apply_speed_ramp_to_selected_clip` itself already accepted
    `start_speed`/`end_speed`/`steps` as parameters before this, so no new avfilter/geq/setpts
    math was needed, none of the sandbox-verification risk the smooth-curve version has.

    **Second follow-up: the smooth continuous-curve version now ships too**, and — unlike when
    this item was first scoped — this development environment turned out to have a working,
    fully-linked FFmpeg build capable of actually rendering and verifying the derivation, not
    just type-checking it. `ClipInstance::speed_ramp_end_factor: Option<f32>` (`None` = plain
    constant `speed_factor`, unchanged) carries the ramp's end speed directly on the clip — no
    splitting needed at all, unlike the stepped mode.
    `keyframe::smooth_speed_ramp_duration_secs`/`smooth_speed_ramp_source_secs_at` solve the
    closed-form integral (`∫ 1/speed(t) dt` for a linear `speed(t)` — a natural-log term) and its
    inverse; `ClipInstance::duration_secs()` and `Track::split_clip_at` (splitting a ramped clip
    now continues the ramp correctly across both halves, sharing the exact speed-at-the-cut
    boundary) both use them. Crosses the FFI boundary as
    `avbridge::ClipSegment::smooth_speed_ramp_end_factor` (`<=0.0` = no ramp, same sentinel
    convention `speed_factor` itself already uses elsewhere); `timeline_export.c`/
    `timeline_export_multi.c` each gained a `build_setpts_str` helper emitting
    `setpts=(K/TB)*log((V0+B*T)/V0)` instead of the old constant `setpts=PTS/speed`, expressed
    entirely in the filter's own runtime `TB`/`T` variables so it's valid regardless of the
    actual stream timebase. Audio has no continuous per-sample tempo-ramp primitive in this
    FFmpeg build (`atempo` takes one fixed parameter, not a `t`-keyed expression) — both files'
    audio tempo command uses the ramp's *average* speed instead, a deliberate, documented
    approximation (exact av-sync isn't achievable here; staying audible and reasonably close
    throughout beats jumping between discrete steps). `timeline_export_multi.c`'s per-frame
    track-0-to-timeline time mapping (`ftl`, used to decide which overlay track is active "right
    now," and progress-callback reporting) is also ramp-aware now
    (`smooth_speed_ramp_timeline_elapsed`); an *overlay*-track clip's own ramp still uses the
    old linear approximation for its end-time/seek math — a real, narrower, documented remaining
    gap (only matters when a ramped clip sits on an overlay track specifically, not the common
    single/background-track case). UI: the existing custom speed-ramp dialog gained a "smooth,
    continuous curve" checkbox — checked calls the new
    `App::apply_smooth_speed_ramp_to_selected_clip` (which just sets the two fields directly, no
    splitting) instead of the stepped path, and hides the now-irrelevant step-count field.
    **Verified for real, not just type-checked**: `keyframe::smooth_speed_ramp_duration_secs`
    cross-checked against brute-force numerical integration (200,000 steps) to a `1e-4` tolerance,
    plus an exact round-trip test through its own inverse function — both run for real in this
    session (`cargo test -p core`, 230+ passing, this machine's FFmpeg/GStreamer toolchain
    actually links here, unlike the more constrained sandbox earlier `ROADMAP.md` entries
    describe); a new `Track::split_clip_at` test confirms ramp continuity across a real split;
    and — the strongest evidence — a new `avbridge` integration test
    (`smooth_speed_ramp_produces_a_real_export_with_the_predicted_duration`) actually encodes a
    ramped segment through the real linked FFmpeg build and asserts the *exported file's own
    probed duration* matches the closed-form prediction to within a loose bound — proof the
    whole Rust-math-to-C-setpts-expression-to-real-decoder pipeline is correct end to end, not
    just that the strings happen to parse.
30. `[x]` Real-time audio level meter (VU/peak) during playback — `matrix/competitor-parity.md`.
    Present in Premiere (VU meters) and DaVinci (Fairlight LUFS/peak meter). A pad probe on the
    preview audio path (same pattern as the existing keyframe pad-probes, reading instead of
    writing): `Preview::build_metering_audio_sink` wraps the real audio-sink element (both the
    single-clip `playbin` path and the manually-built compositor audio-mix path) in a small
    `audioconvert!capsfilter(F32LE)!sink` bin, forcing a known sample format so a buffer probe
    on the capsfilter's src pad can parse raw f32 bytes directly and compute peak/RMS combined
    across every channel (a flat sequence, not per-channel — matches this item's "small meter
    widget" scope, not a full per-channel Fairlight-style meter). Stored in a shared
    `Arc<Mutex<AudioLevel>>` updated from GStreamer's own streaming thread, read from the UI
    thread via `Preview::current_audio_level()`. UI: a small peak/RMS bar in the Editor preview
    panel's transport row, next to the scopes toggle, with the peak marker turning red above
    0.98 amplitude to flag near-clipping. Verified via a scratch-crate real-execution check
    (this sandbox's `core` test binary can't link — missing `libonnxruntime`): the metering
    probe logic, run against a real audio fixture through a real GStreamer `playbin`, observed
    real nonzero peak/RMS from actual decoded samples — audio decode/preroll works in this
    sandbox, unlike video decode (confirmed separately: the same harness against a video
    fixture failed with a missing-decoder-plugin error, the known pre-existing video-decode gap,
    not a metering bug).
31. `[x]` Audio gain keyframes (volume fade/ramp within one clip, not just a constant
    `gain_db`) — found while surveying what else the existing `Keyframe<T>` infrastructure
    could drive. Same shape as position/scale/rotation/opacity: `gain_keyframes: Vec<Keyframe<
    f32>>` on `ClipInstance`, overriding the constant `gain_db` when non-empty. Export-side,
    FFmpeg's `volume` filter's `eval=frame` expression mode evaluates to a *linear* multiplier
    (not dB), so the dB-space keyframe curve needs `pow(10, X/20)` wrapping before being handed
    to `volume=<expr>:eval=frame` — verified against FFmpeg's own filter docs, not assumed.
    Crosses the `avbridge` FFI boundary (a new expression-string field on `AudioSegment`/
    `RawAudioSegment`, and an `audio_mix.c` branch alongside the existing literal-`%.6fdB` path).
    Verified: `gain_filter_db_expr`'s unit tests run for real in a scratch crate; a real
    `avbridge` integration test exercises the new `av_asprintf`-built `volume=<expr>:eval=frame`
    path end-to-end against a real FFmpeg filter graph (`avfilter_graph_config` succeeding is
    proof the expression syntax is valid, not just that the C compiles).
32. `[x]` Color grading keyframes (brightness/contrast/saturation ramping over a clip, not a
    constant value) — `ClipInstance::brightness_keyframes`/`contrast_keyframes`/
    `saturation_keyframes`, each independently overriding its own constant field when non-empty
    (same relationship `gain_keyframes` has with `gain_db`). `keyframe::color_balance_filter_expr`
    builds the combined `eq=brightness=...:contrast=...:saturation=...[:eval=frame]` stage —
    `eval=frame` only appended when at least one axis is actually animated, each un-animated axis
    still using its own plain constant. Spliced into `keyframe_video_filter_chain` (alongside
    scale/rotation/opacity) rather than `video_filter_chain`'s own static `eq` stage, which is
    now suppressed whenever any color-grading keyframe list is non-empty (`has_color_keyframes`)
    to avoid double-emitting. **Known caveat, documented in code**: this moves the animated `eq`
    stage to the *front* of the per-clip filter chain instead of its usual post-crop/deflicker/
    stabilization spot — a clip combining color-grading keyframes with crop/deflicker/
    stabilization sees color grading applied to the pre-crop/pre-deflicker/pre-stabilization
    frame. Preview-side piggybacking on `Preview::set_live_balance` (item 3's follow-up) is not
    done — export only, same "export first" shape every other keyframe field started with.
    Verified: `color_balance_filter_expr`'s 3 new unit tests run for real in the same
    `keyframe.rs` scratch crate as `gain_filter_db_expr`'s (32/32 passing); new `video_filter_chain`/
    `split_clip_at` tests in `timeline_test.rs`.
33. `[x]` Crop/pan keyframes (`crop_x`/`crop_y`/`crop_w`/`crop_h` animated over a clip, e.g. a
    slow pan/reveal independent of the existing `scale_keyframes` symmetric zoom) —
    `ClipInstance::crop_x_keyframes`/`crop_y_keyframes`/`crop_w_keyframes`/`crop_h_keyframes`,
    each independently overriding its own constant field when non-empty. `keyframe::
    crop_filter_expr` generalizes `scale_filter_expr`'s single-axis `geq`-based per-pixel inverse
    sample (chosen over `crop`+`eval=frame` for the same real heap-corruption reason, see
    CLAUDE.md) to four independent axes — the sampled window's x/y/width/height — so a moving/
    resizing crop rectangle animates without the frame's own resolution changing frame-to-frame.
    Spliced into `keyframe_video_filter_chain` *first* (before scale/rotation/opacity/color-
    balance — crop reframes the source before those geometric/color stages operate on it, mirroring
    the static `crop` stage's own traditional first-in-chain position) rather than `video_filter_
    chain`'s own static `crop` stage, which `has_crop_keyframes()` now suppresses to avoid
    double-emitting. Wired into `Track::split_clip_at`. Not yet wired into live preview.
    Verified: `crop_filter_expr`'s 3 new unit tests run for real in the same `keyframe.rs`
    scratch crate as the other keyframe expression builders' (35/35 passing); new
    `video_filter_chain`/`split_clip_at` tests in `timeline_test.rs`.
34. `[x]` Text/shape clip animation keyframes — `TextClip`/`ShapeClip` had *zero* keyframe fields
    (position/scale/rotation/opacity keyframes only existed on `ClipInstance` before this), so
    this was a structural gap, not a one-field addition. **Partial, now covers all of
    `ShapeClip`**: `center_x_keyframes`/`center_y_keyframes` (position) and, in a follow-up
    pass, `width_keyframes`/`height_keyframes`/`rotation_keyframes` (size/rotation) all ship —
    `ShapeClip`'s export path (a self-contained `geq` filter expression built entirely in Rust,
    `crate::shape_render`) turned out to already support a `T`-keyed per-pixel expression with
    zero FFI/C changes (`geq` natively exposes `T`, elapsed seconds, per pixel — confirmed
    against FFmpeg's own filter docs, not assumed). `keyframe::shape_axis_expr` offsets `T` by
    the shape's own `start_secs` (the overlay is composited onto the already-exported full video
    in a post-pass, not inside a per-clip filter chain with its own PTS reset, so `T` is
    timeline-*absolute*, unlike every other `*_filter_expr` builder in this module — a real,
    documented difference). The size/rotation follow-up reworked `shape_render::inside_expr`'s
    ellipse/polygon geometry math to accept `geq`-expression-language sub-expressions for the
    half-extents instead of literal `f64`s — the multiply-through-avoid-division trick the
    static case always used generalizes verbatim, so an unkeyframed width/height still
    degenerates to the same plain numeric literal as before. Rotation swaps the old
    Rust-precomputed `sin_a`/`cos_a` literals for `geq`'s own `sin()`/`cos()`/`PI`
    expression-language functions (confirmed present via FFmpeg's `eval.c`-backed docs, not
    assumed), evaluated per pixel instead of once; a scratch-crate numeric check confirmed the
    new per-pixel formula produces the same sin/cos values the old precomputed literals did, at
    several angles, so this is not a behavior change in the unkeyframed (still the common) case.
    Verified: `shape_axis_expr`'s unit tests and `build_shape_filter_desc`/`inside_expr`
    animation tests (position, then size/rotation) all run for real in a scratch crate
    (`keyframe.rs`+`timeline.rs`+`shape_render.rs` have zero heavy deps) — 58/58 passing,
    including every pre-existing `shape_render` test (confirms the already-visually-verified
    static geometry math is unchanged).

    **`TextClip` opacity keyframes also ship**, in a further follow-up: `TextClip`'s export path
    pre-rasterizes a full-canvas PNG per segment with position baked in at generation time, not a
    moving overlay — so position/scale animation stays out of scope (would mean restructuring
    that pipeline toward a small sprite + `overlay=x=<expr>:y=<expr>`, a materially bigger lift).
    Opacity is different: the raster already carries a real alpha channel, so a fade is just an
    alpha *multiplier* applied to the existing pixels, no rasterization change needed.
    `keyframe::text_opacity_alpha_expr` builds the same `T`-keyed-offset-by-`start_secs`
    expression `shape_axis_expr` does; `avbridge_apply_text_overlays` splices a `geq` alpha
    stage between the movie source and the overlay node when a segment's
    `opacity_keyframe_expr` is non-empty. **Real, non-obvious finding from testing this against
    this project's actual linked FFmpeg build** (`avbridge/tests/text_overlay_test.rs`, not just
    reasoning from docs): `geq`'s alpha read-back function is spelled `alpha(X,Y)`, not `a(X,Y)`
    as FFmpeg's own docs otherwise imply — `a(X,Y)` parses as "Unknown function" against the
    real library. A `colorchannelmixer=aa=<expr>:eval=frame` alternative (no per-pixel read-back
    needed at all) was tried first and ruled out the same way: that filter has no `eval` option
    in this build. The full encode still fails in this sandbox with a `Pipeline` error — but
    that reproduces identically on the *unmodified* (no-fade) code path too, confirming it's the
    same pre-existing encoder-availability gap `CLAUDE.md` already documents elsewhere in this
    codebase, not something this change introduced.

    **`TextClip` position keyframes also ship**, in a further follow-up — without the sprite-
    cropping restructuring described above turning out to be necessary. Instead of shrinking the
    raster to a tight sprite, the raster still bakes one constant anchor (the first keyframe's
    value when `pos_x_keyframes`/`pos_y_keyframes` are present, `pos_x`/`pos_y` otherwise — same
    "keyframes win when present" convention `ShapeClip`'s own fields use) exactly as before, and
    `keyframe::text_position_offset_expr` builds the *delta* from that anchor, in canvas pixels,
    as an `overlay=x=<expr>:y=<expr>` fragment — `overlay`'s own expression evaluator uses
    lowercase `t` where `geq`'s (used by the opacity fade above) uses uppercase `T`, same
    timeline-absolute clock, different filter's own variable name. Each axis independently
    overrides its own constant, same as `ShapeClip::center_x_keyframes`/`center_y_keyframes` (two
    separate `f32` lists, not one `Position`-typed list, since this is an absolute `0.0..=1.0`
    anchor fraction, not `ClipInstance::position_keyframes`' `-1.0..=1.0` pan-offset convention).
    Verified: `text_position_offset_expr`'s unit tests run for real in a scratch crate
    (`keyframe.rs` alone has zero heavy deps) — 47/47 passing, including every pre-existing
    `keyframe` test. `avbridge/tests/text_overlay_test.rs` gained a real-FFmpeg-build test for the
    new `overlay=x='...':y='...'` syntax too — same `Err(Pipeline)` encoder-availability gap as
    the opacity test in this sandbox (confirmed by temporarily no-op-stubbing the unrelated
    `av_opt_set_array` symbol this file also can't link against here, per `CLAUDE.md`'s filters.c
    note, discarded before commit), but critically *not* `Err(FilterGraph)` — confirming the new
    overlay x/y expression syntax itself parses against the real linked `avfilter_graph_parse_ptr`,
    identically to the pre-existing opacity/no-fade tests it was verified alongside.

    **`TextClip` scale keyframes also ship**, in a further follow-up. Unlike position, this needed
    no raster-baking change at all: `keyframe::text_scale_sample_exprs` builds a `geq`
    inverse-sample remap (output pixel `(X,Y)` reads the raster's own pixels back from
    `anchor + (X-anchor)/scale(T)` instead of `(X,Y)`) around the same raster-baked position
    anchor position keyframes already use — an inverse *zoom*, not a literal `scale` filter with
    `eval=frame`: letting `scale` renegotiate its own output size every frame is the exact thing
    that reliably corrupted the heap in a real export elsewhere in this codebase (`CLAUDE.md`,
    `timeline_export.c`) — the same reasoning `ClipInstance::scale_keyframes`' own `scale_filter_
    expr` already applies, now reused for `TextClip`. Because the geq remap keeps the raster's own
    canvas-sized frame unchanged throughout, position and scale keyframes compose independently
    with no interaction term needed. `avbridge`'s `text_overlay.c` filter-chain builder was
    restructured from a 2-branch (fade/no-fade) split into an incremental N-stage chain (movie
    source → optional scale-remap geq → optional opacity-fade geq → overlay), extensible for a
    future rotation stage. Verified: `text_scale_sample_exprs`' 6 new unit tests run for real in
    the same `keyframe.rs` scratch crate as the other keyframe expression builders' (53/53
    passing). `avbridge/tests/text_overlay_test.rs` gained a real-FFmpeg-build test for the new
    `geq=r='r(...)':g='g(...)':b='b(...)':a='alpha(...)'` remap syntax; linking that specific test
    binary hits a separate pre-existing sandbox gap (`av_opt_set_array`, a symbol newer than this
    sandbox's packaged FFmpeg — same category as, but distinct from, `filters.c`'s own gap) so it
    could not be executed here, though `cargo check`/`clippy` for the whole workspace stayed clean.

    **`TextClip` rotation keyframes also ship**, completing this item's `TextClip` scope
    (opacity/position/scale/rotation all now animate). `keyframe::text_rotation_sample_exprs`
    builds a `geq` inverse-sample remap around the same raster-baked anchor scale uses, via
    FFmpeg's `sin()`/`cos()` expression-language functions — the same inverse-rotation math
    `shape_render`'s own `ShapeClip` rotation already relies on, just re-added onto the anchor
    instead of compared against half-extents for an inside test. Composes independently of the
    scale remap around that shared anchor (an isotropic scale and a rotation around the same
    center commute algebraically), so the two geq stages' relative order in the filter chain
    doesn't affect the result. Verified: `text_rotation_sample_exprs`' 6 new unit tests run for
    real in the same `keyframe.rs` scratch crate as the other keyframe expression builders'
    (59/59 passing). `avbridge/tests/text_overlay_test.rs` gained a real-FFmpeg-build test for
    the new rotation `geq` remap syntax; same pre-existing `av_opt_set_array` link-time sandbox
    gap as the scale test above kept it from actually running here, though `cargo check`/
    `clippy` for the whole workspace stayed clean.

    The sprite-cropping restructuring floated as a future possibility above (shrinking the
    raster to a tight sprite instead of a full-canvas-sized one) remains genuinely not needed —
    every axis (opacity, position, scale, rotation) shipped without it, purely via geq inverse-
    sampling and overlay-expression deltas — and stays a possible future performance
    optimization rather than a functional gap.

35. `[~]` **Nested sequences / compound clips** (Premiere/DaVinci/FCP) — the scoping pass
    `matrix/competitor-parity.md`'s own note called for. The key insight that made this tractable:
    oca already has independently-editable `Sequence`s (the Editor's own tabs) — a compound clip
    just points a `ClipInstance` at another `Sequence` in the same `Project`
    (`ClipInstance::nested_sequence_id: Option<u64>`) instead of inventing a second sub-timeline
    concept.
    - **Real recursion, not a bolt-on pointer** (the concern this item's original scoping note
      raised): `avcore::nested_sequence::materialize_nested_sequences` actually renders each
      nested sequence to a cached temp file (recursively — a nested sequence's own clips may
      themselves be nested, with cycle detection via a `visiting` set), then hands back a
      synthetic `MediaAsset` per nested clip pointing at that file. Every existing resolution
      function (`resolve_timeline_segments_multi` and friends) then treats a compound clip
      exactly like an ordinary asset-backed one — zero changes to their own logic, and every
      per-clip effect/keyframe still applies on top of the rendered nested content for free,
      since it's the same `ClipInstance`.
    - **Caching**: keyed on the nested sequence's own `Timeline` content (`PartialEq`, no
      hashing) via `ui`'s `App::nested_sequence_render_cache` — mirrors `ExportPreviewCache`'s
      own "value-equality, not a version counter" pattern. `ExportPreviewCache` itself now also
      compares every *other* sequence (not just the active one), since a compound clip's
      rendered content depends on a sequence `tracks`/`media_library` alone can't see edits to.
    - **UI**: "📦 Criar clipe composto" (timeline clip context menu, Video-track clips only —
      see the later follow-up below for same-track multi-selection support) moves the selected
      clip into a fresh `Sequence`'s own new V1 track
      (rebased to start at `0.0`) and replaces it in place with a plain nested-sequence clip.
      Double-click (or "📦 Abrir clipe composto") switches the Editor's active tab into the
      nested sequence — the common "enter the compound clip" affordance every NLE with this
      feature has. The timeline block itself shows the nested sequence's own name (📦 badge) in
      place of a filmstrip/waveform, since a compound clip has no `asset_id`/media-library entry
      to draw one from.
    - **Follow-up: the UI-thread-blocking cost is gone.** `App::materialize_nested_sequences_for_active_sequence`
      no longer calls `avcore::nested_sequence::materialize_nested_sequences` synchronously —
      the actual FFmpeg re-encode now runs on a background thread (`NestedSequenceRenderState`,
      the same `std::thread::spawn` + channel + `pump_*` pattern every other background job in
      this codebase already uses, e.g. `MotionTrackingState`), and the method always returns
      immediately: the last successfully-materialized result for the active sequence (empty
      before the first render ever completes), while dispatching a fresh render only when
      `Project::sequences` has actually changed since the input that produced that cached result
      (compared wholesale, not just the active sequence's own timeline — a compound clip's
      rendered content depends on whatever *other* sequence it points at, same reasoning
      `ExportPreviewCache` already uses) and no render for that sequence id is already in flight.
      A persistently broken nested-sequence reference (a real cycle, a deleted sequence) latches
      its failed input too, so it doesn't get redispatched to a fresh thread every single frame
      forever. Verified with 5 new `cargo test -p ui` cases (423/423 passing) covering the
      no-nested-clips fast path, the unchanged-input cache hit, dispatch-on-a-real-nested-clip,
      and both `Ready`/`Failed` event merge paths — the underlying `avcore::nested_sequence`
      module itself is untouched (already verified per this item's own earlier write-up above),
      this only changes when/where it's called from.
    - **Follow-up: live preview now works too.** `App::current_preview_clip`/
      `current_preview_overlay_clips`/`current_preview_audio_clips` now take an explicit
      `media_library` slice instead of reading `Project::media_library` directly, so
      `App::ensure_preview_loaded` can merge in
      `materialize_nested_sequences_for_active_sequence`'s synthetic assets before resolving —
      same cache-backed pattern the export path already uses, so scrubbing/playback across a
      compound clip only pays the render cost once per edit to it. Fixed a real bug found while
      wiring this in: `ensure_preview_loaded`'s own cheap per-frame "did anything change" fast
      path (`current_preview_clip_id`/`current_preview_overlay_clip_ids`) still required an
      `asset_id` to resolve in the *plain* `media_library` even for a nested clip (which has no
      real `asset_id` at all) — would have made the fast path see "still unresolved, nothing
      changed" forever and never actually attempt to open a compound clip's pipeline. Two call
      sites that only ever needed clip-level fields (`frozen`, `speed_factor`, `id`), never the
      asset — `toggle_preview_playback`, `seek_preview` — were switched to a new asset-free
      `current_preview_video_clip` instead of threading `media_library` through them for no
      reason.
    - **Follow-up: inserting an existing sequence as a compound clip now ships too**, closing
      most of the "explicitly not done" gap this note originally flagged (a real drag-and-drop
      gesture for a sequence tab onto the timeline panel itself remains out of scope — no
      existing drag source exists for a tab today — but the actual outcome, an existing sequence
      landing as a compound clip, is now reachable). `App::insert_sequence_as_compound_clip`
      appends a new nested-sequence `ClipInstance` onto the active sequence's first `Video`
      track (auto-creating one if none exists), right after whatever's already there — the same
      "append" placement `App::add_asset_to_timeline` already uses for a double-clicked
      media-library asset, reused here since this insertion path has no drag position of its own
      to place a clip at. The new clip's box duration comes from `avcore::timeline::Timeline::
      duration_secs` (an already-existing pure function — the nested sequence's own edited
      length), not a real FFmpeg render/probe up front, matching how
      `create_compound_clip_from_selected_clip` already treats a compound clip's box length as a
      cheap timeline-derived value; the real render still only happens lazily in the background
      the first time the clip is actually previewed or exported. A no-op if the target sequence
      doesn't exist, is the active sequence itself (the trivial direct self-nesting cycle — a
      deeper indirect cycle is instead caught gracefully at render/materialize time by
      `avcore::nested_sequence`'s own existing cycle detection, the same path every other nested
      clip already goes through, so this insertion path doesn't need to duplicate that check), or
      is itself empty. Reachable from a new "📦 Insert sequence as compound clip" submenu in the
      Editor's Sequence menu, listing every *other* sequence in the project by name.

      Verified: 7 new `App`-level tests in `app_test.rs` — appends a nested clip with the target
      sequence's own duration, appends after an existing clip on the first video track rather
      than overwriting it, auto-creates a video track when none exists, refuses the active
      sequence itself, refuses a nonexistent sequence id, refuses an empty nested sequence, and
      pushes exactly one undo snapshot. `cargo check --workspace --all-targets` and `cargo clippy
      -p ui --tests --no-deps` (via the documented temporary `filters.c`/`text_overlay.c` shim,
      discarded before commit) and `cargo fmt --all -- --check` all stayed clean — this session's
      sandbox can't *link* `ui`'s own test binary (the same pre-existing ONNX Runtime network gap
      `CLAUDE.md` documents, confirmed directly this pass), so these are type-checked, not run,
      same caveat every other `ui`-side slice this session has hit.
    - **Follow-up: delete-time warning now ships.** Deleting a `Sequence` still referenced by a
      compound clip elsewhere used to leave a dangling `nested_sequence_id` with no warning at
      all — `RenderError::MissingNestedSequence` degraded gracefully (logged, clip skipped)
      rather than crashing, but only surfaced much later, at render time. `Project::
      sequences_referencing_as_compound_clip` (new `core` method, 4 integration tests) scans
      every other sequence's clips for a reference to the one being deleted; the delete-sequence
      confirmation modal calls it fresh each time it's shown and, if any exist, adds a warning
      line naming them. Deletion itself is unchanged — still allowed after the warning, not a
      hard block.
    - **Verified for real**: `avcore::nested_sequence`'s own test module — cycle detection (both
      direct self-nesting and an indirect A→B→A cycle), a missing-sequence error, and, the
      strongest evidence, an actual end-to-end test that builds a two-sequence project, calls
      `materialize_nested_sequences`, and confirms the rendered output file really exists and
      probes as valid video (a real recursive FFmpeg encode, not just type-checked) — plus a
      cache-reuse test confirming an unchanged nested timeline returns the same cached path
      rather than re-rendering. All run for real in this session (`cargo test -p core`, this
      machine's FFmpeg/GStreamer toolchain actually links here).
    - **Follow-up: same-track multi-selection compounding now ships**, closing this item's own
      "multi-selection/composite-group compounding isn't supported yet" scope cut for the
      multi-selection half. `App::create_compound_clip_from_selected_clip` now checks the
      current ctrl-click multi-selection (`App::multi_selected_clip_ids`) first: if it has more
      than one member, includes the right-clicked clip, and every member sits on that clip's own
      track with no `nested_sequence_id` already — the unambiguous case, one nested track, no
      z-order/flattening decision to make — the whole selection compounds together instead of
      just the one clip. Each member rebases relative to the *earliest* member's own
      `start_secs` (not `0.0` individually), so relative gaps between members survive inside the
      nested timeline; the single wrapper clip spans from the earliest member's start through
      the latest member's own end. A multi-selection spanning more than one track still falls
      back to compounding just the single right-clicked clip (silently, not an error) — a real
      composite-group (`composite_id`) compound routinely pairs a video clip with an audio one on
      a different track, which needs the same multi-track nested handling this pass deliberately
      doesn't attempt; left as a further scope cut, not an oversight. 3 new `app_test.rs` cases
      (single-clip regression, same-track multi-selection collapsing into one wrapper with the
      gap preserved, cross-track fallback touching only the right-clicked clip's own track).
      `cargo check --workspace --all-targets` and `cargo clippy --workspace --all-targets` (via
      the documented temporary `filters.c`/`text_overlay.c` shim, discarded before commit) and
      `cargo fmt --all -- --check` all stayed clean — this sandbox's `ui` test binary still can't
      *link* (the pre-existing ONNX Runtime gap), so the new tests are type-checked, not run,
      same caveat every other `App`-level test this session has hit.

## P5 — Competitive Product Growth

The consolidated 2026-08-28 survey found that oca's largest remaining gaps are workflow and
interchange gaps, not basic timeline tools. Read
[architecture/competitive-feature-plan.md](architecture/competitive-feature-plan.md) for the
competitive evidence, implementation slices, acceptance criteria, security requirements, and
deliberate non-goals. Implement in this order unless an active production problem justifies moving
an item earlier:

- `[~]` **MON-01: Free/Pro monthly subscription.** The product and licensing decision is now
  documented: one useful no-watermark Free edition and one R$39.90/month Pro edition focused on
  automation, local AI, batch workflows, premium content, and support. The workspace metadata,
  source notices, and root license are aligned to GPL-3.0-or-later so the commercial model does not
  contradict the statically linked GPLv3 eSpeak dependency.

  **MON-01B's own local policy engine (its "define stable feature IDs and one central Free/Pro
  capability policy" step) now shipped.** `avcore::entitlement` adds `FeatureId` (one identifier
  per Pro-only capability the doc's own "Oca Pro" feature list names — silence detection,
  correlated highlight detection, auto chapters, Shorts Pack batch, Whisper transcription,
  background removal, auto-reframe, motion tracking, text-to-speech, audio ducking, batch
  loudness consistency, multicam editing, the persistent export queue, collaboration bundles,
  premium content) and `EntitlementState`, mirroring the doc's own "Subscription lifecycle" table
  exactly: `Free`/`Trialing`/`Active`/`Grace`/`PastDue`/`Canceled`/`Expired`/`Revoked`/
  `OfflineExpired`/`Malformed`. `pro_actions_enabled`/`feature_allowed` are pure functions taking
  `now_unix` explicitly rather than reading the system clock, so a caller controls exactly what
  "now" means.

  **Deliberately not wired to gate anything yet.** MON-01C (identity/entitlement service) hasn't
  shipped — there is no real way for a user to become Pro, and no purchase flow to point an
  upgrade prompt at. Wiring this into a real `ui` call site today would silently take
  already-working functionality away from every current user with no way to unlock it back — a
  real regression, not a feature (this exact risk is why the item was scoped this way rather than
  gating anything on the spot). This is the local policy engine only, ready for a `ui`-side gate
  once MON-01C exists to feed it an actual state instead of a hardcoded `EntitlementState::Free`.
  The rest of MON-01B (gating real action call sites at their service boundary, preserving
  project load/export independently of entitlement) remains open until then, as does the rest of
  MON-01 (identity, checkout, provider webhooks, server-authoritative entitlements, secure local
  token storage, downgrade UI, subscription telemetry).

  Verified for real, not just type-checked: `entitlement.rs` depends only on `serde` (no
  `avbridge`/GStreamer/ONNX), so it was copied unmodified into a throwaway scratch crate and
  `cargo test`ed there for real: 15/15 passing — every state's own enabled/disabled behavior per
  the doc's table (including both sides of `Trialing`/`Grace`/`Canceled`'s own time-bound
  transitions), every `FeatureId` agreeing with `feature_allowed`, and a clock-rollback-adjacent
  case confirming a deadline is never silently extended by anything inside the pure function
  itself. `cargo check --workspace --all-targets` and `cargo clippy -p core --lib --no-deps` (via
  the documented temporary `filters.c` shim, discarded before commit) and `cargo fmt --check` all
  stayed clean. Read
  [architecture/monetization-and-licensing.md](architecture/monetization-and-licensing.md).
- `[~]` **ER-01: client error reporting.** Add consent-based, sanitized, bounded remote reporting
  for handled errors and Rust panics, exact release/symbol management, and a separately validated
  native Crashpad phase. This is an operational prerequisite for broad beta distribution, not a
  replacement for local telemetry. ER-01A (the contract, sanitizer, validator, `NullReporter`,
  queue envelope, and central handled-error wiring in `core`+`ui`) is shipped.

  **ER-01B (steady-state consent + queue + delivery worker) is now shipped too, minus the
  post-crash one-time review offer.** `ui::app::error_reporting` adds: a persisted
  `ErrorReportingConsent` preference (`Disabled`/`AlwaysSend`, defaults disabled — Preferences'
  new "Remote error reporting" section, wired through `App::set_error_reporting_consent` for a
  live no-restart toggle); a bounded on-disk queue next to the rolling `tracing` logs
  (`enqueue_to_disk_in`/`load_queue`/`delete_all_queued`, atomic temp-file-then-rename writes,
  pruned to `avcore`'s existing 20-record/20 MiB/7-day bounds, corrupt/expired entries dropped
  on read rather than retried forever); a background delivery worker thread (same
  `std::thread::spawn`+`blocking_recv` shape `telemetry.rs`'s writer already uses) that persists
  every report *before* attempting delivery — so a mid-delivery crash never loses the record —
  then retries transient failures inline with a small bounded backoff, and re-sweeps whatever's
  still on disk at the start of every launch; and a minimal hand-built Sentry envelope
  (`build_sentry_envelope`, no Sentry SDK dependency) sent over `ureq` behind an `EnvelopeSender`
  trait, so the transport is swappable and was tested for real against a local one-shot HTTP
  mock server (2xx/4xx/5xx branching, header/body content), never the production provider.

  **Real, deliberately scoped-out gap**: no Sentry DSN is actually configured anywhere — the
  ER-01 doc's own "Sentry organization/project ownership, region, retention, budget" open
  decision is still unresolved, so `OCA_SENTRY_DSN` is read at startup and, being unset in every
  build today, the worker still validates and queues every report but never attempts a real
  network call (`DeliveryOutcome::NotConfigured`) — opting in today is inert-but-safe, not yet
  actually connected to a live Sentry project. Releases/symbolication (ER-01C), native capture
  (ER-01D), and operations (ER-01E) also remain.

  **The post-crash "Send once / Always send / Do not send" one-time review offer (ER-01B's
  other half) is now shipped too.** `core::error_reporting` gained `ErrorCode::Panic`/
  `Operation::App` for this report shape. `ui`'s new `crash_review.rs` scans the log directory
  at startup for a `crash_<unix>.txt` (already written by `main.rs`'s panic hook — the "crash
  sentinel detection" this item's own doc previously flagged as the hook point) newer than a
  new persisted `PrefsState::last_reviewed_crash_unix`, parses it back into
  `PendingCrashReview`, and stages it on `App::pending_crash_review` for a startup modal
  (`show_crash_review_modal`, same `egui::Modal` shape `pump_autosave_restore` already
  established) offering the ER-01 doc's exact three choices, with a collapsible section
  showing the precise post-sanitization JSON payload "Send once"/"Always send" would transmit
  — the doc's own "before a one-time send, the user can inspect the exact structured payload"
  requirement, satisfied by factoring the Sentry event body out of `build_sentry_envelope`
  into a standalone `sentry_event_payload` so the preview is literally the same JSON a real
  send builds, not a hand-approximated stand-in. `error_reporting.rs` gained
  `build_crash_report` (folds the crash's location/message/backtrace into
  `sanitized_stack_trace` through the same builder/sanitizer path every other report goes
  through, with `release`/`app_version` overridden to the crashed launch's own recorded
  version rather than this launch's, since an update between the crash and the review would
  otherwise misattribute which build crashed) and `one_shot_reporter` (reachable regardless of
  the persisted steady-state consent, since "Send once" is one explicit action on one specific
  report, never a change to the steady-state preference — only "Always send" touches
  `error_reporting_consent`). Any of the three choices marks the crash reviewed and persists
  that immediately, so the same crash is never re-prompted.

  Verified for real: `crash_review.rs`'s pure file-scanning/parsing logic (11 tests — a real
  crash file found and parsed, the most recent among several, one already reviewed correctly
  excluded, and malformed/unrecognized siblings skipped rather than escalated) ran in a
  throwaway scratch crate against a real filesystem, since this sandbox's `ui` test binary
  still can't *link* (the same pre-existing ONNX/FFmpeg gaps `CLAUDE.md` documents — confirmed
  directly this pass: `cargo test -p ui` fails at the final link step on both `OrtGetApiBase`
  and `av_opt_set_array`, not a code issue). App-level state-transition tests (the payload
  preview matching the built report's real fields, each of the three actions marking the crash
  reviewed and persisting that, only "Always send" flipping the steady-state consent, and all
  three being a no-op with nothing pending) were added directly to `app_test.rs` instead,
  type-checked the same way the rest of that file's App-level tests are in this sandbox.
  `cargo check --workspace --all-targets`, `cargo clippy -p core --lib --no-deps` / `-p ui
  --bin ui --no-deps` / `-p ui --tests --no-deps`, and `cargo fmt --check` all stayed clean
  (via the documented temporary `filters.c` shim, discarded before every commit) — no new
  warnings beyond the same pre-existing baseline. Read
  [architecture/client-error-reporting.md](architecture/client-error-reporting.md).
- `[~]` **FONT-01: expanded built-in font catalog.** Grow the deterministic offline catalog from
  6 to 43 families (51 locked OFL binaries, measured at 17.07 MiB), replace the fixed enum/match
  architecture with stable manifest IDs, support real variable weights, and add a searchable,
  lazy, bounded selector. May proceed alongside CF-01 and must land before CF-07 templates. Read
  [architecture/built-in-font-catalog.md](architecture/built-in-font-catalog.md).

  **FONT-01A slice 1 (manifest + stable IDs for the existing six families) shipped.**
  `avcore::font_catalog` adds a locked `FontFamilyEntry`/`FontFace` manifest — one entry per
  bundled family, carrying the doc's required typed fields (`family_id`, `display_name`,
  `category`, `tags`, `license_id`/`license_path`, `source_kind`, per-face `source_path`/
  `sha256`/`size_bytes`, `default_weight`, `fallback_family_id`, `glyphset_guarantees`) —
  `CATALOG` is static data only, no acquisition/network code, per the doc's "vendoring is a
  maintainer-time, offline operation" rule. `TextFontFamily::family_id`/`from_family_id` give
  the enum a stable ASCII slug (`"lato"`, `"bebas-neue"`, ...) round-tripping losslessly for
  every existing variant, and `from_family_id` returns `None` (not a panic) for an unrecognized
  slug per the doc's forwards-compatibility rule. `validate_static_catalog()` checks the
  manifest's own self-consistency (duplicate ids/paths, malformed slugs/hashes, an unmatched
  `default_weight`, a fallback chain that doesn't resolve or terminate outside `lato`).

  **Deliberately not done in this slice** (the doc's own FONT-01A step 2, "migrate the six
  current enum variants," only half-lands here): `.ocproj` still persists `TextFontFamily` as
  the plain enum variant name, not `family_id` — the doc's "unknown future ID keeps its
  serialized value" requirement needs a data-carrying persisted type (not a plain enum), a
  real structural change with its own migration/golden-image risk deliberately deferred rather
  than folded into this pass.

  **FONT-01B slice 1 shipped**: the exact `google/fonts` vendoring pass this note originally
  said needed network access this sandbox didn't have — a later session did. `CATALOG` grew
  from 6 to 15 families: the doc's full Sans table (Inter, Montserrat, Roboto, Open Sans,
  Poppins, Nunito, Source Sans 3, Barlow, Fredoka — Lato was already bundled). Every binary
  downloaded for real from the pinned `GOOGLE_FONTS_REVISION` commit (not `main`, not a guess —
  the same commit the original six were vendored from), SHA-256/size verified against the exact
  committed bytes, real TTF `sfnt` signature checked. `FontSourceKind::Variable` (added in
  FONT-01A but unused until now) covers the 7 families shipped as one upstream variable-axis
  file each; Poppins/Barlow use the doc's chosen static Regular+Bold subset instead, matching
  its "Do not generate static instances... Use official static files when they exist" rule.
  Total bundled size: 6.6 MiB, well inside the doc's 20 MiB budget. 9 new families have **no
  `TextFontFamily` enum variant yet** (that needs the persisted-identity swap above) but are
  already real, loaded, and byte-verified working: `text_layout.rs`'s cosmic-text engine iterates
  `locked_face_bytes()` generically, not just the six enum-backed families, and
  `loads_exactly_the_locked_catalog_faces_and_nothing_else` (a real `fontdb` parse, not just a
  manifest check) confirmed every one of them loads as a valid font through the real production
  path. **Correction to an earlier draft of this note**: it previously claimed
  `every_bundled_family_shapes_ordinary_latin_text_without_missing_glyphs` had confirmed all 9
  shape Latin text with no missing glyphs — checked again while writing FONT-01B slice 2's own
  note below, and that test actually iterates `TextFontFamily::ALL` (the enum, still only 6
  variants), not the full `CATALOG` — it never touched the 9 new families at all. Real
  per-family byte/parse verification stands; Latin-shaping verification for anything beyond the
  original six does not, and is called out honestly as a gap in slice 2's own note. 18 new
  `cargo test -p core --lib` cases (489/489 total passing, this environment's toolchain fully
  links) cover per-family byte verification plus the catalog's existing generic self-
  consistency/traversal/count checks, which needed no changes to already cover the new entries.

  **FONT-01B slice 2 shipped: the catalog is now complete, all 43 families.** The remaining four
  categories plus the international set — Display/condensed/gaming (Oswald, Anton, Barlow
  Condensed, League Spartan, Teko, Black Ops One, Russo One, Bangers — 8 new, Bebas Neue/Archivo
  Black already bundled), Serif (Merriweather, Libre Baskerville, Lora, Cinzel, Bitter — 5 new),
  Handwritten (Caveat, Pacifico, Dancing Script, Comic Neue, Gloria Hallelujah — 5 new),
  Monospace (JetBrains Mono, Roboto Mono, Space Mono — 3 new), and International (7 `Noto`
  families for Arabic/Hebrew/Devanagari/Bengali/Tamil/Thai) — same pinned
  `GOOGLE_FONTS_REVISION`, same sha256/size verification discipline as slice 1. Added
  `FontCategory::International` (the doc's own "visible under an International category"
  requirement — reusing `Sans` for these would have been a real mislabeling, not a shortcut).
  Total bundled size: 18 MiB (doc's own measurement was 17.07 MiB; close enough to be the same
  file set, the difference is rounding/filesystem overhead) — inside the 20 MiB gate with 2 MiB
  to spare. A new `catalog_has_exactly_43_families_and_51_faces` test locks in the doc's own
  release gate ("exactly 43 visible families and 51 approved source binaries") as a real
  assertion, not just a comment. 28 new dedicated per-family byte tests plus the existing generic
  ones (518/518 `cargo test -p core --lib` passing) — same real, linked-toolchain verification
  slice 1 had, not a type-check-only claim.

  **Honest correction found and fixed mid-slice**: the 7 `Noto` international entries were
  initially tagged `glyphset_guarantees: ["gf-latin-core", "international-fallback"]`, copying
  slice 1's pattern without checking whether Noto's own script-specific faces actually carry
  Latin coverage — they may not (Noto's project explicitly ships separate faces per script, and
  a Latin-coverage claim for e.g. `NotoSansThai` was never verified). Nothing in the codebase
  currently reads `glyphset_guarantees` (confirmed via a real grep — it's descriptive metadata
  only, not consumed by any runtime logic), so this was never a functional bug, just an
  inaccurate claim — fixed to `["international-fallback"]` only, pending real verification.

  **FONT-01B slice 3 shipped: all 37 new families are now selectable, not just vendored.**
  `TextFontFamily` grew from 6 to 43 variants (plain enum variants, the same shape the original
  six use — not the doc's own `family_id`-as-persisted-type structural swap, still deferred, see
  below) and `TextFontFamily::ALL` now lists all 43. Two consequences fell out for free, since
  both already iterate `ALL` generically:
  - The Editor's text-clip font picker (`properties_panel/text_clip.rs`) now lists and lets a
    user select every one of the 43 families — `text_font_family_label` reuses each new family's
    `font_catalog::FontFamilyEntry::display_name` directly rather than a bespoke i18n key per
    family (font names are proper nouns, not conventionally translated — no pt-BR equivalent for
    "Montserrat" — a dedicated key would just repeat the same string in both locales). No
    category grouping in the list yet — that's FONT-01C's own scope, not invented here.
  - `every_bundled_family_shapes_ordinary_latin_text_without_missing_glyphs`, which iterates
    `TextFontFamily::ALL`, now genuinely covers all 43 with zero code changes to the test itself.
    **Retracts slice 2's own "Latin-shaping verification for the 37 new families" gap**: run for
    real, it confirmed every one of them — *including* the 7 `Noto` international faces — shapes
    ordinary Portuguese/Latin text with zero `.notdef` hits. Noto's own faces do carry Latin
    coverage after all (a deliberate design choice by that project for mixed-script documents);
    slice 2's defensive `glyphset_guarantees` downgrade was the right call at the time (no
    verification existed yet) but is now upgraded back to `["gf-latin-core",
    "international-fallback"]` for all 7, this time backed by a real passing test rather than a
    copy-pasted assumption.

  `TextFontFamily::supports_bold()` changed from a hardcoded 3-family match to a real derivation
  from `font_catalog::CATALOG` (does this family's own entry declare a weight-700 face?) — every
  FONT-01B variable-font family locks only its default 400 instance today (no separate Bold
  entry, since one variable file already carries the whole weight axis), so `supports_bold()`
  correctly returns `false` for all of them; only the static two-face families (Poppins, Barlow,
  Barlow Condensed, Comic Neue, Space Mono, plus the original Lato/Playfair Display/Anonymous
  Pro) return `true`. `cargo build -p ui` + a real launch of the compiled `ui.exe` (no crash,
  clean startup) confirm this compiles and runs, on top of 518/518 `cargo test -p core --lib` +
  423/423 `cargo test -p ui` passing.

  **Still open**: FONT-01A's persisted-identity swap (`.ocproj` storing a `family_id` slug
  instead of the plain enum variant name, so an unrecognized *future* id keeps its exact string
  through a resave rather than normalizing to `Lato` — a real structural change; adding plain
  enum variants already gives the *practical* "selectable and persists correctly" outcome
  without it), variable-weight axis *selection* in the UI (today's default/bold-only picker
  still doesn't expose the axes these variable fonts actually carry), the searchable/categorized
  selector itself (FONT-01C, genuinely needed now more than ever — a 43-item flat list is a real
  UX regression from 6), and TEXT-01's own shaping-test gate before the 7 international families
  are used as an *automatic* script-fallback chain rather than a manual pick (they're manually
  selectable today, same as everything else).

  Verified for real, not just type-checked: this session's sandbox turned out to have a
  working path to a fully-linked `core` test binary (`rustup update stable` past a
  too-old-bundled-rustc block, then `apt-get install libavfilter-dev` +
  `libgstreamer*-dev` past the missing-headers/pkg-config gaps `CLAUDE.md` documents,
  plus the same temporary local `filters.c` shim `CLAUDE.md` describes for the
  FFmpeg-7.1-API gap, discarded before commit) — but `core`'s own test binary still can't
  *link* here (the pre-existing ONNX Runtime `download-binaries` network gap), so
  `font_catalog.rs`+`timeline.rs`+`keyframe.rs` (all pure, zero heavy deps) were copied into a
  throwaway scratch crate alongside the real bundled font assets and `cargo test`ed there for
  real: 76/76 passing, including a `sha2`-backed byte-for-byte hash/size check of every one of
  the 9 real `.ttf` files against the manifest's locked values (catching a hash/size drift for
  real, not just a manifest self-consistency check), a real-filesystem existence/no-traversal
  check for every `source_path`/`license_path`, and the `family_id` round-trip. `cargo fmt
  --check` and `cargo clippy -p core --lib --no-deps` (via the same temporary shim) both stayed
  clean for the new code; `cargo check --workspace --all-targets` passed for the whole
  workspace on the same shim.

  **Follow-up: `TextFontFamily` now survives loading a project saved by a newer build.**
  `#[serde(other)]` on `Lato` (moved to be the enum's last declared variant, which the attribute
  requires) makes an unrecognized persisted family name fall back to Lato instead of failing the
  whole project load — the doc's forwards-compatibility rule, minus the "keeps its serialized
  value" half, which still needs the full `family_id`-as-persisted-type swap this slice
  continues to defer. Confirmed against `rmp-serde`'s own source that `serialize_unit_variant`
  always writes the variant's name string, never a positional index, so reordering the variant
  declaration order cannot change the meaning of already-persisted `.ocproj` bytes for the five
  untouched variants. Verified for real: a new `core` integration test
  (`projects_with_an_unrecognized_font_family_name_load_with_the_lato_fallback`,
  `crates/core/tests/persistence_test.rs`) hand-edits a saved project's encoded MessagePack to
  reference a fictitious `"InterVariable"` family and confirms the project still loads, with
  that one field falling back to Lato while unrelated fields on the same clip round-trip
  untouched; a second scratch-crate check (real `rmp-serde` round-trips, not `core`'s own
  unlinkable test binary) independently proves both the name-not-index wire format and the
  fallback for an unrecognized/empty name, 4/4 passing. `cargo check --workspace --all-targets`
  (including this new persistence test) and `cargo clippy -p core --lib --no-deps` both stayed
  clean via the same temporary shim.

  **FONT-01A's persisted-identity swap shipped for real.** `TextFontFamily` now implements
  `Serialize`/`Deserialize` by hand instead of deriving them: it serializes as its
  `font_catalog` `family_id` slug (e.g. `"bebas-neue"`), deserializes that slug back, still
  accepts the old bare variant name (`"BebasNeue"`) for backward compat with pre-swap saves,
  and — the doc's forwards-compat half this slice previously deferred — preserves an
  unrecognized id verbatim as a new `Unknown(String)` variant instead of collapsing it to
  `Lato`. `Unknown` renders/behaves as `Lato` everywhere else (`family_id()` returns the
  original string, `supports_bold()` returns `false`) but is never a member of `ALL`, so it
  can't be selected — only arrived at by loading a save from a newer build. `Copy` had to be
  dropped from the enum (`Unknown` owns a `String`); the resulting move errors were fixed with
  `.clone()` at six real call sites (`motion_template.rs`, `overlay_render.rs` x2, `render.rs`
  x2, `text_layout.rs`'s `shape()`), not by adding `Copy` back or cloning speculatively
  elsewhere. The `ui` font picker (`text_clip.rs`) now shows `"Unknown font (<id>)"` for an
  `Unknown` value instead of panicking or matching nothing.

  Verified for real, fully linked (this session's sandbox links `core`'s test binaries): the
  golden-file test this slice's docs named as still-needed
  (`projects_with_an_unrecognized_font_family_name_load_with_the_lato_fallback`) is renamed to
  `..._preserve_it_as_unknown` and rewritten to hand-edit a real saved project's MessagePack
  bytes to reference a fictitious `"InterVariable"` family, then confirm it survives both the
  initial load *and* a resave as `Unknown("InterVariable")` rather than falling back to `Lato`
  — `cargo test -p core --test persistence_test`, 19/19 passing. Five new unit tests in
  `crates/core/tests/timeline_test.rs` cover the slug serialization format, a full round-trip
  over every `ALL` member, old-format backward-compat deserialization, and `Unknown`'s
  preserve/render/behave contract — `cargo test -p core --test timeline_test`, 139/139 passing
  including these. Full-suite regression check: `cargo test -p core`, 45 test binaries, every
  one passing except the pre-existing (confirmed via `git stash` against a clean checkout,
  unrelated to this change) `preview_test.rs` GStreamer refresh flake; `cargo test -p ui`,
  424/424 passing. `cargo fmt -p core -p ui` clean.

  **FONT-01C's categorized/searchable selector shipped too.** The flat 43-item `ComboBox` in
  `text_clip.rs` is now a search box (filters by display name, `family_id`, and catalog tags —
  typing "mono" surfaces every monospace family) followed by sections grouped under their
  `font_catalog::FontCategory` heading (Sans/Display/Serif/Handwritten/Monospace/
  International, the doc's fixed order), each section omitted when nothing in it matches. The
  search string lives in egui's own per-widget temp storage keyed by the clip id, not a new
  `App` field. `cargo build -p ui` clean, `cargo test -p ui` 424/424 passing (no dedicated
  widget-level test — this codebase doesn't unit-test `ComboBox` popup bodies elsewhere
  either), and a direct launch of the built `ui.exe` confirmed no startup crash
  (`MainWindowTitle: "oca"`); interactive click-through of the new popup itself wasn't done
  this session (`computer-use` can't grant access to an unregistered dev-build `.exe` — see
  memory note — and `make test-e2e` has its own unrelated hang), so treat the picker's visual
  layout as code-reviewed and compile/launch-verified, not click-tested.

  **Still open at the time of this slice**: variable-weight axis selection in the UI, and TEXT-01's
  automatic script-fallback chain for the international families — both since shipped, see this
  entry's own later notes below.
- `[~]` **TEXT-01: complex text shaping and bidirectional layout.** Replace per-character
  `fontdue` layout with one bundled-only shaping/layout/rasterization engine covering OpenType
  ligatures/contextual forms, UAX #9 bidi, UAX #14 wrapping, cluster-safe timed highlights,
  variable weights, and deterministic Arabic/Hebrew/Indic/Thai fallback. May proceed alongside
  FONT-01A/B and is required before international families are exposed. Read
  [architecture/complex-text-shaping.md](architecture/complex-text-shaping.md).

  **Acceptance spike passed (2026-08-28)**, confirming the user-picked `cosmic-text` candidate
  before any production code changes, per the doc's own "before migrating production rendering, a
  small isolated spike must prove..." gate. Run as a throwaway crate (not committed — nothing in
  `core`/`ui` changed by this), `cosmic-text = { version = "0.19", default-features = false,
  features = ["std", "swash"] }` against a `fontdb::Database` built via `new_with_locale_and_db`
  (never `FontSystem::new()`, which the doc explicitly forbids for loading installed system
  fonts) and loaded only with oca's own six bundled families plus one real fetched Inter (variable
  weight) and the six Noto international families FONT-01C will eventually vendor — all fetched
  from the same pinned `google/fonts` revision `built-in-font-catalog.md` already uses, for spike
  verification only. All 8 of the doc's acceptance gates passed with real evidence, not just
  reading vendor docs: bundled-only loading (confirmed both by reading `PlatformFallback`'s source
  — a compile-time name list tried against whatever db is supplied, never an OS scan — and by
  counting exactly the loaded faces, no more); real per-character UAX #9 bidi levels and correct
  RTL visual glyph ordering for mixed Arabic/Latin/digit text; zero `.notdef` hits shaping Hebrew/
  Devanagari (with a real conjunct)/Bengali/Tamil/Thai sample words; `fi`/`fl` ligature formation
  in Lato (16 chars -> 12 glyphs, stable multi-char cluster ranges); different rasterized pixel
  bytes at `wght` 400 vs 700 from one real variable Inter file; real non-zero pixels painted into
  a plain RGBA buffer shaped like `overlay_render.rs`'s existing overlay; a base+combining-accent
  grapheme surviving a narrow-width wrap intact; ~37µs/shape-pass for a realistic caption in
  release mode; and a fully pure-Rust dependency tree (harfrust/skrifa/swash/fontdb/unicode-bidi/
  unicode-linebreak, MIT/Apache-2.0, `rust-version 1.89` under this repo's pinned `1.98.0`) once
  the default `fontconfig` feature is disabled. One real nuance recorded for TEXT-01C: a variable
  Noto font can register into `fontdb` at a non-400 default weight, so oca's own loader must
  request the axis value explicitly rather than trust the file's default named instance. See
  `architecture/complex-text-shaping.md`'s own "Spike result" note for the full gate-by-gate
  writeup.

  **TEXT-01A steps 1-2 (adapter) shipped.** `avcore::text_layout` adds the provider-neutral
  `ShapedText`/`ShapedLine`/`ShapedGlyph` value and `TextLayoutEngine` — `cosmic-text` is now a
  real `core` dependency (`default-features = false, features = ["std", "swash"]`, `fontconfig`
  disabled). `font_catalog` gained `locked_face_bytes()`/`face_bytes()`, the single
  `include_bytes!` source of truth both `text_metrics`'s existing `fontdue` table and this new
  module load from, so the two engines can never silently diverge on which bytes a family/weight
  resolves to. `TextLayoutEngine::new_from_locked_catalog()` builds its `fontdb::Database` from
  exactly those locked bytes via `FontSystem::new_with_locale_and_db` — never `FontSystem::new()`
  — matching the spike's own gate-1 finding. Verified for real (not just type-checked): a
  throwaway scratch crate with a real `cosmic-text` dependency and the real bundled font assets
  ran 87/87 tests, 9 new — bundled-only loading (exact face count, no system-font leak), all six
  families shaping the GF Latin Core corpus with zero `.notdef` hits, `fi`/`fl` ligature formation
  with a real multi-character cluster, Bold vs. Regular resolving to different faces, a
  single-weight family accepting a Bold request cleanly, width monotonicity, and wrapping actually
  producing multiple lines. `cargo check --workspace --all-targets`/`clippy`/`fmt` (via the
  documented temporary shim) all clean.

  **TEXT-01A step 3 (the swap) shipped the same day.** `overlay_render.rs` now shapes and
  rasterizes every `TextClip`/`TextSegment` through `text_layout::with_shared_engine` instead of
  `fontdue::layout::Layout` — glyphs paint via `cosmic_text::SwashCache::with_pixels`, and the
  rounded background's bounding box comes from each glyph's real rasterized `Placement` (a tight
  ink bbox, matching `fontdue`'s old semantics), not just its advance box. `TextLayoutEngine::
  shape` gained an `origin: (f32, f32)` parameter so every glyph is already positioned at its
  final canvas pixel.

  **Real bug found and fixed while wiring this in**: `cosmic-text`'s `LayoutGlyph::y` is relative
  to that glyph's own run, not an absolute canvas position — the piece that actually varies line
  to line is `LayoutRun::line_y`, which a first pass didn't add in, collapsing every wrapped line
  onto the same row (and, combined with `origin.1 = 0.0` in the failing tests, pushing the first
  line's whole ascent above row 0 — a blank render, not an obviously-wrong one). Caught by 3 of
  `overlay_render_test.rs`'s existing regression tests failing for real, root-caused against
  `cosmic-text`'s own source, then fixed and reproduced-passing.

  Verified for real: all 16 of `overlay_render_test.rs`'s existing structural tests (opaque/
  transparent pixel checks, rounded-background corners, highlight-color-word tests, multi-line
  wrapping) pass unchanged against the new renderer — 137/137 in the same throwaway scratch-crate
  approach (this time also carrying `render.rs`'s `TextSegment` struct, `shape_render.rs`, and
  `text_metrics.rs`). Beyond structural tests: two real sample captions (plain text with a
  highlighted word and rounded background; a caption wrapping across three lines) were rendered
  through the real `render_text_clip_rgba` entry point, composited onto an opaque backdrop, saved
  as PNG, and visually inspected — correct glyph shapes including `ç`, correct background padding,
  correct highlight coloring, correctly stacked wrapped lines. `text_metrics.rs`'s old
  per-character `text_width_px`/`word_x_offsets_px` are now dead code (no caller outside their own
  tests) — left in place with a doc-comment note rather than deleted, a separate cleanup from this
  swap. `cargo check --workspace --all-targets`/`clippy`/`fmt` (via the documented temporary shim)
  all stayed clean.

  **Still not done**: golden-image comparison against the pre-swap `fontdue` renderer's actual
  pixels (never the goal — different rasterizers, different hinting; the doc's bar is preserved
  *visual* output, satisfied by the inspected PNGs above) and any live-GUI/export-encode
  confirmation, which needs a real windowed session this sandbox doesn't have.

  **TEXT-01B slice 1 (explicit paragraph direction) shipped.** `TextClip` gained
  `direction: TextDirection` (`Auto`/`Ltr`/`Rtl`, `#[serde(default)]` = `Auto`, the exact behavior
  every clip already had) plus a persisted-but-not-yet-consumed optional `language` hint (real,
  deliberately deferred gap — `cosmic-text` 0.19's `Attrs` has no language field to feed it into;
  kept now so an authored project doesn't need a second migration once TEXT-01C's shaping actually
  reads it). `TextLayoutEngine::shape` forces UAX #9's own P2/P3 auto-detected paragraph level by
  prepending an invisible LRM/RLM bidi mark before shaping — a real strong-directional character
  for detection purposes, not a bidi *override*, so embedded opposite-script runs still resolve
  normally within the pinned level (matching CSS's `direction` property, not `unicode-bidi:
  bidi-override`) — then strips the mark back out of the returned glyph list and rebases every
  cluster byte range to the caller's own original text, so the override is fully invisible to
  every caller. `TextSegment` (`render.rs`) carries the same field through so preview and export
  apply it identically. New "Direção do texto"/"Text direction" combo box in the properties
  panel's text-clip section, next to the existing font style control.

  Verified for real against actual `cosmic-text` and the real bundled fonts (scratch-crate
  technique, not just type-checked): forcing a direction never drops/duplicates a glyph or leaks
  the internal mark into a cluster range (proven both by comparing cluster *sets*, order-
  independent, against `Auto`, and by every cluster staying within the caller's own text length);
  a pure Latin string still renders left-to-right in final pixel position even when RTL-anchored;
  and forcing `Rtl` on a mixed Latin/Hebrew string measurably flips which side of the line each
  script's run lands on (the real, observed proof the override changes paragraph/run *ordering*,
  TEXT-01's own goal 2). One real false start caught by running these tests for real rather than
  trusting the type-check: an earlier draft asserted glyph *iteration order* stays unchanged
  across directions, which is false — HarfBuzz-style RTL shaping legitimately emits glyphs in
  reverse-of-visual (rightmost-pen-position-first) iteration order for an RTL-leveled run even
  though final pixel positions are correct; fixed by comparing cluster *sets* and final `x`
  positions instead of raw iteration order. A second false assumption (forcing `Ltr` on
  already-strong-RTL Hebrew text should flip every glyph's individual resolved bidi level) was
  also caught and corrected — UAX #9's own I1/I2 rules keep a strong-R character's level odd
  regardless of the paragraph's base direction; only *run ordering/positioning* is what a
  paragraph-direction override actually changes, not an individual strong character's intrinsic
  script-level resolution.

  **Deliberately not done in slice 1**: alignment and the `language` hint's own consumption were
  both open — alignment now shipped as its own slice below; `language` remains open.

  **TEXT-01B slice 2 (visual alignment) shipped.** `TextClip` gained `text_align: TextAlign`
  (`Auto`/`Left`/`Center`/`Right`, `#[serde(default)]` = `Auto` — this clip's exact pre-existing
  behavior). Scoped down from the doc's own "semantic alignment" (`Start`/`End`, direction-aware)
  to plain visual Left/Center/Right for this slice — a `Start`/`End` mapping needs a resolved
  paragraph direction to know which physical edge "start" even means, which for `TextDirection::
  Auto` isn't knowable ahead of actually shaping the text; a real, separate follow-up.

  The real design decision this slice turned on, confirmed with the user before writing code
  (see `.claude/CLAUDE.md`'s own "design before code" rule for exactly this kind of anchor-
  semantics ambiguity): once alignment isn't `Left`, what does `pos_x` mean? Two options —
  always align within a fixed canvas-wide box (simple, but `pos_x` silently stops mattering the
  moment alignment leaves `Left`) vs. redefine which edge `pos_x` anchors per alignment (matches
  how most caption/design tools bind a position handle to the selected alignment, at the cost of
  `pos_x`'s meaning depending on this field). The user picked the second, so: `Auto`/`Left` keep
  `pos_x` as the block's left edge (unchanged); `Center` makes it the block's horizontal center;
  `Right` makes it the block's right edge. `overlay_render.rs`'s new `text_horizontal_box`
  computes the `(origin_x, max_width_px)` wrap/alignment box per anchor edge — symmetric around
  the anchor for `Center` (so `cosmic-text`'s own per-line centering, given that symmetric box,
  lands each line's center exactly on the anchor regardless of that line's own width), mirrored
  around the anchor for `Right`. `TextLayoutEngine::shape` threads the resulting `TextAlign`
  straight into `cosmic-text`'s own `Align` via `Buffer::set_text`, `Auto` passing `None` through
  unchanged (`cosmic-text`'s existing direction-aware left/right default, zero behavior change).
  `TextSegment` carries the field through the same way `direction` already does.

  Verified for real against actual `cosmic-text` and the real bundled fonts (scratch-crate
  technique): `text_horizontal_box`'s own geometry (5 pure-arithmetic tests — `Auto`/`Left`
  unchanged, `Center` symmetric on either side of the canvas's midpoint, `Right` anchors the
  right edge, no variant ever produces a non-positive width) plus two real end-to-end shaping
  checks proving `cosmic-text`'s actual glyph output lands a line's center/right edge exactly on
  the anchor pixel for `Center`/`Right` respectively — not just that the box arithmetic
  type-checks. `cargo check --workspace --all-targets` (the documented temporary `filters.c`
  shim, discarded before commit) and `cargo fmt --check` both stayed clean.

  **TEXT-01B step 4 (cluster/run-geometry replacing the approximate word-width helpers) is now
  done too — as a deletion, not a rewrite.** `text_metrics.rs`'s `fontdue`-based `bundled_font`/
  `text_width_px`/`word_x_offsets_px` (and their font-selecting counterparts) turned out to
  already have zero callers outside their own tests, confirmed by grep across `core`+`ui` before
  touching anything — `TextLayoutEngine::shape`'s real shaped-glyph geometry has been the only
  measurement path actually wired to pixel-affecting output since TEXT-01A's rendering swap
  shipped. Removed the dead functions, their nine per-face `OnceLock` font caches, and the now-
  entirely-unused `fontdue` dependency itself (confirmed gone from `Cargo.lock` after removal —
  a real, verified reduction, not just an unused-import warning suppressed). `word_byte_ranges`
  (pure string search, engine-independent, still used by both `overlay_render.rs` and `render.rs`
  regardless of shaping engine) is untouched. `cargo check --workspace --all-targets` (the
  documented temporary `filters.c` shim, discarded before commit) and `cargo fmt --check` both
  stayed clean.

  **Directional-control visibility/warnings (the first half of TEXT-01B step 3) now shipped
  too.** `avcore::text_layout::scan_bidi_controls` flags two shapes the architecture doc calls out
  for a non-blocking warning, never a strip (legitimate bidi content must round-trip exactly): a
  well-formed LRO/RLO override anywhere in the text (forces every character between it and its
  close — or the end of the paragraph, if never closed — to render in one direction regardless of
  script, visually misleading by design even when correctly paired), and an unmatched explicit
  directional control (LRE/RLE/LRO/RLO/LRI/RLI/FSI never closed by end of string, or a stray
  PDF/PDI with nothing open to match). Matching uses a single combined open-control stack rather
  than full UAX #9 isolate-run resolution, which this only needs to *flag*, not apply
  (`TextLayoutEngine::shape`'s `cosmic-text`-backed `unicode-bidi` already does that) — `PDF`/`PDI`
  only pop when the stack top is the matching kind, mirroring UAX #9 rule X7/X6a's own "if there
  is no matching code, do nothing" so a stray close never incorrectly closes an unrelated open
  control underneath it. The text-clip properties panel shows a non-blocking `theme::WARNING`
  label under the text field when the current text trips either flag — same treatment
  `DeleteSequenceCompoundClipWarning` already established.

  Verified for real, not just type-checked: all 8 tests — including the "a stray PDF must not
  incorrectly close an outer isolate" edge case — ran in a bare standalone scratch crate (this
  function has zero dependencies, not even `cosmic-text`, so no font assets were needed either)
  and passed. `cargo check --workspace --all-targets` (the documented temporary `filters.c` shim,
  discarded before commit) and `cargo fmt --check` both stayed clean.

  **`Start`/`End` semantic alignment now shipped too**, closing the gap this entry's own
  "deliberately not done" note originally flagged as a separate follow-up. `TextAlign` gains
  `Start`/`End`, the CSS-logical-property counterparts of `Left`/`Right`. `avcore::text_layout::
  resolve_paragraph_direction(text, direction)` resolves `TextDirection::Auto`'s own UAX #9 P2/P3
  first-strong-character detection via `unicode_bidi::get_base_direction` — already a transitive
  dependency of `cosmic-text` at the exact same pinned version (0.3.18), now also a direct one, so
  this added no new dependency tree — or the explicit `Ltr`/`Rtl` override when `direction` isn't
  `Auto`. `resolve_text_align` maps `Start`/`End` onto the matching physical `Left`/`Right` edge
  and is the single source of truth both `TextLayoutEngine::shape` (per-line `cosmic-text`
  alignment) and `overlay_render.rs`'s `text_horizontal_box` wrap/alignment box computation call —
  `draw_text_segment_onto` resolves once and passes the resolved value to both, so a clip's
  `Start`/`End` alignment can never resolve to different physical edges between the box and the
  actual shaping. The properties panel's alignment combo box gained the two new options next to
  the existing four.

  Verified for real against the actual `unicode-bidi` crate (scratch-crate technique, not just
  type-checked): 7 new tests covering pure-Latin/pure-Hebrew auto-detection, UAX #9 P3's
  all-neutral-text-defaults-to-LTR rule, an explicit override winning over the text's own script,
  first-strong-character resolution deciding a mixed-script paragraph, `Start`/`End` flipping
  physical edge under both an explicit direction and auto-detected RTL text, and the non-logical
  variants (`Auto`/`Left`/`Center`/`Right`) passing through `resolve_text_align` unchanged.
  `cargo check --workspace --all-targets` (the documented temporary `filters.c` shim, discarded
  before commit) and `cargo fmt --all -- --check` both stayed clean.

  **Mixed-direction golden tests (TEXT-01B step 3's second half) now shipped, a later session.**
  `overlay_render_test.rs` gained a real pixel-level render (not just `text_layout`'s own
  shaping-level cluster-position checks) of mixed Latin+Hebrew content forced `Ltr` vs `Rtl`,
  confirming forcing `Rtl` visibly shifts the block's own ink right of forcing `Ltr` — a real,
  observed pixel difference from the actual `render_text_clip_rgba` pipeline, matching `Auto`
  alignment's documented per-paragraph-direction default in `cosmic-text` (see
  `text_horizontal_box`'s and the `TextAlign` mapping's own doc comments). `cargo test -p core
  --lib` 528/528 passing (this session's environment fully links).

  **Deliberately still not done**: the `language` hint's own consumption (still genuinely
  blocked — re-confirmed this session against `cosmic-text` 0.19, still the crate's own newest
  published version per a real `crates.io`/`docs.rs` check, and `Attrs` still has no language
  field), and the "expose invisible directional controls on demand" editing feature (letting a
  user insert/reveal these characters directly, as opposed to just being warned about them) — a
  real UX design decision (where in the text-clip panel, which characters, cursor-position
  insertion vs. append-only) not yet made, left open rather than guessed at. The original
  cluster-safe-highlight note is now unblocked in principle — FONT-01B's international faces are
  fully vendored and TEXT-01's own shaping test already confirms every one of them renders real
  glyphs with zero `.notdef` hits — but verifying real cluster-safety specifically for a
  highlighted word mid-conjunct/RTL-cluster hasn't been attempted yet; a real, separate follow-up
  from the golden test above, which only checked whole-paragraph direction, not word-level
  highlight-boundary safety.

  **TEXT-01D slice 1's "shaped/glyph caches" piece now shipped.** `TextLayoutEngine::shape`
  memoizes through a new small bounded LRU `ShapeCache` (64 entries) keyed by every input that
  can change its output — text, family, style, size, wrap width, origin, direction, alignment
  (floats compared by exact bit pattern via `to_bits()`, never derived epsilon-smoothed
  equality). Memoization is only correct because this engine's locked font catalog never changes
  at runtime, so identical inputs always produce identical glyphs. Targets the real common
  workload: `overlay_render.rs`'s `draw_text_segment_onto` re-shapes the same caption every
  preview frame while the playhead moves within one clip's own steady on-screen duration — text,
  styling, and position all unchanged frame to frame, so every frame after the first becomes a
  cache hit instead of a full re-shape. A plain `Vec` with linear scan, not a `HashMap`: the four
  `crate::timeline` enums making up the cache key don't derive `Hash`, and this crate's own
  convention is to reuse shared types as-is rather than adding derives elsewhere for one caller's
  convenience — a linear scan over a capacity this small is far cheaper than a text-shaping pass
  regardless. `shape_cache_stats()` exposes `(hits, misses)` for tests and a future TEXT-01D
  slice 2 benchmark.

  **Deliberately not done**: "bounded background shaping" and "generation cancellation" (slice
  1's other two pieces) — this crate's preview rendering shapes synchronously on the calling
  thread today, with no existing async shaping pipeline for a cancellation token to hook into;
  benchmarking, optional CJK/color-emoji packs, and the supported script/language matrix (slices
  2-4) remain fully open.

  Verified for real against actual `cosmic-text` and the real bundled fonts (scratch-crate
  technique, same setup TEXT-01A's own verification established): 5 new tests — a repeated
  identical call registers as a cache hit with matching glyph output to a fresh uncached shape,
  any differing input (text, origin, or font size) each independently causes a miss, and pushing
  the cache one entry past its capacity evicts the genuinely least-recently-used entry (confirmed
  by re-shaping it and observing a fresh miss, not a hit) — 35/35 total passing alongside every
  pre-existing `text_layout` test. `cargo check --workspace --all-targets` and `cargo clippy
  --workspace --all-targets` (via the documented temporary `filters.c` shim, discarded before
  commit) and `cargo fmt --check` all stayed clean. See `architecture/complex-text-shaping.md`'s
  own writeup for the full detail.

  **FONT-01C's "variable-weight axis selection" gap (flagged open above) now shipped, scoped
  down from a full TEXT-01C shaping migration.** Read as originally written, this looked gated
  behind a much larger change; investigated cautiously instead of either skipping it or guessing
  at an implementation. Direct inspection of `cosmic-text` 0.19's own source (`font/system.rs`,
  `font/mod.rs`, `swash.rs`) confirmed it already resolves an explicit OpenType `wght` axis value
  through `fontdb`'s variable-font instancing and `Attrs::weight()` — no engine change needed, only
  a plumbing one. `TextClip`/`TextSegment` gained `font_weight: Option<u16>` (`#[serde(default)]`,
  `None` = today's plain Regular/Bold behavior, unchanged). `TextLayoutEngine::shape`/
  `shape_uncached` take a new `weight_override: Option<u16>` parameter threaded into
  `Attrs::weight()`; `ShapeCacheKey` gained the matching field (two shapes differing only in this
  input produce different glyph outlines on a variable family, so it must be part of the cache
  key like every other shaping input, same reasoning as `direction`/`text_align` before it).
  `text_clip.rs`'s properties panel shows a 100-900 weight slider, but only when
  `font_catalog::find_family(..).source_kind == FontSourceKind::Variable` — a static two-face
  family keeps its existing Regular/Bold-only control, since it has no axis to slide.

  Verified for real against actual `cosmic-text` and the real bundled Inter variable file
  (scratch-crate technique, same setup TEXT-01A's own verification established): weight 100 vs.
  900 resolve to the *same* font file (`font_id` equal — genuine single-face axis instancing, not
  cosmic-text silently substituting two different static faces) yet render measurably wider
  glyphs at 900, with at least one individual glyph's own advance width (not just aggregate line
  width) growing; an explicit `Some(700)` override on a static family (Lato) produces the exact
  same shape as its existing `TextFontStyle::Bold` path, confirming `None` isn't silently doing
  anything different from an explicit value matching the old Bold weight; and an off-axis override
  a static family has no matching face for (`Some(550)` on Lato) degrades gracefully via `fontdb`'s
  own nearest-weight matching rather than panicking or producing an empty shape. Mirrored as 3 new
  inline tests in the real `crates/core/src/graphics/text_layout/text_layout_test.rs`, plus the existing
  `shape_cache_misses_when_any_input_differs` test extended with a 5th differing-input case for
  the new field. `cargo check --workspace --all-targets` and `cargo clippy -p core --lib --no-deps`/
  `-p ui --tests --no-deps` (via the documented temporary `filters.c` shim, discarded before
  commit) and `cargo fmt --check` all stayed clean.

  **Still open**: the doc's own "categorized/searchable selector" scope for any future per-family
  axis beyond `wght` (e.g. width/slant/optical-size axes some variable fonts also carry) — this
  slice only exposes the one axis every FONT-01B variable family actually ships.

  **TEXT-01's own automatic script-fallback chain now shipped too.** Direct inspection of
  `cosmic-text` 0.19's own source (`font/fallback/mod.rs`) found a real `Fallback` trait already
  wired into `FontSystem` construction — `FontSystem::new_with_locale_and_db_and_fallback`, not
  the `new_with_locale_and_db` this module previously called (which defaults to `PlatformFallback`,
  an OS-conventional name list). `text_layout.rs` now builds its `FontSystem` with a new
  `OcaFallback` implementing that trait, mapping each `unicode_script::Script` to the single
  bundled `Noto Sans <Script>` display name that actually covers it (Arabic, Hebrew, Devanagari,
  Bengali, Tamil, Thai — FONT-01B's own 6 default international faces; "Noto Naskh Arabic" stays a
  manual style pick, not part of the automatic chain) — `common_fallback`/`forbidden_fallback` both
  empty, since nothing beyond a caller's own selected family should apply regardless of script, and
  nothing needs forbidding. `unicode-script = "0.5"` added as a direct dependency at the exact
  version already pinned transitively through `cosmic-text` — no new dependency tree, same
  reasoning `unicode-bidi`'s own earlier direct-dependency addition documents.

  **A real, contrary finding from running this against the actual bundled catalog, not assuming
  the acceptance-criteria framing was still accurate**: a control test proved the *previous*
  `PlatformFallback` construction does **not** actually produce `.notdef` for embedded Arabic/
  Hebrew/Devanagari/Bengali/Tamil/Thai text today, despite none of its own OS-conventional
  fallback names (e.g. "Segoe UI Historic") ever matching anything in this bundled-only database
  — `cosmic_text::font::fallback::FontFallbackIter` turns out to carry a *third*, undirected
  fallback tier below both the script- and common-fallback name lists: it walks every other
  loaded font in weight-match order and tries shaping the missing cluster against each one in
  turn, stopping at the first that actually produces a real glyph, regardless of whether that
  font was ever named in any fallback list. With only 43 bundled families today, this exhaustive
  per-font trial-and-error already stumbles onto the correct `Noto` face by elimination — meaning
  the doc's original "produces `.notdef` boxes" motivation for this gap doesn't hold as stated.
  `OcaFallback`'s real, still-genuine value is turning that undirected, ~three-dozen-font trial
  into a deterministic, efficient, name-directed match to exactly the intended face — confirmed
  by comparing resolved font ids, not just re-checking for the absence of `.notdef` (which both
  the old and new construction already avoid).

  Verified for real against actual `cosmic-text` and the real bundled fonts (scratch-crate
  technique): 11 tests — the 6 international scripts each shaping through the unrelated `Lato`
  family with zero `.notdef` hits, a realistic mixed-script string, a plain-Latin regression
  check, an explicit `NotoSansArabic` pick still working unchanged, the control above (documented
  as a passing, informative assertion rather than deleted once its original premise turned out
  false), and the determinism check (`Lato`'s fallback-resolved Arabic glyph and a direct
  `NotoSansArabic` pick resolve to the exact same font id). Mirrored as 8 new tests in the real
  `crates/core/src/graphics/text_layout/text_layout_test.rs` (the control test itself stays scratch-crate-
  only, since it deliberately reconstructs the *old*, no-longer-present construction path rather
  than testing this module's own current code). `cargo check --workspace --all-targets`, `cargo
  clippy -p core --lib --no-deps` (via the documented temporary `filters.c`/`text_overlay.c` shim,
  discarded before commit), and `cargo fmt --all -- --check` all stayed clean.
- `[x]` **CF-01: transcript-based editing and speech cleanup.** Reuse Whisper word timings to
  search, seek, propose filler-word/retake removals, and apply reviewed cuts as one undo action.
  **Slice 1 (persist a media-relative transcript document) shipped**:
  `avcore::transcript::TranscriptDocument` flattens whisper.cpp's segment-grouped output into a
  stable-word-id, explicit-schema-versioned, media-relative (source-file seconds, not timeline)
  document — deliberately its own type, not the existing `TranscribeSegment`/`TranscribeWord`
  (Whisper's raw per-inference output, no id/schema/persistence), per the plan's own "keep
  subtitle text and editorial transcript separate" requirement. `TranscribeWord` gained a real
  `confidence: f32` field (the mean of a word's constituent tokens' own
  `whisper_token_data::p` — confirmed against `whisper-rs-sys`'s bindgen output, not assumed).
  Persisted as a sidecar (`.<project>_transcripts/asset_<id>.octr`) via a new `OCTR` framing
  through `crate::persistence`'s existing magic-bytes + version + size-bounded-gzip-MessagePack
  machinery (same one `.ocproj`/`.ocqueue` already use) — `TranscriptDocument::validate` layers
  semantic checks (schema version, duplicate word ids, out-of-range/non-finite
  timestamps/confidence) on top, satisfying the backlog's shared "treat sidecars as untrusted
  input" security requirement, since `#[serde(default)]`-driven forward compatibility only
  proves the bytes parsed, not that their contents make sense. Verified for real: 13 new unit
  tests including a real end-to-end one (actual Whisper inference against a fixture, through
  `from_transcribe_segments`, through `validate()`), plus a new confidence-range assertion added
  to the existing real-transcription integration test — both currently skip (not fail) without
  `WHISPER_MODEL_PATH` set, same convention `transcribe_test.rs` already used.

  **Slice 2 (transcript panel, seek-on-select, current-word highlight) shipped**: a searchable
  modal panel (toolbar's "📝 Transcrição/Transcript") over the *previewed clip's* own transcript
  document — same modal-panel shape `show_timeline_index_panel` (markers) already established.
  `App::ensure_transcript_loaded_for_preview` lazily loads/clears the shown document whenever the
  previewed clip's `asset_id` changes (called alongside `App::ensure_preview_loaded`, so no extra
  per-frame disk I/O beyond what preview loading already triggers); clicking a word maps its
  media-relative time back onto the timeline through the clip's own trim/speed and seeks there —
  never into a different clip, since a word only ever comes from the one clip currently
  previewed. The word covering the current playhead highlights live. `App::spawn_transcribe`'s
  existing subtitle-`TextClip` flow now also builds and saves a `TranscriptDocument` right after
  a transcription completes (`App::save_transcript_document_for`), closing the loop end to end —
  no separate action needed to populate the panel. Compound clips (nested sequences, no real
  `asset_id`) are treated the same as "no transcript yet," not an error. Verified: 5 new `App`-
  level unit tests (load/clear on clip change, missing-vs-present document, immediate panel
  refresh after a fresh save) — `cargo test -p ui` (309 passed).

  **Slice 3 (cross-project transcript search) shipped**: `avcore::transcript_search::
  search_transcripts_in_project` scans every media asset's persisted `.octr` sidecar for a
  substring match, sorted by source time within an asset — re-read on demand rather than
  indexed, since sidecars are small gzip-MessagePack and `MediaAsset` carries no "has transcript"
  flag to index against. The Transcript panel gained a "Pesquisar no projeto/Search in project"
  toggle: on with a non-empty query it lists hits grouped by asset instead of the previewed
  clip's own document; clicking a hit from a non-previewed asset seeks through the first timeline
  clip that uses it.

  **Slices 4-5 (proposed-edit list, apply as one undo action) shipped**: `avcore::
  transcript_proposals::detect_proposals` derives four reviewable edit kinds purely from a
  `TranscriptDocument`'s word stream — dead air (pause between words over
  `DEAD_AIR_THRESHOLD_SECS`), filler-word runs (language-aware, conservative vocabulary),
  immediate retakes (back-to-back word repetition), and repeated phrases (2-4 word n-gram
  repeated within `REPEAT_WINDOW_SECS`). `App::begin_transcript_proposals` (toolbar's "✂️ Edições
  de fala/Speech Edits") runs detection over the previewed clip's transcript and stages the
  result in `App::transcript_review` for a checkbox-per-proposal modal
  (`show_transcript_proposals_modal`), defaulting every proposal to accepted — never a silent
  auto-apply, same convention D1's silence review established. `App::apply_transcript_proposals`
  merges the accepted proposals' overlapping/adjacent source ranges
  (`avcore::merge_source_ranges`), maps each onto the clip's timeline coordinates
  (`avcore::map_source_range_to_timeline`, trim/speed-aware), and ripple-deletes them
  rightmost-first as **one** undo snapshot, reusing `Track::ripple_delete_range` unchanged. A
  no-op with an explanatory toast when there's no previewed clip, no saved transcript, or nothing
  detected; a no-op if the reviewed clip was deleted before Apply.

  Verified: 33 new `core` unit tests (search matching/sorting/corrupt-sidecar-skip; each proposal
  kind's detection, merge, and source-to-timeline mapping) and 13 new `ui` `App`-level tests
  (empty-state toasts, accept/reject toggling, merged-range apply, deleted-clip fallback) —
  `cargo test -p core --lib transcript` (33 passed), `cargo test -p ui transcript` (13 passed).
  All six of CF-01's original slices are now shipped (persist, panel, search, propose, apply,
  and subtitle/transcript kept as distinct types by design from slice 1).
- `[~]` **CF-02: gameplay event ingestion and watched-folder import.** Import versioned event
  sidecars/bookmarks and combine them with audio-spike scoring before building a full recorder.

  **Slices 2 and 4 (sidecar schema/validation, marker import, event-based/combined highlight
  scoring) shipped.** `avcore::gameplay_events` adds a versioned `EventSidecar` (`schema_version`,
  `source_media_filename`, `Vec<GameplayEvent>`) — `GameplayEvent` carries a closed
  `GameplayEventKind` (Kill/Death/Assist/Objective/Bookmark, deliberately *not*
  `#[serde(other)]`-tolerant like `TextFontFamily`'s font fallback: this item's own acceptance
  criteria require an unrecognized event kind to be **rejected**, not silently normalized),
  `source_timestamp_secs`, `confidence` (`[0.0, 1.0]`), and optional per-event
  `pre_roll_secs`/`post_roll_secs` overrides. `source_media_filename` is a bare filename, never an
  absolute path — same portability reasoning `collab_bundle` already established, checked by
  `EventSidecar::validate()` alongside schema-version/timestamp/confidence/roll-range checks, each
  with an actionable `Display` message naming the field and the bad value (this crate's existing
  manual `Display`/`std::error::Error` convention, e.g. `watched_folder::ProcessError` — no
  `thiserror` dependency in `core`, so none added here either).
  `import_events_as_markers` converts accepted events into `MarkerKind::Highlight` markers
  (reusing P2 item 9's marker infrastructure unchanged) through a caller-supplied
  source-to-timeline mapping closure, deduplicating by (kind, position-within-50ms) so importing
  the same sidecar twice — or once per clip instance of the same recording — is a no-op the
  second time, satisfying the item's own idempotency requirement without adding a new persisted
  "already imported" flag anywhere.

  `highlight_detection` gained the item's other explicit requirement — "event-only, audio-only,
  and combined highlight scoring independently testable" — as two new pure functions alongside
  the existing audio-only `detect_highlight_candidates`: `event_highlight_candidates` windows
  each event by its own or a caller default pre/post-roll and merges overlapping windows
  (keeping the higher confidence, same "don't double-count adjacent evidence" reasoning the
  audio detector's own consecutive-hot-window merging uses); `combine_highlight_candidates`
  unions audio-only and event-only candidate lists, boosting a window flagged by *both* sources
  to maximum confidence (`1.0`) — deliberately not requiring both like the audio-only detector
  requires both game-audio and mic channels, since CF-02's own goal is refining the existing
  detector with events as an additional signal, not replacing it with a stricter one.

  **Slice 1's automated watched-folder *import* — the "reusing the latter's `StabilityTracker`"
  follow-up this entry used to flag — is now shipped, in its tractable narrow form.** Rather
  than a second full watcher, the Limpeza screen's existing `avcore::watched_folder` cleanup
  pipeline (stability-detects a finished recording, re-encodes its audio into `processed/`)
  gained the missing last step: each `Done` row now shows a "+ Add to project" button.
  `App::add_watched_file_to_project` recomputes that row's deterministic `processed/` output
  path via `avcore::output_path_for` and queues it through the existing `App::spawn_import`
  pipeline unchanged — the same import path every other source uses, not a bespoke one — and
  marks the row so a second click can't queue it twice. Toasts instead if no project is open.
  Deliberately *not* a fully automatic "watch → clean → auto-import with no user action"
  pipeline: an explicit per-file click keeps the user in control of which recordings actually
  enter a project's media library, matching how every other import path in this app already
  works (no auto-import exists anywhere else in the codebase either). Slice 3's OBS/Medal/
  Outplayed format adapters are also not attempted — reverse-engineering an external
  tool's own export format without a documented spec to verify against would mean guessing at a
  schema, which `CLAUDE.md`'s own "do not guess APIs" rule rules out; a sidecar today is
  hand-authored or produced by an external script directly against this module's own schema and
  imported manually, the doc's own "user-provided sidecars" fallback path.

  **Slice 5 (per-game event allowlists) shipped too.** `EventSidecar` gained an optional
  `game_id: Option<String>` (`#[serde(default)]`, so an already-authored sidecar still parses).
  `avcore::gameplay_events::GameEventAllowlist` (`game_id`, `allowed_kinds`,
  `default_pre_roll_secs`, `default_post_roll_secs`) is a persisted, app-wide profile (`ui`'s new
  `PrefsState::game_event_allowlists`) a caller matches against a sidecar's own `game_id`; the new
  `apply_game_event_allowlist` filters events down to the allowed kinds (an empty list is a
  deliberate "import nothing from this game," distinct from leaving a game unconfigured — no
  matching allowlist at all — which imports unrestricted, CF-02's own pre-slice-5 behavior) and
  fills each passing event's own missing roll seconds from the profile's defaults, never
  overriding an event's own explicit value. A new Preferences card lists each configured profile
  — a `game_id` field, a checkbox per `GameplayEventKind`, and two optional roll-default rows (a
  checkbox toggles between "no default" and an editable seconds value, since `Option<f32>` has no
  natural blank-vs-explicit-zero widget of its own). `App::import_gameplay_events` looks up a
  matching profile by the imported sidecar's `game_id` and applies it before the existing per-clip
  range filtering.

  Verified for real, not just type-checked: all 21 `gameplay_events` tests (6 new — filter/
  empty-allowlist/roll-default-precedence cases, `game_id`'s JSON round-trip and back-compat
  parsing) ran in a scratch crate (this module has zero heavy dependencies) and pass.
  `cargo check --workspace --all-targets` (the documented temporary `filters.c` shim, discarded
  before commit) and `cargo fmt --check` both stayed clean.

  Verified: 3 new `App`-level `app_test.rs` cases for `add_watched_file_to_project` (a `Done`
  row's output queued through `spawn_import` and marked added; toasts and queues nothing
  without an open project; a no-op on a second call once already added) — type-checked cleanly
  under `cargo check`, same "can't link this sandbox's `ui` test binary" caveat as the sidecar-
  import `ui` wiring below.

  **Correction, a later session**: the "can't link this sandbox's `ui` test binary" caveat above
  and below was specific to the sandboxed environment that wrote it — this session's own
  environment fully links `cargo test -p ui`. Actually run for real, not just re-asserted: all 3
  `add_watched_file_to_project` cases and all 6 `import_gameplay_events_*` cases below pass —
  `cargo test -p ui add_watched_file_to_project` (3/3) and `cargo test -p ui
  import_gameplay_events` (6/6).

  **`ui` wiring shipped too**: the Editor toolbar's new "🎮 Import Events" button (`ui`'s new
  `gameplay_events.rs`) opens a file picker, reads and validates the picked sidecar through
  `avcore::gameplay_events::EventSidecar::parse_and_validate`, then imports its events as
  `Highlight` markers onto every timeline clip whose source asset's `MediaAsset::file_name`
  matches the sidecar's `source_media_filename` — this module supplies the source-to-timeline
  mapping `import_events_as_markers` deliberately leaves to its caller. Only events whose
  `source_timestamp_secs` falls within a given clip's trimmed `[source_in_secs,
  source_out_secs]` range are imported onto that clip, mapped through the same `start_secs +
  (source_secs - source_in_secs) / speed_factor` formula
  `transcript_proposals::map_source_range_to_timeline` already uses for the same purpose — so a
  sidecar covering a whole recording cut into several clips only places each event where it's
  actually visible, not extrapolated past a clip's own trim. Toasts on read/parse/validation
  failure, when no clip covers any of the sidecar's events (either no clip uses the recording,
  or every event falls outside the trimmed range of the clips that do), and when nothing new
  was imported (the import itself stays idempotent per `import_events_as_markers`' own dedup
  rule).

  Verified for real: since `gameplay_events.rs` and `highlight_detection.rs`'s new `core`
  functions have zero heavy dependencies (`serde`/`serde_json` plus
  `crate::timeline`/`crate::keyframe`), they were copied into a throwaway scratch crate with the
  real bundled dependency versions and `cargo test`ed for real (`core`'s own test binary can't
  link in this sandbox — the pre-existing ONNX Runtime gap): 96/96 passing, 30 new — sidecar
  round-trip through real JSON, every validation-rejection case (unsupported schema version,
  non-portable filename, non-finite/negative timestamp, out-of-range confidence, negative
  roll), an unrecognized event kind producing an actionable parse error naming the bad value,
  idempotent/offset-aware marker import, and both new highlight-scoring functions' windowing/
  merging/boosting behavior. The `ui` wiring's 6 new `App`-level tests (marker added at the
  mapped position, idempotent re-import, no-covering-clip toast, event-outside-trimmed-range
  toast, invalid-JSON toast, unreadable-path toast) type-check cleanly under `cargo check` but
  could not run for real this pass — this sandbox's `ui` test binary genuinely can't link here
  (confirmed directly, not assumed: `nm -D` on this box's packaged FFmpeg 6.1.1 shared libs shows
  `av_opt_set_array` is absent — a real FFmpeg-version gap, not a build misconfiguration — and no
  `libonnxruntime` package is installed), and unlike a pure-logic module this glue exercises
  `App`/`Project`/`Timeline` directly, so it can't be lifted into a scratch crate the way
  `gameplay_events.rs`'s own `core` functions were. `cargo check --workspace --all-targets`,
  `cargo clippy -p core --lib --no-deps` / `-p ui --bin ui --no-deps` / `-p ui --tests --no-deps`,
  and `cargo fmt --check` (the first three via the documented temporary local `filters.c` shim,
  discarded before every commit) all stayed clean — no new warnings beyond the pre-existing
  baseline.
- `[x]` **CF-03: integrated gameplay-voice cleanup.** Move the proven watched-folder FFmpeg chain
  into a non-destructive `Mic`-role effect with A/B preview and measured output.

  **Slice 1 (export-side non-destructive effect) shipped.** Reuses `scripts/Watch-Gameplay.ps1`'s
  own proven `highpass=f=80,afftdn=nf=-30,acompressor=threshold=-18dB:ratio=3:attack=10:
  release=250:makeup=1.5,alimiter=limit=0.95` chain verbatim (real values read from the script
  itself, not guessed) as `ClipInstance::voice_cleanup_enabled` plus four adjustable params
  (`voice_cleanup_noise_floor_db`, `voice_cleanup_compressor_threshold_db`,
  `voice_cleanup_compressor_ratio`, `voice_cleanup_ceiling_linear` — the "small advanced panel"
  scope this item's own doc calls for; highpass frequency and compressor attack/release/makeup
  stay fixed at the proven defaults, not exposed). A plain per-clip toggle, not role-gated at the
  data layer — the "Mic by default, explicit override elsewhere" acceptance criterion is a `ui`
  selector concern for a later slice, deliberately not baked into the model itself. Wired through
  the FFI boundary as five new `avbridge::AudioSegment` fields, spliced into `audio_mix.c`'s
  per-branch filter chain (`build_mix_graph`) between that branch's own `volume` and `delay`
  stages when enabled — ahead of the existing whole-mix `afftdn`/`loudnorm`/`alimiter` mastering
  pass every export already runs, so a Mic branch gets cleaned up before mixing, not just at the
  very end. `render.rs::resolve_audio_segments` passes the `ClipInstance` fields straight through
  unchanged, and `ClipFormatting`/`split_clip_at`/copy-formatting carry them the same way every
  other per-clip effect field already does.

  **Deliberately not done**: no live GStreamer preview effect (export-only, the same "export
  first" shape several other effect fields on `ClipInstance` started with — P4 item 32's own
  entry documents why: most effects' elements are only conditionally present in the running
  pipeline at all), no A/B preview, no measured before/after loudness display, and no per-clip
  role-based default/suggestion UI. All real, separate follow-up slices, not silently dropped.

  **Follow-up: slice 2's `ui` wiring now ships.** A "🎤 Limpeza de voz" section in the properties
  panel (`components::property_block`, same shape Chroma Key/Background Removal already use — a
  checkbox plus the four sliders once enabled), gated to Audio-track clips —
  `App::set_selected_clip_voice_cleanup` clamps each parameter to its own new
  `VOICE_CLEANUP_*_RANGE` constant and goes through the shared `with_selected_clip_mut` dispatch
  (undo coverage for free, no live-preview push since none exists for this effect yet). Still no
  A/B preview or measured before/after display (slice 3) and no per-clip role-based default/
  suggestion UI — both remain open. Verified via `cargo check --workspace --all-targets` (the
  documented temporary local `filters.c` shim) and `cargo fmt --check`, both clean; not run
  against a live GUI session (no display in this environment) — the properties-panel section's
  actual appearance/behavior needs a manual pass on a real dev machine.

  **Per-clip role-based default/suggestion UI now shipped too — CF-03's last still-open piece.**
  `App::add_asset_to_timeline`/`add_asset_to_timeline_at`/`detach_audio_from_selected_clip` now
  default a newly created clip's `voice_cleanup_enabled` to the landing track's own
  `audio_role == Mic`, satisfying the item's own "Mic by default, explicit override elsewhere"
  acceptance criterion — applied only at clip *creation* time; reassigning a track's role later
  never retroactively flips already-placed clips, and the field stays a plain always-overridable
  per-clip toggle either way. The remaining gap (a clip that predates its track's Mic role, or was
  moved there after creation, never got the creation-time default) is covered by a new
  `App::selected_clip_track_audio_role` plus a non-blocking suggestion label in the properties
  panel — never an auto-toggle, just a nudge.

  Verified via `cargo check --workspace --all-targets` (the documented temporary local
  `filters.c` shim, discarded before commit) and `cargo fmt --check`, both clean. New `App`-level
  tests (default-on for a Mic-role track, default-off for a non-Mic track,
  `selected_clip_track_audio_role` reporting the right role/`None`) type-check cleanly under
  `cargo check` — same "can't link this sandbox's `ui` test binary" caveat every other `App`-level
  test addition this session has carried.

  Verified for real, not just type-checked: `crates/avbridge` has zero heavy dependencies (no
  ONNX/whisper/GStreamer), so `cargo test -p avbridge --test audio_mix_test` fully links and runs
  in this sandbox against real fixture audio (the pre-existing FFmpeg-too-old `filters.c` gap
  worked around via the documented temporary local shim, discarded before commit) — three new
  integration tests: cleanup enabled and disabled both mix successfully (`avfilter_graph_config`
  succeeding is the real proof this exact filter syntax is valid against this build's actual
  linked FFmpeg, not just that the C compiles), and — the strongest evidence — an A/B test
  proving the extra stages actually change the encoded output: not just byte-for-byte difference,
  but the compressed file's own measured loudness range (via `avbridge::measure_loudness_json`,
  the same real `loudnorm`-analysis primitive `avcore::loudness` already uses) comes back
  narrower than the uncompressed file's, real confirmation `acompressor` measurably reduced
  dynamic range. `cargo check --workspace --all-targets` (confirming every `ClipInstance`
  construction site across `core`/`ui` — 15 call sites, both production and test code — was
  updated for the five new fields), `cargo clippy -p core --lib --no-deps` / `-p avbridge
  --all-targets --no-deps`, `cargo fmt --check`, and `clang-format --dry-run --Werror` on the
  touched C files all stayed clean.

  **Slice 3 shipped: A/B preview + measured before/after loudness/peak — CF-03's own last open
  acceptance criterion.** Deliberately *not* a live GStreamer preview effect (that gap is
  separately documented above and on `ClipInstance::voice_cleanup_enabled`'s own doc comment —
  most effects' elements only conditionally exist in the running pipeline at all, a materially
  bigger lift than this slice's own scope). Instead, `avcore::voice_cleanup_preview::
  render_voice_cleanup_preview` renders two short (≤6s, `PREVIEW_SAMPLE_MAX_SECS`), disposable
  samples of the clip's own source audio — cleanup bypassed vs. applied at its currently staged
  parameters — through the exact same real mixing path a real export uses
  (`avbridge::mix_audio_timeline`, reusing the item's own already-proven filter chain unchanged),
  then measures each with `avcore::loudness::measure_loudness` (the same primitive used
  elsewhere, not a new analysis path). Both renders use the sequence's own configured
  `target_lufs` so the preview's overall loudness matches what a real export would actually
  produce; the cleanup chain's own contribution still shows up in the measured true peak/
  loudness range even once both land on roughly the same integrated LUFS via the shared
  mastering pass every mix already applies.

  `ui`: the properties panel's Voice Cleanup section gained a "🔊 Prévia A/B" button
  (`App::spawn_voice_cleanup_preview`, the same background-thread + channel + per-frame
  `pump_voice_cleanup_preview()` pattern every other background job in this app already uses,
  disabled while a render for the selected clip is already in flight) and, once a render
  completes, two "▶ Original"/"▶ Tratado" buttons with each sample's measured LUFS/true-peak/
  loudness-range line next to it. Playback (`App::play_voice_cleanup_preview_sample`) opens the
  chosen sample through a dedicated `avcore::preview::Preview` instance
  (`VoiceCleanupPreviewState::player`) kept deliberately separate from the main timeline's own
  preview pipeline (`PreviewState::preview`), so auditioning a sample never disturbs the
  timeline's playhead/pipeline state. The result is tagged with the clip id it belongs to, so
  switching the selected clip never shows a stale A/B comparison for a different clip's
  parameters.

  Verified for real, not just type-checked: this session's sandbox has a working path to a
  fully-linked, real-executing `avbridge` (`rustup update stable` past a too-old-bundled-rustc
  block, `apt-get install libavfilter-dev`/`libgstreamer*-dev` past missing-headers/pkg-config
  gaps, plus the documented temporary local `filters.c` shim — this pass additionally needed the
  identical shim applied to `text_overlay.c`'s own unrelated `av_opt_set_array` call site purely
  so the rest of the crate's object files satisfy the linker, since that call is never actually
  reached under the shim — both discarded before commit). `cargo check --workspace --all-targets`
  passed clean via the shim; `crates/core/src/av/voice_cleanup_preview.rs` (depends only on
  `avbridge` + `crate::loudness`/`crate::media`, none of core's heavy ONNX/whisper/GStreamer
  dependencies) was copied into a throwaway scratch crate that depends on the *real* `avbridge`
  crate directly (a path dependency, not a copy) and `cargo test`ed there for real: 4/4 passing,
  including the same real-difference assertion `avbridge`'s own `audio_mix_test.rs` already
  established for this filter chain (the processed sample's measured loudness range comes back no
  wider than the bypassed one's — real evidence the compressor/limiter chain actually ran, from a
  real AAC encode through the real linked FFmpeg build, not just that the C compiles). New
  `App`-level tests (`spawn_voice_cleanup_preview` marks the selected clip rendering and is a
  no-op while one is already in flight; `pump_voice_cleanup_preview` stores a tagged result and
  clears the in-flight marker on success, toasts without storing a result on failure) type-check
  cleanly under `cargo check` — this sandbox's `ui` bin test target still can't *link* (the
  pre-existing ONNX Runtime gap), same caveat every other `App`-level test addition in this
  environment already carries. `cargo fmt --check` and `cargo clippy -p core --lib --no-deps` /
  `-p ui --bin ui --no-deps` / `-p ui --tests --no-deps` (no new warnings in any touched file)
  both stayed clean.
- `[x]` **CF-04: dynamic auto-reframe.** Track a face/selected subject and generate reviewed,
  smoothed crop/position keyframes for vertical exports and Shorts Pack.

  **Slice 1 (trajectory-to-keyframes core + basic `ui` trigger) shipped.** `avcore::dynamic_reframe`
  is the pure-logic half: `fill_reframe_gaps` holds a short (≤3 consecutive samples) detection gap
  at the last-known center and falls back to `None` (centered framing) on a longer one or a
  leading gap; `smooth_subject_centers` is a centered moving average over detected samples only
  (`None` passes through unchanged); `sparse_crop_keyframes` converts the resulting per-sample
  `CropRect` trajectory into the four `ClipInstance::crop_x/y/w/h_keyframes` lists, sparsified per
  axis independently (only emits a point past a 0.01 epsilon change, always keeps first/last).
  Detection is unchanged — reuses `avcore::detect_faces`/`main_subject_center`/
  `compute_reframe_crop` exactly as the existing static auto-reframe does, one call per sample, no
  second ONNX surface. Sampling itself reuses `avcore::FrameSampler::even_sample_times` (same
  primitive motion tracking already uses) rather than a new cadence primitive — an initial
  implementation duplicated it as `reframe_sample_times`/`MAX_REFRAME_SAMPLES`, caught and removed
  before commit per `spec/RULES.md`'s reuse-before-building rule.

  `ui`'s `App::spawn_dynamic_reframe_selected_clip` (`crates/ui/src/app/dynamic_reframe.rs`) opens
  one `FrameSampler`, samples at 2/sec (bounded 4–30 samples — lower than motion tracking's 4/sec
  since ONNX inference is costlier per-sample than block matching), decodes+downscales each frame
  via a helper (`downscale_frame_rgba`) factored out of the static auto-reframe module rather than
  duplicated, and pipes the result through the three `avcore` functions above into a single
  `App::set_selected_clip_crop_keyframes` call (new — replaces all four crop keyframe lists in one
  undo snapshot, unlike the properties panel's own four independent per-axis setters). Triggered by
  a new "Reenquadramento dinâmico" button next to the existing static "Reenquadramento automático"
  one in the properties panel's Crop section, disabled while a run (static or dynamic — separate
  in-flight state, they don't share one) is active.

  **Deliberately not done at the time of this slice** (since shipped — see this entry's own later
  notes for the seed point, Shorts Pack integration, and content-adaptive cadence): there is no
  dedicated review/correction UI — the emitted keyframes land directly in the properties panel's
  existing Crop X/Y/W/H Keyframes sections (already editable there), the same "existing UI is the
  review step" precedent D4's chapter-marker detection established, not a bespoke accept/reject
  modal. Still open, a real, separate follow-up.

  Verified via `cargo check --workspace --all-targets` (the documented temporary local `filters.c`
  shim, discarded afterward) and `cargo fmt --check`, both clean. `avcore::dynamic_reframe`'s own
  11 pure-logic tests (gap-filling, smoothing, sparsification — no `avbridge`/GStreamer/ONNX
  dependency) were run for real via the scratch-crate technique, not just type-checked, and pass.
  Not run against a live GUI session or a real ONNX model (no display, no network to fetch the
  bundled face-detector model in this sandbox) — the button's actual behavior and the smoothing
  window's real-footage quality need a manual pass on a real dev machine.

  **Content-adaptive cadence now shipped too**, closing the "cadence is bounded but not content-
  adaptive (no scene-cut-aware sampling)" gap this slice originally left open. `avcore::
  dynamic_reframe::augment_sample_times_for_cuts(sample_times, cuts, extra_per_cut)` thickens the
  fixed-cadence sample-time list around a detected hard cut
  (`crate::scene_detection::detect_scene_cuts`, reused rather than a second detector) with
  `DEFAULT_EXTRA_SAMPLES_PER_CUT` (2) evenly-spaced extra times inserted strictly inside the gap
  immediately preceding the cut — without this, the crop keyframes either side of that gap and
  `sparse_axis_keyframes`'s own linear interpolation would visibly smear the framing change across
  the whole fixed-cadence interval instead of snapping to it. `ui`'s `dynamic_reframe_one` reuses
  each sample's *already-decoded* RGBA frame (converted via `avcore::rgba_to_gray`) for the
  scene-cut scan — zero extra decode cost in the common no-cuts case — then, only when a cut is
  actually found, runs a second small bounded pass (`extra_per_cut` × cut count, not unbounded)
  decoding+detecting at just the newly inserted times before re-sorting the full sample list and
  continuing through the unmodified gap-fill/smooth/sparsify pipeline. A manual seed point (the
  earlier CF-04 follow-up) still takes priority and skips this entirely — a user-pinned anchor has
  no cuts to adapt around.

  Verified for real, not just type-checked: since `augment_sample_times_for_cuts` depends only on
  its own `SceneCut` parameter (no `avbridge`/GStreamer/ONNX), it was copied into a throwaway
  scratch crate and `cargo test`ed there for real: 8/8 passing — a no-op with no cuts, zero
  `extra_per_cut`, or fewer than two samples; evenly-spaced insertion within the correct preceding
  gap; a cut at the very first sample (no preceding gap) correctly ignored; a cut matching no real
  sample time ignored; multiple cuts handled without duplicate times; and the result staying
  strictly ascending under a stress case with a cut at every gap. Mirrored as the same 8 tests in
  the real `crates/core/src/analysis/dynamic_reframe/dynamic_reframe_test.rs`. `cargo check --workspace
  --all-targets` and `cargo clippy -p core --lib --no-deps` / `-p ui --tests --no-deps` (via the
  documented temporary `filters.c`/`text_overlay.c` shim, discarded before commit) and `cargo fmt
  --all -- --check` all stayed clean.

  **Cross-sample subject continuity now shipped too.** The gap this entry's own "deliberately not
  done" note flagged — "subject selection has no cross-sample identity tracking (two people
  trading highest-confidence would visibly re-target between samples, damped but not corrected)"
  — is closed. `avcore::dynamic_reframe::select_subject_center` prefers whichever detected face
  continues the *previous* sample's own chosen subject (within
  `DEFAULT_CONTINUITY_MAX_DISTANCE = 0.25`, a frame-fraction distance) over `main_subject_center`'s
  single-sample "highest score always wins" rule, falling back to the plain highest-score face
  when nothing continues (no previous center yet, the previous subject left frame, or a real
  scene/subject change) — so a genuine change still gets picked up rather than clinging to a stale
  position forever. Deliberately a new function, not a change to `main_subject_center` itself,
  which the static (single-sample) auto-reframe path still uses unchanged — continuity across
  samples is meaningless for a one-shot detection. `ui`'s `dynamic_reframe_one` threads a
  `previous_center` accumulator across its sequential sample loop, updated only on an actual
  detection (a gap-filled/centered sample carries no new evidence about where the subject is now).

  Verified for real, not just type-checked: all 16 `dynamic_reframe` tests (5 new — continuity
  preferred over a higher score elsewhere, fallback when nothing continues, a continuity tie still
  breaking by score, the no-faces/no-previous-center edge cases) ran in a scratch crate (a
  stripped `FaceBox`/`CropRect`/`Keyframe` stand-in, avoiding the real `auto_reframe.rs`'s
  `ort`/ONNX dependency this module doesn't otherwise need) and pass. `cargo check --workspace
  --all-targets`/`cargo fmt --check` (via the same documented temporary shim) both stayed clean.

  **Shorts Pack integration now shipped, closing this entry's last flagged gap.** Building a
  Shorts Pack (`App::spawn_shorts_pack`, `crates/ui/src/app/shorts_pack.rs`) now runs an
  auto-reframe pre-pass before queuing any vertical exports: `clips_needing_reframe` (new, pure
  logic) scans every video-track clip overlapping any highlight-marker window (via
  `sorted_highlight_positions`, itself new — sorted, deduplicated `MarkerKind::Highlight`
  positions) and collects the ids of clips that don't already have crop keyframes
  (`ClipInstance::has_crop_keyframes`, pre-existing), deduplicated across overlapping windows.
  When that list is non-empty, `spawn_shorts_pack` pushes a single `push_undo_snapshot()` for the
  whole batch up front (deliberately not `push_undo_snapshot_for_drag`'s pointer-release-gated
  coalescing — that mechanism is built for continuous UI drags and would be a fragile fit for a
  background-thread-driven, no-pointer batch), stores the pending clip ids in a new
  `ShortsPackReframeState { pending_clip_ids, output_dir }` field on `App`, and starts the first
  job via `spawn_next_shorts_pack_reframe`; the actual export queuing (`queue_shorts_pack_exports`,
  the pre-existing logic, now a private helper) only runs once every clip in the pre-pass list has
  a result, success or failure.

  Reuses `DynamicReframeState`'s existing single-in-flight-job guard rather than adding a second
  background-job mechanism: `App::spawn_dynamic_reframe_for_clip(&mut self, clip_id: u64)` (new,
  factored out of the existing `spawn_dynamic_reframe_selected_clip` wrapper) can now target any
  clip id, not just the selected one, and `pump_dynamic_reframe()` branches on
  `is_shorts_pack_reframe_target(clip_id)` to route a finished job's result through
  `apply_shorts_pack_reframe_result` (direct clip-id mutation, bypassing the selection-gated
  `set_selected_clip_crop_keyframes` and its live-preview push — a shorts-pack target is
  essentially never the currently selected/previewed clip) and `advance_shorts_pack_reframe_queue`
  (pops the queue, starts the next pending clip via `spawn_next_shorts_pack_reframe`, or falls
  through to `queue_shorts_pack_exports` once the queue is empty) instead of the ordinary
  selected-clip path. Fixed a real, previously-latent gap surfaced while wiring queue advancement:
  `DynamicReframeEvent::Failed` carried no `clip_id`, so a failure couldn't be attributed to a
  specific clip — added the field and updated its one send site (`dynamic_reframe_one`); a failed
  reframe still advances the queue (falls back to whatever crop the clip already had) rather than
  aborting the whole Shorts Pack build.

  **Deliberately not done**: no dedicated review/correction UI for the auto-triggered reframes
  (same "existing UI is the review step" precedent as slice 1 — results land in the properties
  panel's already-editable Crop Keyframes sections) and no optional user-provided seed point;
  both remain open, separate follow-ups, matching the original spec's own harder asks.

  Verified for real: 7 new pure-logic tests for `clips_needing_reframe`/`sorted_highlight_positions`
  (overlap, exclusion by existing keyframes, exclusion by no overlap, non-video-track exclusion,
  dedup across one clip spanning two windows, dedup across distinct clips, highlight-only
  sort/filter) ran in a scratch crate and pass; 3 new `App`-level tests exercise the pre-pass
  gating (`app_test.rs`). `cargo check --workspace --all-targets` (the documented temporary
  `filters.c` shim, discarded afterward) and `cargo fmt --all -- --check` both stayed clean, no
  new warnings beyond the established baseline. Not run against a live GUI session (no display in
  this sandbox) — the end-to-end queue-advancement behavior on real footage needs a manual pass on
  a dev machine.

  CF-04 is now considered complete for this phase; remaining harder asks (content-adaptive
  cadence, optional seed point, a dedicated review/correction UI) are tracked as open follow-ups
  rather than blocking this item.

  **The "optional user-provided seed point" follow-up now shipped too.** `ClipInstance` gains
  `reframe_seed_point: Option<(f32, f32)>` (`#[serde(default)]`, `None` — the default, and every
  clip saved before this field existed — keeps the exact old face-detection-only behavior). When
  set, both `App::spawn_auto_reframe_selected_clip` and `App::spawn_dynamic_reframe_for_clip`
  (the shared core `App::spawn_shorts_pack`'s own pre-pass already goes through) use it directly
  as the crop anchor and skip face detection entirely — no frame decode, no ONNX inference, and
  no configured model required at all, closing a real, related gap this pass found while wiring
  it in: previously *any* reframe run demanded `prefs.reframe_model_path`, even one that would
  now use a manual anchor instead. `compute_reframe_crop` itself needed no change — it already
  took an `Option<(f32, f32)>` `subject_center` parameter; only *where that value comes from* was
  the gap. For the dynamic (multi-sample) path, every sample is fixed to the same pinned point
  rather than a per-sample detection loop — correct since a manually pinned anchor has no
  trajectory to track — which the existing gap-fill/smooth/sparsify pipeline degenerates
  correctly to a single constant crop for, no separate code path needed.

  A real design decision made before writing code, per `.claude/CLAUDE.md`'s own "design before
  code" rule: rather than a new click-on-the-video-preview interaction (a real, unverifiable-in-
  this-sandbox risk — no way to visually confirm hit-testing against a rendered texture with no
  display available here), the seed point is edited as a plain "Ancorar manualmente" checkbox
  plus X/Y `DragValue` sliders in the properties panel's existing Crop section, reusing the exact
  widget pattern `crop_x`/`crop_y` already establish right above it — no new interaction paradigm,
  fully consistent with this codebase's own components.

  Verified: 6 new `App`-level tests in `app_test.rs` — the setter stores/clamps/clears the field
  correctly and is a no-op with nothing selected, a clip carrying a seed point skips the
  "no model configured" toast even with an empty `reframe_model_path`, and a clip without one
  still requires a configured model as before. `cargo check --workspace --all-targets` and `cargo
  clippy -p core --lib --no-deps` / `-p ui --tests --no-deps` (via the documented temporary
  `filters.c`/`text_overlay.c` shim, discarded before commit) and `cargo fmt --all -- --check` all
  stayed clean. Not run against a live GUI session (no display in this sandbox) — the checkbox/
  slider's actual on-screen appearance and the background thread's real skip-detection behavior
  against real footage need a manual pass on a dev machine, same caveat this item's own prior
  slices already carry.
- `[~]` **CF-05: OpenTimelineIO interchange.** Round-trip the supported editorial subset and
  emit an explicit compatibility report for unsupported effects.

  **Slice 1 (the `avcore::interchange` boundary, plus the structural half of slice 2 and an
  early slice 4) shipped.** `avcore::interchange` adds a schema-neutral intermediate
  representation — `RationalTime`/`TimeRange`/`InterchangeTimeline`/`InterchangeTrack`/
  `InterchangeClip`/`InterchangeMarker` — and `sequence_to_interchange(sequence, project)` maps
  a `Sequence`'s `Timeline` into it: video/audio track order (`Text`/`Shape` overlay tracks
  omitted — not part of an editorial cut), each clip's speed-adjusted source range, an explicit
  `InterchangeTrackItem::Gap` wherever there's on-timeline space before/between clips (most
  interchange formats require contiguous track children, unlike `Track::clips`' own sparse
  `start_secs`-addressed model), markers as zero-duration point ranges, transition kind, and a
  resolved `MediaReference` — `External` when `ClipInstance::asset_id` still resolves in the
  project's media library, `Missing` otherwise (a deleted asset, or a compound clip's
  `nested_sequence_id`) — the doc's own "missing media produces offline references rather than
  dropping clips" acceptance criterion.

  **Follow-up: real OpenTimelineIO JSON serialization now shipped**, the exact gap this slice
  originally deferred for lack of network access to verify OTIO's real wire format against. A
  later session had real network access, so `avcore::interchange::otio_json` writes an actual
  `.otio` document — and, critically, verified it against the **real reference implementation**,
  not just sample files eyeballed off GitHub: `pip install opentimelineio` (0.18.1, the current
  stable release) and `read_from_file` on oca's own generated output, for real, in this
  environment. This caught a genuine mismatch a docs-only check would have missed: an initial
  attempt used `Marker.3` (a nested `Color.1` object), matching a sample fetched from the
  OpenTimelineIO GitHub repo's `main` branch — but 0.18.1's own `otio.schema.Marker()`
  constructor emits `Marker.2` (a plain string `color` field, plus a `comment` field) and
  rejected the `Marker.3` file outright with `UnsupportedSchemaError` when loaded back through
  the real library. `main`'s own sample data was ahead of what's actually released/installable —
  fixed to `Marker.2`, then re-verified clean: every object type oca emits (`Timeline.1`/
  `Stack.1`/`Track.1`/`Clip.1`/`Gap.1`/`ExternalReference.1`/`MissingReference.1`/`Marker.2`/
  `TimeRange.1`/`RationalTime.1`) loads through `opentimelineio.adapters.read_from_file` with the
  correct track order, clip timing, media reference resolution (both `ExternalReference` and
  `MissingReference` cases), and marker color/timing intact, and the loaded timeline
  round-trips cleanly back through OTIO's *own* JSON writer. `Clip.1` (a single `media_reference`
  field, matching `MediaReference`'s own single-reference shape) was deliberately kept over the
  newer `Clip.2` (a multi-reference map) even though 0.18.1 constructs `Clip.2` by default —
  confirmed for real that 0.18.1's reader still accepts `Clip.1` (backward compatibility, not an
  assumption; tested directly against the OTIO project's own `simple_cut.otio` fixture). 9 new
  `cargo test -p core --lib` cases (480/480 total passing, this environment's FFmpeg/GStreamer/
  ONNX toolchain fully links here) cover every schema tag, both media-reference variants, marker
  color mapping, an empty timeline, and JSON well-formedness — on top of the real-library
  end-to-end check above, which isn't (and can't be) part of the Rust test suite itself.

  **Still not attempted: the reverse import direction** (reading an arbitrary real-world `.otio`
  file from Resolve/Premiere/the reference implementation) — a genuinely larger, riskier scope
  than verifying export against known-good samples: unknown schema versions, `Clip.2`'s
  multi-reference form, nested `Stack`s, vendor-specific metadata. Left as CF-05's next slice.
  `sequence_to_interchange` also still uses a fixed microsecond-resolution time rate rather than
  each clip's own native probed frame rate — unrelated to the serialization gap this follow-up
  closed, a separate remaining placeholder.

  **UI wiring**: the Editor menu bar's File menu gained "Export OpenTimelineIO (.otio)..."
  (`App::export_otio_for_active_sequence`, mirroring `export_collab_bundle`'s own "build the
  payload in `core`, this method just turns the `Result` into a toast or a written file" shape)
  — a real save-file dialog, not just a library function nothing in the app can reach yet.

  **Slice 4's compatibility report shipped early too** (before any real file gets written, since
  the report only needs to know what *would* survive interchange): `interchange_compatibility_
  report(sequence)` walks every clip and reports, with track/clip context per the doc's own
  acceptance criterion, which of a curated list of in-use visual/audio effect fields (crop,
  mask, flip, color grading, sharpen, chroma key, blur/shake/glitch/pixelize, vignette, 3D LUT,
  stabilization, deflicker, background removal, voice cleanup, position/scale/rotation/opacity/
  gain animation, nested sequences) are omitted, a transition present as approximated (kind
  carried, not yet real in/out-offset semantics), and overlay tracks as omitted outright.

  Verified for real, not just type-checked: since `interchange.rs` depends only on `crate::
  project`/`crate::timeline` (no `avbridge`/GStreamer/ONNX), it was copied unmodified into a
  throwaway scratch crate alongside stripped stand-in `Project`/`Sequence`/`MediaAsset`/
  `Timeline`/`Track`/`ClipInstance`/`Marker` types (same "dynamic_reframe`'s own stripped
  `FaceBox`/`CropRect`/`Keyframe` stand-in" technique this session has used before) and
  `cargo test`ed there for real: 17/17 passing, covering track order/kind filtering, source-range
  mapping, media-reference resolution (both present and missing-asset cases), leading/middle/
  no-gap insertion, out-of-order-clip sorting, speed/transition carrying, marker mapping, and
  every compatibility-report category. `cargo check --workspace --all-targets` and `cargo clippy
  -p core --lib --no-deps` (via the documented temporary `filters.c` shim, discarded before
  commit) and `cargo fmt --check` all stayed clean.

  **Slice 3 (the import/reverse direction) shipped too, scoped to this module's own intermediate
  representation.** `interchange_to_timeline(interchange, project, next_id)` reconstructs a
  `Timeline` from an `InterchangeTimeline` — walking each track's items in order (a `Gap` just
  advances the reconstruction cursor; a `Clip` gets placed at the cursor's current position),
  resolving each clip's `MediaReference` back to an asset id by matching `target_url` against the
  target project's own media library (`resolve_asset_id`, the reverse of
  `resolve_media_reference`), and rebuilding a `ClipInstance` with only what `InterchangeClip`
  itself carries (source range, speed, transition kind) — every other field (crop, color grading,
  masks, every other effect/keyframe animation) at its own untouched default, matching what
  `interchange_compatibility_report` already says doesn't round-trip. Takes `next_id: &mut u64`
  so every allocated `Track`/`ClipInstance`/`Marker` id increments it in place, letting a caller
  keep allocating unique ids afterward without recomputing a high-water mark.

  A clip whose `MediaReference` doesn't resolve against the target project (an asset genuinely
  offline, or importing into a different project than the one exported) is skipped — never given
  a placeholder/sentinel asset id — and reported in `InterchangeImportResult::warnings` with
  track context instead, extending the doc's own "preserve unsupported fields as warnings, never
  silently approximate them" rule to unresolvable references as well as unsupported fields.

  Verified for real, not just type-checked: 6 new tests (round-tripping `sequence_to_interchange`
  through `interchange_to_timeline` and back) ran in the same scratch-crate setup slice 1 used,
  25/25 total passing — no-timing-drift round-trip for a single clip, **no accumulated drift
  across a 500-clip long sequence with gaps between every clip** (the doc's own "without timing
  drift over long sequences" acceptance criterion, checked directly rather than assumed), speed/
  transition carrying, a warning-and-skip for an unresolvable reference, strictly-increasing
  unique id allocation across tracks/clips advancing the caller's counter, and marker round-trip.
  `cargo check --workspace --all-targets`, `cargo clippy -p core --lib --no-deps` (via the
  documented temporary `filters.c` shim, discarded before commit), and `cargo fmt --check` all
  stayed clean.

  **Real `.otio` import now shipped too**, closing this entry's own previously-open "reverse
  direction" gap. `avcore::interchange::otio_json::parse_otio_json` reads a real OpenTimelineIO
  JSON document into an `InterchangeTimeline` — verified against three real files fetched from
  the OpenTimelineIO project's own `tests/sample_data/` (`simple_cut.otio`, `transition.otio`,
  `multitrack.otio`, now vendored under `crates/core/tests/fixtures/otio/`), each one first
  loaded through the real `opentimelineio` 0.18.1 Python library to confirm what it actually
  contains before writing the parser against it. Two real-world shapes this module's own writer
  never produces needed that verification specifically: a `Transition.1` item carries no
  `source_range` of its own (confirmed against `transition.otio`'s real `in_offset`/`out_offset`
  and its neighboring clips' own ranges — the overlap comes out of the two clips it sits between,
  not extra timeline duration), so it attaches as `transition_in` on the following clip without
  advancing the track's own timing cursor; and a `null` `Clip.source_range` means "use the media
  reference's own `available_range`" (confirmed against a real `Clip-004` in `simple_cut.otio`
  and `transition.otio` both), not a malformed clip. A nested compound clip (a `Stack.1`/
  `Track.1` sitting directly among a track's own children, confirmed against the real
  `nested_example.otio`) isn't recursively imported yet — a separate, later slice — but is
  replaced with a same-duration `Gap` when its own `source_range` is explicit, so later siblings
  on the same track still keep their correct start time. Anything else structurally unrecognized
  (a `Clip.2` with multiple media references — only the active one is kept — an unrecognized
  media-reference schema, a malformed individual clip/marker) completes import and lands in
  `OtioImportResult::warnings` instead of failing the whole document, matching
  `InterchangeImportResult::warnings`'s own convention; only a genuinely malformed top-level
  document (not `Timeline.1`/`Stack.1` at all, or missing a structurally required field) is a
  hard `OtioParseError`.

  **UI wiring**: the Editor menu bar's File menu gained "Import OpenTimelineIO (.otio)..."
  (`App::import_otio_into_new_sequence`) — places the imported content into a brand-new sequence
  tab (`App::add_sequence`'s own "append and switch to it" shape), never overwriting an existing
  one; every clip's media reference is resolved against the *active project's* own media library,
  and an unresolvable one is skipped and counted in the resulting toast rather than silently
  linked to the wrong asset.

  Verified for real, fully linked: 17 new/updated tests in `otio_json_test.rs` (a round-trip
  through oca's own writer, the three real fixture files above, and synthetic cases for a
  malformed document, an unrecognized media-reference schema, an unsupported nested item, and a
  multi-reference `Clip.2`) — `cargo test -p core --lib interchange::otio_json`, 17/17 passing;
  full `cargo test -p core --lib`, 527/527 passing. 3 new `App`-level tests (successful import
  creates and switches to a new sequence with the right clip/asset mapping, malformed JSON toasts
  without adding a sequence, an unresolvable reference is skipped and counted in the toast) —
  `cargo test -p ui`, 427/427 passing. `cargo fmt`/`cargo clippy -p core --lib --no-deps` both
  clean, and a direct launch of the rebuilt `ui.exe` confirmed no startup crash.

  **The "imported paths are normalized and cannot escape an explicitly selected media root"
  acceptance criterion now shipped too.** `resolve_asset_id`'s existing exact-path-string match
  (the common case: this project already carries the asset at the exact path the exporting
  machine recorded) now falls back, when it fails and an optional `media_root` is given, to
  `resolve_under_media_root(media_root, target_url)` — a new safety primitive that extracts only
  `target_url`'s final path segment, joins it under `media_root`, then canonicalizes both sides
  (resolving any symlink) and requires the candidate to still start with the canonicalized root.
  This is the cross-machine/cross-OS relink case: a `.otio` exported on one machine and imported
  on another where the same file already exists in the importing project's own media library
  under a different absolute path — never an auto-import of new, previously-unseen media (that
  would need this app's own async probe pipeline, a real, separate follow-up; this slice only
  re-anchors matching against assets the project already has). `interchange_to_timeline` gained
  the `media_root: Option<&Path>` parameter threaded through; the Editor's File menu gained a
  second "Import OpenTimelineIO with media root..." action (`App::import_otio_into_new_sequence`'s
  existing single-path button is untouched, still passing `None`) that additionally prompts for a
  folder before importing.

  **Real bug found and fixed by running the path-safety primitive against a real filesystem,
  not just type-checking it**: an initial implementation extracted the final segment via
  `Path::file_name()`, which only recognizes the *host's own* separator convention — a
  Windows-style `\`-separated `target_url` parsed on this Linux sandbox (where `\` isn't a path
  separator at all) came back as one giant unsplit "file name" that simply failed to exist under
  `media_root`, silently defeating the exact cross-machine relink case this primitive exists for.
  Fixed by splitting `target_url` on both `/` and `\` explicitly regardless of host OS, with an
  added explicit rejection of an empty/`.`/`..` final segment (`media_root.join("..")` would
  climb to the parent directory despite containing no separator of its own, so this check has to
  run before candidate construction, not only after canonicalizing).

  Verified for real against a real filesystem (not just type-checked): since this primitive
  depends only on `std::path`/`std::fs` (no `avbridge`/GStreamer/ONNX), it — plus a minimal
  stand-in `MediaAsset`/`Project`/`MediaReference` — was copied into a throwaway scratch crate and
  `cargo test`ed there for real: 11/11 passing, covering a real bare-name match, a real Windows-
  style foreign absolute path correctly reduced to its bare file name (the bug above, caught this
  way), `../`-parent traversal rejected before any filesystem call, a real symlink placed inside
  the root but resolving outside it rejected via `canonicalize()`, a nonexistent file, a directory
  (not a plain file), a trailing separator with no final segment, and bare `.`/`..` segments — plus
  `resolve_asset_id`'s own fallback ordering (exact match preferred over the media-root fallback,
  the fallback engaged only when the exact match fails, and no match invented when the root has
  nothing matching). Mirrored as 9 new tests directly in `crates/core/src/project_io/interchange/
  interchange_test.rs` plus 2 more exercising `interchange_to_timeline`'s new parameter end-to-end
  (resolves via the root when the exact match fails; still reports the existing warning when the
  root has no match either) — type-checked cleanly here (this sandbox's own `core` test binary
  still can't *link*, the same pre-existing ONNX Runtime network gap `CLAUDE.md` documents), with
  the scratch-crate run above standing in as this slice's real-execution proof, same "can't link
  this sandbox's own test binary" caveat every other `core`-side slice this session has hit.
  `cargo check
  --workspace --all-targets`, `cargo clippy -p core --lib --no-deps` / `-p ui --tests --no-deps`
  (via the documented temporary `filters.c`/`text_overlay.c` shim, discarded before commit), and
  `cargo fmt --all -- --check` all stayed clean.
- `[ ]` **CF-06: live multicam monitor.** Show synchronized proxy-backed feeds and materialize
  angle decisions through the existing ordinary clip-split representation. **Deliberately skipped
  for now** (user-confirmed): every one of its 4 implementation slices needs a live GStreamer
  pipeline with real hardware/display to verify frame sync, preview/program distinction, and
  decode-capacity degradation — this sandbox has neither, and writing that code with only a
  type-check as verification would carry real risk of shipping subtly wrong pipeline code
  untested. Revisit on a real dev machine, or once a display becomes available in this
  environment.
- `[~]` **CF-07: parameterized motion-graphics templates.** Add a declarative, script-free,
  versioned asset format for reusable channel graphics and aspect-ratio variants.

  **Slice 1 (the versioned JSON format itself) shipped.** `avcore::motion_template` adds
  `GraphicTemplate` — `schema_version`, `name`, `canvas_width`/`canvas_height`, a
  `Vec<TemplateParameter>` (named, typed editable slots: `Text`/`Color`, the two the doc's own
  slice 1/2 split lists first), and a `Vec<TemplateElement>` of allowlisted primitives.
  Deliberately just `TemplateElement::Text`/`::Shape` for this slice — the exact two overlay
  kinds `crate::timeline`/`crate::overlay_render`/`crate::shape_render` already render, per
  `spec/RULES.md`'s reuse-before-building rule; an `Image` primitive (the doc's own slice 2
  scope) has no existing overlay-clip kind to reuse yet (`TrackKind` has no `Image` variant), so
  it stays a real, separate follow-up rather than inventing new overlay-rendering infrastructure
  this slice was never meant to cover. `TemplateElement` is a closed enum (no `#[serde(other)]`,
  matching `GameplayEventKind`'s own CF-02 precedent) — an unrecognized primitive kind fails to
  deserialize outright. `GraphicTemplate::validate` covers the rest of the doc's "unknown
  primitives/parameters are rejected rather than executed or ignored" acceptance criterion:
  schema version, positive canvas size, unique parameter/element ids, positive font
  size/shape extent, and every `TextBinding::Parameter`/`ColorBinding::Parameter` reference
  resolving to a declared parameter of the matching kind. Every field is a plain literal or a
  named parameter reference — no scripts or executable expressions anywhere in the format, per
  the doc's own explicit constraint for this whole feature. `GraphicTemplate::parse_and_validate`
  is the one recommended untrusted-input entry point, mirroring `EventSidecar::parse_and_validate`'s
  own established shape.

  `instantiate(template, values)` makes "editable parameters" real, not just declared: resolves a
  validated template's `Text`/`Color` bindings against caller-supplied `ParameterValue`s into
  placement-ready `InstantiatedElement`s, failing on a missing or wrong-kind value. Deliberately
  timeline/track-agnostic — building an actual `TextClip`/`ShapeClip` from an `InstantiatedElement`
  and placing it on a track (assigning its own id/`start_secs`/`duration_secs`) is left to a
  `ui`-side follow-up slice, the same "core stays UI-agnostic" boundary every other feature in
  this crate keeps.

  Templates serialize via plain `serde_json`, not this crate's usual gzip-MessagePack `.ocproj`
  framing — deliberate, since a template is meant to be an inspectable, shareable asset file
  (the doc's own slice 3 "package templates as data" goal), not just internal persisted state.

  Verified for real, not just type-checked: since `motion_template.rs` depends only on
  `crate::timeline`'s three plain enums (`ShapeKind`/`TextFontFamily`/`TextFontStyle`, no
  `avbridge`/GStreamer/ONNX), it was copied unmodified into a throwaway scratch crate — this time
  with the *real* `serde`/`serde_json` dependencies rather than stand-ins, since the format's own
  JSON round-trip and closed-enum rejection are exactly what needed proving — and `cargo test`ed
  there for real: 17/17 passing, covering every validation rule, a real JSON round-trip through
  `serde_json::to_string`/`parse_and_validate`, an unrecognized primitive kind actually failing
  JSON deserialization (not just asserted to), and `instantiate`'s fixed/parameter-bound
  resolution, missing-value, wrong-kind-value, and validate-before-touching-values behavior.
  `cargo check --workspace --all-targets` and `cargo clippy -p core --lib --no-deps` (via the
  documented temporary `filters.c` shim, discarded before commit) and `cargo fmt --check` all
  stayed clean.

  **Slice 2 (two of its five pieces) shipped too.** `safe_area_violations` is a non-blocking
  design-time check — never a `GraphicTemplate::validate` failure, since a template legitimately
  wanting a full-bleed background or edge-anchored element is a real, valid design choice this
  shouldn't forbid (same "warn, don't block" precedent `scan_bidi_controls` established for a
  different feature) — flagging any `Text`/`Shape` element whose position/bounding box intrudes
  into the new `GraphicTemplate::safe_area_margin` fraction of the canvas edge. A text element
  has no baked width in this format (no shaping happens until a `ui`-side apply step), so this
  only checks its anchor point clears the margin, not the full rendered extent — a real,
  documented simplification. A shape's bounding box is conservatively approximated as a square of
  side `max(width, height)`, which the true rotated extent can only shrink toward, never exceed —
  cheap and rotation-safe at the cost of occasionally over-flagging. `#[serde(default)]` so a
  template authored under slice 1 loads with the exact same never-flagged behavior it always had.

  `TemplateFamily` is CF-07's own "aspect-ratio variants," modeled as sibling `GraphicTemplate`s
  rather than one template auto-adapting its own layout across canvas shapes — a lower third
  designed for 16:9 and one designed for 9:16 are, in practice, different layouts (different
  element placement, not just a rescale). Reuses `crate::export::ExportAspectRatio` (already this
  crate's own aspect-ratio vocabulary) to tag each variant rather than inventing a second
  aspect-ratio type, per `spec/RULES.md`'s reuse-before-building rule.
  `TemplateFamily::validate` checks every variant's own validity plus the family's own
  constraints (at least one variant, no aspect ratio repeated); `variant_for` looks one up by
  aspect ratio, matching a sequence's own `SequenceExportSettings::aspect_ratio`.

  **Deliberately not done**: `Image` primitive and timing (animation-in/out), the rest of slice
  2's own five-item list; packaging templates with their own validated media assets and the
  `ui`-side apply/preview flow (slice 3); and migration tests for a second schema version, which
  has no reason to exist yet (slice 4). All real, separate follow-ups.

  Verified for real: 10 new tests (5 for `safe_area_violations` — no-op at zero margin, a text
  anchor and a shape bounding box each flagged/accepted correctly; 5 for `TemplateFamily` — empty
  rejected, duplicate aspect ratio rejected, a variant's own validation error propagated with
  context, distinct-aspect-ratio variants accepted, and `variant_for` lookup) ran in the same
  scratch-crate setup, this time also carrying a real stand-in `ExportAspectRatio` — 27/27 total
  passing. `cargo check --workspace --all-targets`, `cargo clippy -p core --lib --no-deps` (via
  the documented temporary `filters.c` shim, discarded before commit), and `cargo fmt --check`
  all stayed clean.

  **`ui`-side apply flow (part of slice 3) shipped too.** `App::apply_graphic_template`
  instantiates a validated `GraphicTemplate` and places every resolved element onto the timeline
  at the current playhead: every `Text` element on the first-or-created text track, every `Shape`
  element on the first-or-created shape track — the same "first track of that kind" placement
  `App::add_text_clip`/`App::add_shape_clip` already use for a manually inserted overlay.
  Multiple template elements can share one track without conflict, since nothing in this app's
  timeline model requires same-kind clips to avoid overlapping in time — only each element's own
  template-defined position keeps them visually apart. One `push_undo_snapshot` for the whole
  batch (Shorts Pack's own "one snapshot per batch, not per clip" precedent), never pushed at all
  if instantiation fails or the template has no elements.

  The toolbar's new "🖼 Load graphic template" button (`App::load_graphic_template_from_file`) is
  this slice's real, reachable trigger. A parameterless template applies immediately; one that
  surfaces `instantiate`'s own actionable "no value supplied for parameter ..." toast rather than
  silently applying garbage.

  Verified: 6 new `App`-level tests in `app_test.rs` (a text element placed at the playhead with
  its template-defined position, a shape element on its own track, text and shape elements
  landing on separate tracks, a parameter-bound value correctly resolved, a missing parameter
  value toasting with no timeline change, and exactly one undo snapshot pushed for a
  multi-element batch — confirmed by undoing once and checking the timeline reverts to empty
  *and* no further undo remains) — type-checked cleanly under `cargo check -p ui --tests`, same
  "can't link this sandbox's `ui` test binary" caveat every other `ui`-side slice this session
  has hit. `cargo check --workspace --all-targets` and `cargo clippy --workspace --all-targets`
  (via the documented temporary `filters.c` shim, discarded before commit) both stayed clean —
  clippy's own `dead_code` lint confirmed the new `App` methods are actually reachable (via the
  new toolbar button and the new tests), not orphaned. `cargo fmt --check` also stayed clean.

  **The parameter-fill modal (closing the gap the previous slice's own doc comment flagged) now
  ships too.** Loading a template that declares one or more `TemplateParameter`s stages it in
  `App::pending_graphic_template_apply` — `text_values`/`color_values` pre-seeded with one entry
  per declared parameter (empty string / opaque white) — instead of applying it right away.
  `App::show_apply_graphic_template_modal` (`app/modals.rs`, same modal shape/lifecycle
  `show_apply_layer_template_modal` already established) shows one row per parameter: a plain
  text field for `Text`, `egui::color_edit_button_srgba` (the same widget `shape_clip.rs`'s own
  color row already uses) for `Color`. Confirming (`App::confirm_apply_graphic_template`) builds
  the `ParameterValue` map from the staged values and hands it to `App::apply_graphic_template`;
  cancelling or Escape (`App::cancel_apply_graphic_template`) discards the pending state without
  applying anything.

  Verified: 5 new `App`-level tests (a parameterless template still applies immediately; a
  parameterized one stages instead of applying, pre-seeding `text_values` for every declared
  parameter; an invalid-JSON file toasts with nothing staged; confirming applies with the filled
  values; cancelling discards without touching the timeline) — type-checked cleanly under `cargo
  check -p ui --tests`, same caveat as above. `cargo check --workspace --all-targets` and `cargo
  clippy --workspace --all-targets` (via the documented temporary `filters.c` shim, discarded
  before commit) both stayed clean, confirming the new modal/methods are actually reachable.
  `cargo fmt --check` also stayed clean. CF-07 slice 3 is now considered complete; `Image`
  primitive and migration tests (slice 2's remainder and slice 4) remain open.

  **Slice 2's "timing" (animation in/out) now shipped too, for `Text` elements.**
  `TemplateTextElement` gains `timing: TemplateTiming` (`fade_in_secs`/`fade_out_secs`,
  `#[serde(default)]` — all-zero, i.e. the exact old static-only behavior, so a template authored
  before this field existed applies unchanged). `GraphicTemplate::validate` rejects a negative
  fade duration (`TemplateValidationError::NegativeTiming`); a combined fade time exceeding the
  clip's own placed duration isn't a validation concern (a `TemplateTextElement` has no duration
  of its own to compare against — only a `ui`-side caller knows that, once it decides where and
  how long to place the instantiated clip), so `timing_opacity_keyframes(timing, duration_secs)`
  instead scales both fades down proportionally, guaranteeing the resulting keyframes stay
  ascending rather than producing a reversed/overlapping pair. `App::apply_graphic_template` calls
  it against the placed clip's own 3-second default duration and uses the result as the new
  `TextClip`'s `opacity_keyframes` — no manual keyframe edit needed after applying a template that
  declares timing, closing the gap this entry's own "a template element is a static starting
  point" doc comment (on `TemplateTextElement`/`TemplateShapeElement`) originally left open for
  entrance/exit animation specifically.

  **Text-only, not Shape, in this slice**: `crate::timeline::ShapeClip` has no opacity-keyframe
  field of its own at all (only `center`/`width`/`height`/`rotation` keyframes) — adding one
  would mean touching `shape_render.rs`'s FFmpeg `geq` filter-expression machinery, a materially
  bigger and riskier change than reusing `TextClip::opacity_keyframes`, which already existed;
  left as a real, separate follow-up rather than attempted here.

  Verified for real, not just type-checked: since `motion_template.rs` depends only on
  `crate::export`/`crate::keyframe`/`crate::timeline` (no `avbridge`/GStreamer/ONNX), it — plus
  `keyframe.rs` (also unmodified) and minimal stand-in `export.rs`/`timeline.rs` — was copied into
  a throwaway scratch crate and `cargo test`ed there for real: 36/36 passing (27 pre-existing plus
  9 new), covering the two new validation-rejection cases, zero timing/zero duration each
  producing no keyframes, fade-in-only and fade-out-only each holding full opacity outside their
  own fade window (checked via the real `evaluate_keyframes`, not just asserting the raw keyframe
  list), both fades together producing four strictly-ascending keyframes, an overlapping
  combined fade correctly scaled down to land exactly at the clip's midpoint, and negative inputs
  clamped rather than producing backwards keyframes if this pure function is ever called with
  unvalidated input directly. Mirrored as the same 9 tests in the real
  `crates/core/src/graphics/motion_template/motion_template_test.rs`. `cargo check --workspace
  --all-targets`, `cargo clippy -p core --lib --no-deps` / `-p ui --tests --no-deps` (via the
  documented temporary `filters.c`/`text_overlay.c` shim, discarded before commit), and `cargo fmt
  --all -- --check` all stayed clean.
- `[~]` **CF-08: semantic transcript and visual search.** Build a bounded, versioned local index
  after exact transcript search ships in CF-01. Slice 1 (exact transcript search) shipped as part
  of CF-01 — `avcore::transcript_search`.

  **Slice 2 (the versioned local index) shipped, plus the storage/scoring half of slice 4.**
  `avcore::semantic_index::SemanticIndex` — one `MediaIndexEntry` per indexed asset, keyed by a
  `MediaFingerprint` (a cheap `(file size, mtime, duration)` proxy for "this file's content is
  unchanged," not a true content hash — hashing multi-gigabyte video files on every staleness
  check would be far too slow) and a caller-defined `model_version` string. `needs_reindex`/
  `upsert_entry`/`remove_entry` give the doc's own "index invalidation is deterministic when
  media or model versions change" acceptance criterion as pure functions; `enforce_chunk_budget`
  evicts whole oldest entries (never a partial one, so a search never sees an asset with an
  inconsistent subset of its own spans) until the total indexed-chunk count is back under a
  caller-supplied cap — the doc's own "bounded in CPU, memory, and disk usage" criterion.
  `SemanticIndex::search` ranks stored `IndexedChunk`s by cosine similarity to a query embedding,
  excluding any entry whose `model_version` doesn't match the query's own (a different model's
  embedding space isn't comparable) — every result carries its own `start_secs`/`end_secs`, the
  doc's own "search results seek to the matched moment, not only the containing asset" criterion.
  `combine_search_results` merges same-moment hits from *different* sources (`Transcript`/
  `Visual`/`Metadata`) into one `MatchSource::Combined` result with a boosted score — the doc's
  own "...or a combined score" criterion, adapting `highlight_detection::
  combine_highlight_candidates`'s own CF-02 "evidence from more than one signal deserves a boost"
  idea from binary flags to a continuous score domain (mean of the pair plus a fixed `0.1` bonus,
  clamped to `1.0` — a simple, documented heuristic, not a statistical claim).

  `persistence.rs` gained a new `OCSI` magic/framing (`to_ocsi_bytes`/`from_ocsi_bytes`, same
  compressed-MessagePack shape `OCTR` already established) — embeddings are opaque float vectors,
  not human-inspectable content, so this reuses the compact binary `.ocproj`-style framing rather
  than `motion_template`'s deliberate plain-JSON choice for a shareable asset.

  **Deliberately not attempted: computing any real embedding.** A real semantic-search embedding
  model (text and/or visual) needs network access to fetch model weights and `libonnxruntime` —
  this sandbox has neither (the same `ORT_SKIP_DOWNLOAD=1`/no-network gap `CLAUDE.md` documents
  for `background_removal`/`auto_reframe`). `IndexedChunk::embedding` is an opaque `Vec<f32>` this
  module never produces itself.

  Verified for real, not just type-checked: since `semantic_index.rs` depends only on `std` plus
  `serde`, it was copied unmodified into a throwaway scratch crate alongside the real
  `persistence.rs` (also unmodified, with a minimal stand-in `Project` type satisfying its own
  `save_project_to_file`/`load_project_from_file` helpers' bounds) and real `flate2`/`rmp-serde`
  dependencies, then `cargo test`ed there for real: 20/20 passing — fingerprint/model-version
  invalidation in every direction, entry replace/remove, chunk-budget FIFO eviction (and its
  no-op-within-budget case), cosine-similarity ranking/top-k/model-version exclusion/moment-level
  seeking, `combine_search_results`' merge/no-merge cases (different source, same source left
  unmerged, far-apart spans left unmerged, different media left unmerged, score clamped to `1.0`),
  and a real gzip/MessagePack round-trip through the actual `OCSI` framing (plus confirming `OCSI`
  bytes are correctly rejected by the `OCTR` decoder via its own magic-byte check). `cargo check
  --workspace --all-targets` and `cargo clippy -p core --lib --no-deps` (via the documented
  temporary `filters.c` shim, discarded before commit) and `cargo fmt --check` all stayed clean.

  **Slice 3's planning half now shipped too** — "the incremental representative-frame/
  transcript-chunk sampling pass" this note used to flag as a genuine follow-up. Still
  deliberately stops short of computing anything (no model, no network, no `FrameSampler`/
  transcript-sidecar I/O in this module — that stays the caller's job), but the doc's own three
  acceptance words for this slice ("cancellable, resumable, bounded") are now real, pure logic:
  `plan_representative_frames` (thin wrapper over `frame_sampler::FrameSampler::
  even_sample_times`, reused rather than reimplemented, with its own `min`/`max` sample-count
  clamps) and `plan_transcript_chunks` (groups a transcript's already-ordered words into spans
  bounded by *either* a max time span or a max word count, whichever is hit first, and never
  drops a single word whose own span alone exceeds the bound) decide *which* frame timestamps and
  transcript spans are worth embedding. `IndexingCursor`/`PendingMedia` (plain, `Serialize`able
  data — a caller persists it exactly like a `SemanticIndex` itself) track how far a resumed pass
  has gotten through each queued asset, and `next_indexing_batch` pops up to a caller-chosen
  `max_items` per call across pending assets (front first, so one huge asset can't starve every
  other one), draining and dropping each entry once it's exhausted. Nothing here ever writes to
  `SemanticIndex` itself — a caller only calls `upsert_entry` once it has real embeddings in hand
  — so a cancelled pass (the caller just stops calling `next_indexing_batch`) never leaves the
  index in an inconsistent state.

  Verified for real: 19 new tests (54/54 total for this module) added to the same throwaway
  scratch crate this note's own earlier slice already used — this time with two additional
  minimal stand-ins (`frame_sampler_stub`'s `even_sample_times`, copied verbatim from the real
  `frame_sampler.rs`, since the real `FrameSampler` struct needs a live GStreamer `Preview` to
  open a file, and a `transcript_word_stub::TranscriptWord`, an exact field-for-field copy of the
  real `transcript.rs` struct) — covering frame-sample bounds/clamping, transcript-chunk grouping
  on both the span and word-count bounds (plus the never-drop-a-too-long-word edge case), cursor
  enqueue/replace/finished semantics, batch bounding/draining/multi-asset ordering/resume, and a
  serde round-trip of the cursor itself. **Real finding from this verification**: a first draft of
  the frame-sample test assumed `even_sample_times`' output stays strictly less than the
  requested end time; running it for real against the actual (unmodified) function showed the
  last sample lands exactly *on* the end timestamp whenever more than one sample is requested —
  the test's assumption was wrong, not the already-shipped function, fixed by asserting the
  correct inclusive bound instead. `cargo check --workspace --all-targets`, `cargo clippy -p core
  --lib --tests` (via the documented temporary `filters.c`/`text_overlay.c` shim plus an
  `FFMPEG_DIR`/`lib`-symlink workaround for this sandbox's multiarch FFmpeg package layout, both
  reverted before commit), and `cargo fmt --all -- --check` all stayed clean.
- `[x]` **CF-09: arbitrary-object mask and tracking.** Start with a user-seeded local model and
  privacy blur, reusing the existing matte/model/tracker infrastructure. Shipped end to end —
  see the "CF-09 shipped end to end" note further down for the final UI-wiring slice.

  **Slice 2 (mask propagation between sampled frames, plus a manual-correction affordance)
  shipped in tractable, model-free form.** A real arbitrary-object segmentation model (slice 1)
  needs network access to fetch weights and `libonnxruntime`, same gap CF-08's real-embedding
  slice already hit — but *propagating* an already-seeded mask across frames needs neither, once
  the propagation is rigid translation rather than per-pixel resegmentation. `avcore::
  mask_propagation` reuses `motion_tracking::track_region`'s existing block-matching tracker
  (not reimplemented) to follow one tracked point, then translates every vertex of a caller-
  supplied polygon (any shape, drawn once on the seed frame — arbitrary, not limited to a
  rectangle) by that same per-frame delta. `propagate_mask_by_translation` does the plain case;
  `propagate_mask_with_corrections` lets a caller supply an observed correction at any frame
  index, restarting tracking from the corrected vertices/center from there rather than continuing
  to drift from a source of error already fixed — the doc's own "propagate... and offer manual
  correction at failure points" acceptance shape. The tracker's own sum-of-absolute-differences
  match score (previously computed but discarded — `track_region` only ever returned the
  positions) is now exposed via a new `track_region_with_scores`/`TrackResult` (kept alongside the
  original `track_region`, a thin wrapper over it now, so its own existing callers/tests are
  untouched) and normalized into a `0.0..=1.0` confidence (`match_confidence`) that doubles as
  this slice's own "failure point" signal — a poorly-matched frame is flagged
  `PropagatedMask::needs_correction`.

  Deliberately not attempted: a real segmentation model (slice 1) and anything beyond rigid
  translation (rotation/scale/deformation of the mask shape) — both need either network access
  this sandbox lacks or a materially harder tracking problem, left as genuine, separate
  follow-ups. No UI wiring yet either (an actual lasso/rectangle mask-drawing interaction is a
  new, non-trivial interaction this session can't visually verify — "design before code" per
  `CLAUDE.md`), same category as CF-08's index staying core-only until a real caller exists.

  Verified for real: copied `motion_tracking.rs` and the new `mask_propagation.rs` (both
  unmodified — neither has any `avbridge`/GStreamer/ONNX dependency, only `crate::keyframe`'s
  small `Keyframe`/`Position` types, stubbed field-for-field) into a throwaway scratch crate and
  ran the real test suites there: 28/28 passing, 19 new (8 for `track_region_with_scores`'
  positions-match/first-frame-score-zero/perfect-match/template-dimensions/empty-input cases, 11
  for `mask_propagation` covering `match_confidence`'s edge cases, translation correctness, a
  real low-confidence flag when the tracked content genuinely vanishes from the frame, correction
  restart/out-of-order/out-of-range handling, and the with-no-corrections-matches-plain-
  translation equivalence). **Real finding from this verification**: a first draft of the
  low-confidence test used a uniform mid-gray "vanished" frame, expecting a poor match — the
  actual measured confidence came back `0.82` (not low), because this synthetic block's own texture
  values average close to mid-gray, so the L1 distance to a mid-gray field is small by
  construction. Computed the same texture's real per-background-value L1 distance for a few
  candidates and picked the one that's genuinely worst (`255`, confidence `0.43`) instead of
  guessing — the test's own synthetic setup was wrong, not `match_confidence`'s math. `cargo check
  --workspace --all-targets`, `cargo clippy -p core --lib --tests` (via the documented temporary
  `filters.c`/`text_overlay.c` shim plus the `FFMPEG_DIR`/`lib`-symlink workaround, both reverted
  before commit), and `cargo fmt --all -- --check` all stayed clean.

  **Slice 3's storage question answered by reuse, not new code.** "Store generated matte data
  outside the main project JSON with versioned references and cache invalidation" turns out to
  already be satisfied by `background_removal`'s existing hidden-sibling-folder convention
  (`mask_dir_for`/`matte_path_for`, keyed by `clip_id`) — the missing piece was only a bridge from
  a `PropagatedMask` sequence into that same `encode_matte_video(frames: &[Vec<u8>], ...)` input
  shape. Added `rasterize_mask_to_matte`/`rasterize_to_matte_frames` to `mask_propagation`, reusing
  `crate::shape_render::point_in_polygon` (the same ray-casting test `overlay_render`'s own shape
  rasterizer already calls, just without that module's per-shape center/rotation transform — a
  mask polygon here is already in absolute frame-fraction coordinates) to rasterize each frame's
  translated polygon into a `255`-inside/`0`-outside grayscale-as-luma buffer. A caller can now
  pipe `propagate_mask_by_translation`/`propagate_mask_with_corrections`'s output straight into
  the existing `encode_matte_video` with no reshaping and no new storage format.

  Verified for real: 5 new tests added to the same scratch crate (33/33 total for this module) —
  inside/outside/corner correctness against a known square, a degenerate (<3-vertex) polygon
  producing an all-zero buffer rather than a meaningless ray-cast, zero-sized-canvas handling, and
  the per-mask batch shape. `cargo check --workspace --all-targets`, `cargo clippy -p core --lib
  --tests`, and `cargo fmt --all -- --check` all stayed clean via the same shim, reverted before
  commit.

  **Slice 4 (privacy blur as the first end-to-end effect) shipped, with an explicit unverified-
  at-runtime caveat.** Added `avbridge_apply_privacy_blur` (`csrc/privacy_blur.c`) — structurally
  identical to `avbridge_apply_text_overlays`/`_shape_overlays`'s existing "post-process pass over
  an already-rendered export" shape, but with two sources feeding one node instead of chaining N
  overlay segments onto one: the main video via the usual `buffer` source, and the matte video
  (from #136's `rasterize_to_matte_frames` + `encode_matte_video`) via FFmpeg's own `movie=`
  filter source — no second manual decode loop needed, the same trick `avbridge_apply_text_
  overlays` already uses to pull in its PNG overlays. The graph itself: `split=2` the main video,
  `gblur=sigma=<sigma>` one copy, then `maskedmerge` it back over the untouched copy using the
  matte's own luma as the blend weight — blurred wherever the matte is non-zero, sharp elsewhere.
  `format=yuv420p` on both the main branch and the matte source keeps every `maskedmerge` input in
  the same pixel format (its own documented requirement); `scale=W:H` on the matte guards against a
  matte built at a different resolution than the main video; `loop=0` repeats the matte's last
  frame if a caller supplies fewer matte frames than the main video's own frame count (a coarser
  sampling cadence, not necessarily every frame) rather than the `movie` source hitting EOF and
  stalling the graph early. Wired through the full stack: `avbridge::apply_privacy_blur` (Rust FFI
  wrapper, `PrivacyBlurError`) and `avcore::privacy_blur::apply_privacy_blur` (thin wrapper, same
  `MatteEncodeError`-style pattern `background_removal` already uses) — no `ui` wiring yet (no
  design pass done for the properties-panel/toolbar affordance that would trigger this on export;
  out of scope for this pass, which only had to prove the pipeline itself is reachable end to
  end from a propagated mask).

  **Explicit, deliberate gap: real runtime behavior of this filter graph is unverified.** This
  sandbox's packaged FFmpeg is too old to build the rest of `avbridge` at all (`CLAUDE.md`'s own
  documented gap), so there is no way here to actually run this function against a real video and
  confirm the blur/mask compositing produces a correct frame — verification stopped at `gcc
  -fsyntax-only` (per-file, against the real installed FFmpeg headers — confirms the C parses and
  every type/function reference resolves, catching real syntax/type errors) plus `cargo check
  --workspace --all-targets`/`cargo clippy --workspace --all-targets` (via the documented temporary
  `filters.c`/`text_overlay.c` shim, reverted before commit — confirms the Rust FFI declaration's
  signature matches the C function and the whole crate graph still type-checks) and `cargo fmt
  --all -- --check`. Treat the filter-graph string itself as code-reviewed, not execution-verified
  — the user explicitly chose this verification level (over a design-only note, or skipping the
  item) knowing this constraint. **Drive-by fix found while re-running clippy for this change**:
  #135's `propagate_mask_by_translation`/`propagate_mask_with_corrections` (8 and 9 parameters)
  were both past clippy's `too_many_arguments` default threshold and had no `#[allow(...)]` —
  apparently missed in that PR's own verification pass (a `cargo clippy` invocation difference,
  not a regression introduced since); fixed alongside this change with the same `#[allow(clippy::
  too_many_arguments)]` this codebase already uses on comparably-shaped functions elsewhere
  (`keyframe.rs`, `preview.rs`, `text_layout.rs`, `overlay_render.rs`, `render.rs`).

  **CF-09 shipped end to end, closing out the whole item.** The UI wiring the previous note
  designed (`architecture/competitive-feature-plan.md`'s CF-09 "UI integration design
  (2026-09-06)" section) is now implemented:
  - New `ClipInstance` fields (`privacy_blur_enabled`, `privacy_blur_mask_path`,
    `privacy_blur_sigma`, `privacy_blur_seed_vertices`, `privacy_blur_seed_center_x/y_frac`),
    reset on split (the seed was traced against the pre-split clip's own first frame, same
    staleness reasoning `background_removal_mask_path` already documents) and carried over on
    copy-paste (same range, still valid).
  - A properties-panel "Blur de privacidade" section: enable checkbox, blur-intensity slider,
    numeric center/width/height controls for the seed rectangle (no preview click-and-drag
    picker yet — a real, separate follow-up, same v1 scope cut the design note already flagged),
    and an "Aplicar blur" button.
  - `App::spawn_apply_privacy_blur_for_selected_clip` (new `ui/src/app/privacy_blur.rs`) —
    structurally identical to `spawn_generate_matte_for_selected_clip`'s background-thread
    pattern: samples frames at motion-tracking's own `4.0`/sec cadence (block-matching is cheap,
    unlike background-removal's ONNX-per-sample cost), seeds a rectangle from the properties
    panel's current region state, propagates it (`avcore::mask_propagation::
    propagate_mask_by_translation`), rasterizes and encodes it into a matte video cached next to
    the project (same directory background-removal's own matte already uses).
  - Export-side integration resolves the open design question exactly as planned: `avcore::
    render::resolve_privacy_blur_segments` snapshots every privacy-blur clip at queue time
    (`PrivacyBlurSegment`, threaded through `ExportJob`/`queue_export`/`PendingExportConflict`
    same as `text_segments`/`shape_segments`), and a new `apply_privacy_blur_pass` re-samples
    each clip's own clip-local matte via `avcore::FrameSampler` (the matte is a plain
    grayscale-as-luma H.264 file, no different from any other video this crate decodes), pads it
    to the export's own canvas duration (`crate::privacy_blur::
    pad_matte_frames_to_canvas_duration` — black frames outside the clip's own timeline window),
    re-encodes it, and calls `crate::privacy_blur::apply_privacy_blur` — chained after the
    text/shape overlay passes (so blur covers whatever text/shapes ended up inside the tracked
    region too), same "log-and-skip on error, never destroy an already-complete export" posture
    those two passes already take. Wired into both the single- and multi-track render paths, and
    into the nested-sequence pre-render pass.
  - **Drive-by fix, unrelated to this feature**: a "refactor(ui): isolate properties panel
    chrome" commit that landed on `main` mid-session broke the whole `ui` crate's build (6
    visibility errors — `pub(super)` items re-exported one module level higher than their own
    visibility allowed). Fixed by bumping `DIVIDER_HIT_WIDTH`/`stereo_db_meter`/`toolbar`/
    `sequence_tab_bar`/`save_active_project`/`export_srt_for_active_sequence` to `pub(crate)`,
    matching the existing `pub(crate) mod menu_bar` convention in the same file — confirmed via a
    stashed-changes check that this was already broken on `main` before this session's own
    changes, not something introduced here.
  - **Explicit, unchanged caveat**: `avbridge_apply_privacy_blur`'s own filter graph
    (`gblur`+`maskedmerge`) is still unverified at runtime in this sandbox — see the previous
    slice-4 note for the full reasoning. Everything added in this pass is either pure/
    deterministic (`pad_matte_frames_to_canvas_duration`, verified for real via a scratch crate,
    8/8 passing) or exercises only already-verified machinery (`FrameSampler`, `encode_matte_
    video`, `mask_propagation`'s own already-tested functions) — no new avbridge C code was
    added this round.

  Verified: `cargo check --workspace --all-targets`/`cargo clippy --workspace --all-targets`
  (via the documented temporary shim, reverted before commit) both clean; `pad_matte_frames_
  to_canvas_duration` scratch-crate tested for real (8/8); `cargo fmt --all -- --check` clean.
  Not verified: the actual UI interaction (no running build available in this sandbox — same
  "compiles and type-checks, not click-tested" honesty this session's other UI work already
  carries), and the export pass's real behavior against a real video (same FFmpeg-build
  limitation as the C filter graph itself).
- `[ ]` **CF-10: direct publishing.** Add a secure YouTube upload flow; keep OAuth credentials
  in the OS vault and separate from offline bundles.

Quick wins that may be completed alongside CF-01:

- `[x]` **Marker ruler rendering/snap.** Every marker now draws as a small color-coded (by
  `MarkerKind`) triangle at the top of the timeline ruler (`timeline_panel::draw::
  draw_marker_ticks`), click-to-seek like the Timeline Index panel's own rows, and is a
  magnetic-snap target for both the ruler's own playhead drag and every per-clip trim/move drag
  — closing the gap P0 item 2's own doc comment flagged ("Timeline markers aren't a snap
  target — no markers feature exists yet... revisit when it lands"), now that P2 item 9 shipped
  markers. Pure UI wiring reusing already-tested `snap_to_nearest`/`snap_move_start` — no new
  pure-function surface needed a unit test of its own.
- `[~]` Stabilization and deflicker preview parity — deflicker done (P4 item 21), stabilization
  still open (motion estimation, a materially different problem).
- `[~]` Real-hardware GPU encoder validation — NVENC confirmed on a real RTX 4070 (P4 item 19),
  Quick Sync/AMF/VAAPI still unverified (no such hardware on any dev machine checked so far).
- `[x]` **Integrating the standalone watched-folder utility into the app.** New "Limpeza"
  screen (nav rail, 🧹) replaces `scripts/Watch-Gameplay.ps1` + its `ui.html` status page with a
  native egui equivalent. `avcore::watched_folder` (new `core` module) detects a video file in a
  chosen folder, waits for its size to stay stable for a configurable window (same heuristic the
  script uses), then normalizes it via `render::render_export` — video passthrough-copied, audio
  through `afftdn` noise reduction + `loudnorm` + a true-peak `alimiter`, re-encoded AAC — a
  primitive that already existed in `core` but had **zero UI callers before this feature**.
  Before/after LUFS shown per file, same as the script's own before/after panel. Background
  polling thread + `mpsc` channel + per-frame `pump_watch_folder()`, the same shape every other
  background feature in this app already uses.

  **Documented gaps, not silently dropped** (see `watched_folder.rs`'s own module doc comment):
  the script's `highpass=f=80` rumble cut and `acompressor` voice-leveling steps aren't in
  `render_export`'s shared filter chain — adding them would change every export caller's audio,
  not just this feature, so that's left for a deliberate follow-up rather than a side effect of
  this one; and the script's exclusive-file-lock stability check (`Test-FileReady`) isn't
  ported — it needs a platform-specific dependency this crate doesn't have, so only the
  size-stable-for-N-seconds heuristic is implemented, a real (if slightly weaker) signal, not a
  fake one.

  Verified for real: this dev machine's full FFmpeg/GStreamer toolchain actually links `core`'s
  own test binary (confirmed across this whole session, unlike the sandboxed environment most
  prior ROADMAP entries describe) — `cargo test -p core --lib watched_folder` runs all 8 tests
  (extension filtering, output-path construction, stability-tracker state machine across
  multiple files) for real. `ui`: new `App`-level tests for start/stop no-ops and
  `pump_watch_folder`'s event-to-row-state mapping (detect/progress/done/failed). **Not run
  against a live GUI session or real gameplay footage** — the actual background thread's
  filesystem polling and its call into `process_watched_file` haven't been exercised end to end,
  same caveat every other UI-only or GStreamer-adjacent change in this environment already
  carries.

## P6 — Explicitly Deferred

Real ideas, deliberately not queued — revisit only if a P3 differentiator proves the audience
wants more in this direction. See `architecture/differentiators.md`'s "out of scope" section
and `matrix/competitor-parity.md`'s "deliberately not adopted" section.

23. `[ ]` Voice-clone TTS beyond the single bundled Piper voice — still deferred until CF-01 and
    CF-03 ship, and must require explicit consent plus deletion of derived voice artifacts.
24. `[ ]` Content-hash render cache (FilmCraft-style render bar: per-segment hashed ProRes
    previews that survive edits and undo). Considered on 2026-10-11 and deferred, because the
    workloads it would help are already covered or already measured: 4K sequential playback is
    covered by editing proxies (`avcore::proxy`), and the only recorded case that cannot run in
    real time is 4K with 6 layers and effects (594 ms CPU, 65 ms GPU in
    `architecture/bush-performance-plan.md`), which the existing Criterion suites
    (`crates/bush/benches/editor_workloads.rs`, `engine_workloads.rs`) already track. Revisit
    when playback telemetry shows deadline misses on proxy-backed footage with typical gameplay
    edits; the design would hash resolved segments, write previews beside the project, and swap
    them in as a Bush source.
Motion-graphics templates and real-time AI object masking moved from this section to CF-07 and
CF-09 respectively. The competitive refresh found concrete gameplay/channel use cases for both,
but they remain behind the higher-impact CF-01-CF-06 workflow items.

## P7 — Design System Consolidation

`ui` crate presentation-layer only. Full findings: `docs/design/UI_DESIGN_AUDIT.md` (whole-app audit) and
`docs/design/DESIGN_SYSTEM_CONSOLIDATION.md` (architectural review — source-of-truth table, bypass
enforcement analysis, component inventory). Verdict: `theme.rs`/`components/*.rs` infrastructure
is sound, the problem is unenforced bypass at ~half the relevant call sites, not a missing
system. Implementation order below matches the consolidation review's recommendation
(zero-ambiguity items first, decisions-needing-confirmation items later — both flagged decisions
resolved in favor of consolidating, see items 3 and 5).

25. `[x]` **Stage 1 — zero-risk refactors.** `library.rs`'s hand-rolled card `Frame` →
    `components::card_frame()`. Nav rail's flagged "two icon conventions" inspected: the profile
    avatar is a static identity badge (no click handler), not an interactive icon button like
    `rail_button()`/`prefs_button()` (both already correctly state-conditional) — a different
    semantic role, not a real inconsistency. Documented as a resolved non-issue, no code change.
26. `[x]` **Stage 2 — title tier components.** Added `components::page_title()` (20.0 strong) and
    `components::modal_title()` (15.0 strong); migrated Home's 22.0 outlier and all ~17 modal
    `.size(15.0).strong()` sites (`app/modals.rs`, `app/transcript_panel.rs`) onto them. The
    About dialog's 28.0 ACCENT-colored app-name wordmark was deliberately kept as-is (commented
    as a brand-moment exception, not a dialog header) rather than folded into `modal_title()` —
    a considered deviation from this item's original wording, made because forcing it down would
    have been a real visual regression for a normal About-box convention. Breadcrumb's three
    13.0 segments left unchanged (a single cohesive breadcrumb trail already internally
    consistent within one file — a shared component for a single-use 3-segment trail would be an
    abstraction layer without enforcement benefit).
27. `[x]` **Stage 3 — spacing/radius token rollout.** Added `SPACE_XS/SM/MD/LG` (4/8/12/20) and
    `RADIUS_SM/MD/PILL` (4/8/999) to `theme.rs`. `card_frame()`'s radius 10→8. All confirmed
    hardcoded-radius sites (10 in `timeline_panel/mod.rs`'s `CornerRadius::same(4)` clip-drawing
    calls, `home.rs`/`library.rs`'s nested thumbnails, the in-editor media-library row, the
    drag-ghost tooltip, `sound_library.rs`'s track rows, the toast frame, `nav_rail.rs`'s icon
    backgrounds, `tag.rs`/`breadcrumb.rs`'s pill radius) now reference the named constants.
28. `[x]` **Stage 4 — property-row consolidation.** Added `components::property_row()` (label +
    widget, no separator/note). Migrated all 22 of `shape_clip.rs`'s/`text_clip.rs`'s hand-rolled
    label+widget pairs onto it; their keyframe blocks are unchanged (still `property_section`).
29. `[~]` **Stage 5 — `icon_button()` and icon-convention unification.** Added
    `components::icon_button(ui, glyph, tooltip, opts: IconButtonOpts)` with the two escape
    hatches (size override, hover-color override) from the design review. First migration batch
    landed: all 4 confirmed bare `small_button("🗑")` delete-action sites (3 in
    `keyframe_editors.rs`, 1 in `modals.rs`'s marker list), each gaining a real tooltip via two
    new `i18n.rs` keys (`RemoveKeyframe`/`RemoveVertex`/`RemoveMarker` — previously icon-only
    with no accessible name at all). Second batch: the timeline track-visibility toggle's
    ambiguous "👁"→"—" hidden-state fallback fixed, routed through `icon_button()`, and its
    tooltip — previously hardcoded English, never localized — moved to real `Text::TrackHide`/
    `TrackShow` i18n keys. **Resolved**: a later session's systematic tofu-glyph sweep (real
    `cargo build`/`cargo test` runs against a real compiled `ui.exe`, not just visual inspection)
    replaced the originally-planned "⊘" with the vendored `icons::EYE_STR`/`EYE_OFF_STR` pair
    instead — audio tracks now reuse the same eye/eye-off icon video tracks already used, rather
    than a distinct glyph per track kind, closing this note's open rendering-verification
    question by removing the unverified glyph entirely. Third
    batch: transport's seek-to-start/seek-to-end and play/pause buttons were icon-only with no
    tooltip at all (in both the normal and fullscreen-overlay preview) — fixed via two new
    `Text::SeekToStart`/`SeekToEnd` keys plus reusing `Text::ShortcutPlayPause`. The non-
    fullscreen skip buttons route through `icon_button()`; the fullscreen overlay's variants keep
    their existing `small_button`/`button` calls with an added `.on_hover_text` rather than
    migrating, since they fade with a runtime opacity value `icon_button()`'s two deliberately-
    narrow escape hatches don't cover — forcing that in would grow the component's API for one
    caller. A systematic sweep confirmed **zero remaining bare `small_button("<glyph>")` calls
    anywhere in the crate** — every icon-only `small_button` now has a tooltip.

    **Remaining, not yet migrated** — the items below don't have a confirmed missing-tooltip
    defect the way the three batches above did; they're cosmetic-convention inconsistencies that
    need a visual pass (not available this session) to resolve well, or need `icon_button()`'s
    API to genuinely grow (which the design review cautioned against doing casually):
    - Toolbar's inline `format!("{glyph} {label}")` buttons — already have visible labels, not an
      accessibility gap, just a different (acceptable) convention from icon-only buttons.
    - `breadcrumb.rs`'s `window_button()` — full-rect background-fill-on-hover (OS window-chrome
      convention) that `icon_button()`'s stroke-only hover-color hatch doesn't reproduce; already
      has an accessible name via `widget_info` (screen readers/UI-Automation), and OS-native
      window controls conventionally have no visible hover tooltip either — not a confirmed
      defect, left as-is rather than forced through a mismatched component.
    - Timeline badges: the freeze badge (`draw.rs`) is a non-interactive painted overlay, not a
      button — doesn't apply. The audio-role glyphs (🎮/🎤/🎵) are `ComboBox` content, already a
      well-formed component with its own accessible label — not a bypass.
    - Nav rail: already resolved as a non-issue (Stage 1).

    Re-checked the `✂`/`✂️` duplicate-glyph note: no longer accurate as written. The toolbar's
    Cut tool itself was migrated to the vendored icon font (`icons::SCISSORS_STR`, via
    `icon_label_job`) as part of the Stage 2.5 icon-font work, so it no longer renders either
    emoji at all. The one remaining `✂️` is `Text::DetectSpeechEdits`'s label — an emoji prefix
    consistent with every other AI-detection cluster button in the same toolbar group
    (`DetectSilence`'s 🔇, `DetectChapters`'s 🎬, `DetectHighlights`'s ⭐, `ShortsPack`'s 🎞), all
    following the accepted "visible label, emoji prefix" convention this stage's own note above
    already carves out as different from (not a defect relative to) icon-only buttons. Changing
    just this one emoji to avoid a passing resemblance to the Cut tool's icon would break that
    convention for one button out of five; routing all five through vendored icons instead would
    need five new icons fetched for a purely cosmetic concern. Not a confirmed defect — closing
    this note without a change.
30. `[x]` **Feedback tokens.** Added `WARNING`/`WARNING_TINT`/`INFO_TINT` to `theme.rs` and
    `components::tag_warning()`. Queue's `ACCENT.gamma_multiply(0.10)` info banner → `INFO_TINT`;
    its "Paused" pill (previously `tag_outline`, indistinguishable from "Queued") → `tag_warning`.
31. `[x]` **Progressive disclosure.** Toolbar grouping done: the trailing cluster (panel-visibility
    toggles, AI-detection actions, multicam grouping) previously ran together with zero
    separation — now split into three `ui.separator()`-delimited groups: [timeline-index/
    transcript panel toggles] | [detect silence/speech-edits/chapters/export-chapters/highlights,
    shorts pack] | [create multicam group] (structural, not an automatic detection, so kept
    distinct from the AI-detection cluster). The rest of the toolbar already had separator
    discipline (edit tools / composite+template actions / add-track actions / undo-redo), so this
    closes the one confirmed gap rather than restructuring the whole toolbar.

    Prefs sectioning: all 6 sections (Language, Audio, Export, Project, Shortcuts, About) were
    plain muted labels followed by always-fully-expanded content in one long scroll column — the
    clearest violation of "not all functionality simultaneously" in the whole audit, given Export
    alone bundles 10 sub-controls (workers, GPU encoder, preview quality, hardware decode, output
    folder, and 4 separate AI-model-path rows). Wrapped each in a new `prefs_section()` helper
    (`egui::CollapsingHeader` styled to match the existing muted/uppercase/strong header look),
    `default_open` chosen per section by how often it's touched after initial setup rather than
    uniformly: Language/Audio/Project default **open** (small, or commonly adjusted); Export/
    Shortcuts/About default **closed** (large one-time setup, a reference table, and rarely-needed
    metadata respectively). Collapse state persists via egui's own per-`Id` memory, so it survives
    switching screens and coming back, though not a full app restart. This is the first
    unavoidable code change in this whole consolidation effort with no way to visually verify the
    result — this session has no way to run the desktop app. Flagging explicitly, per repo
    `CLAUDE.md`'s own guidance that UI changes should be checked in a running instance before
    being called done: this is implemented and compiles/tests clean, but **not yet visually
    confirmed**.
32. `[x]` **Missing states.** Queue had no empty-state message at all — added one
    (`Text::QueueEmpty`), plain muted text matching the existing empty-state convention elsewhere
    (Library, Sound Library) rather than waiting on the still-in-progress icon system.

## Mixer, HDR and color (2026-10-12)

Plan, findings and acceptance criteria: [architecture/mixer-hdr-color.md](architecture/mixer-hdr-color.md).

33. `[x]` **CM-00 SDR decode uses the wrong matrix** (fixed, see the status table). Measured: BT.709 footage shows as
    186, 46, 41 instead of 200, 60, 40 in the preview (BT.601 matrix). First slice, small fix.
34. `[~]` CM-01 colour metadata, CM-02 HDR to SDR, CM-03 output tagging and CM-06 scopes are done;
    CM-04 10-bit/HDR output and CM-05 grading tools are not.
35. `[~]` MX-01 multi-stream audio, MX-02 per-track fader and pan, MX-03 per-track EQ, filters and
    compressor, MX-04 declick at cuts and audio-only export, and MX-00 preview ducking and
    voice cleanup matching the export are done (a gate, strip presets, per-clip strips and an
    adjustable declick length are also left).

---

[← back to spec/INDEX.md](INDEX.md)
