# UI Implementation Plan — oca

Analysis-only document. Nothing in this plan has been implemented. Grounded in direct reading of
`crates/ui/src/{app,screens,theme.rs,i18n.rs,components}` and `crates/core/src/{project,timeline,
media,keyframe,undo,nested_sequence}.rs` — every claim below is either a confirmed fact (with a
file reference) or explicitly marked as a proposal/judgment call.

## 1. Current State Summary

**Architecture.** `eframe::App for App` (`crates/ui/src/app/mod.rs`) is a thin per-frame
dispatcher: drain ~12 background-job queues (`pump_*`), then `nav_rail` → `breadcrumb` →
`match self.screen { Home | Editor | Library | SoundLibrary | Queue }` inside a
`CentralPanel`, then ~18 modal checks, then toasts. `app/` (~30 files) holds state + mutation
logic with no drawing; `screens/` holds drawing and delegates to `app/`. The timeline
(`screens/editor/timeline_panel/`) is custom-painted against `ui.painter()`, not built from
nested widgets — genuinely a specialized canvas, not a generic list. See `.claude/engineering/
ARCHITECTURE.md` and `.claude/domain/TIMELINE.md` for the full detail this plan builds on.

**Strengths.**
- Timeline drag/trim already separates transient preview state (`ClipDrag`, `trim_requests`)
  from committed mutation (applied once, post-draw-loop, with one `push_undo_snapshot()`).
- Consistent value-equality caching (`ExportPreviewCache`, `NestedSequenceCache`) and a bounded,
  evicting thumbnail texture cache.
- A real, centralized theme (`theme.rs`) and i18n catalog (`i18n.rs`) — no hardcoded colors or
  UI strings scattered through screens.
- `App`'s prior "122 flat fields" problem was already identified and fixed with sub-structs —
  the team has already self-corrected once on exactly the kind of growth this plan must avoid
  reintroducing.

**Limitations (confirmed, not assumed).**
- No track/clip-level viewport culling in the timeline — every clip painted every frame
  regardless of visibility (`.claude/engineering/TIMELINE_PERFORMANCE.md`).
- No marquee/rubber-band multi-select.
- Keyframes animate export only, not the live GStreamer preview — a real preview/export parity
  gap, not a bug.
- `App` at 2100+ lines in `mod.rs` alone; adding state carelessly regrows the earlier problem.

**Technical constraints.** Two independent rendering paths (GStreamer preview vs. FFmpeg/avfilter
export) that must be reasoned about separately — a UI change that assumes preview accurately
represents export output (or vice versa) will be wrong for any export-only effect. `tokio` has
only the `sync` feature — no async task runtime; all background work is OS threads + mpsc +
per-frame `pump_*`.

## 2. Product UX Vision

**Primary user**: someone cutting their own gameplay footage for a YouTube channel (per
repo-root `CLAUDE.md` — PacoPaçoca) in long solo sessions: import raw recordings, cut/trim,
normalize loudness, export at source bitrate. Not a multi-editor collaborative shop (though a
`collab_bundle.rs`/`.zip` export exists) and not primarily a motion-graphics tool — the AI/
analysis features (transcript, silence detection, scene detection, background removal) exist to
speed up exactly this cut-gameplay-footage workflow, not as a generic creative suite.

**Most frequent actions**: scrub/play, select clip, trim/split, adjust one or two properties
(crop/gain/speed), export. These deserve permanent, low-travel-distance screen space.
**Contextual**: properties panel content (already context-aware by clip type — keep this),
keyframe editors, per-effect controls.
**Hidden until needed**: the batch-review modals (silence review, transcript-proposal review),
export queue detail, collab bundle export, multicam grouping UI — these are real but
lower-frequency; the existing modal/queue-screen treatment for them is already correct, not a gap
to fix.

**Personality**: Professional, fast, focused, precise, calm, powerful. The existing dark/teal
`theme.rs` palette and restrained 8px-radius widget styling already lean this direction — see
Phase 3 audit below for where it doesn't fully land it yet.

**Avoid**: generic SaaS/dashboard framing, decorative gradients, oversized empty space,
icon-only controls without a label (the toolbar's existing emoji+label convention already avoids
this — extend it, don't regress it).

## 3. Workspace Architecture

Current real layout (`screens/editor/mod.rs`), described accurately rather than idealized:

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ breadcrumb.rs — window drag region / project & sequence breadcrumb        │
├───────┬─────────────────────────────────────────────────────────────────┤
│ nav_  │ toolbar (screens/editor/mod.rs) — tool select, split, silence/    │
│ rail  │ speech-edit detection, chapter detection, etc.                    │
│ .rs   ├───────────────┬─────────────────────────┬───────────────────────┤
│ (left │ media library  │        preview          │   properties panel    │
│ nav)  │  column        │  (screens/editor/        │  (properties_panel/,  │
│       │                │   preview + transport)   │   context-aware by    │
│       │                │                          │   clip type)          │
│       ├───────────────┴─────────────────────────┴───────────────────────┤
│       │        timeline_panel/ — ruler, track headers, clips              │
└───────┴─────────────────────────────────────────────────────────────────┘
```

Per-region assessment (purpose / priority / resizable / collapsible / contextual):
- **nav_rail** — screen switcher (Home/Editor/Library/SoundLibrary/Queue). Priority: low,
  persistent chrome. Not resizable/collapsible today — **proposal**: keep fixed-width, it's
  already thin; not worth the complexity of making it collapsible for a 5-item rail.
- **Media library column** — priority: medium, permanent during editing. Currently
  fixed/semi-fixed width — **proposal (Phase 1)**: make it user-resizable via a splitter
  (`components/` has no splitter today — new component, see §7), since library items compete
  for space with preview on smaller displays.
- **Preview** — priority: highest during editing. Should get first claim on available width;
  confirm (Phase 3 audit) it isn't being squeezed by fixed-width side columns today.
- **Properties panel** — priority: high when something is selected, near-zero otherwise.
  Already context-aware. **proposal**: collapse to a slim "nothing selected" state rather than a
  full-width empty panel, if not already doing so (verify in Phase 3 audit / needs a screenshot
  pass, not confirmed from code alone).
- **Timeline** — priority: highest, permanent, full-width. Already full-width; the real work
  here is performance (viewport culling, §5), not layout.
- **Toolbar** — priority: high, permanent during editing. Already grows as features are added
  (split, silence detection, speech-edit detection, chapter detection, ...) — **risk**: an
  ever-growing flat toolbar is the toolbar equivalent of the "122 flat fields" problem. See §8
  Phase 2 for a proposed grouping/overflow strategy before it becomes unreadable.

## 4. Component Architecture

Using real module names, not invented ones:

```text
App (crates/ui/src/app/mod.rs) — state + per-feature impl blocks, ~30 files
├── screens/
│   ├── breadcrumb.rs        — top chrome
│   ├── nav_rail.rs          — left nav
│   ├── home.rs, library.rs, sound_library.rs, prefs.rs, queue.rs
│   └── editor/
│       ├── mod.rs           — toolbar, 3-column layout, keyboard shortcuts, dispatch
│       ├── properties_panel/
│       │   ├── mod.rs, keyframe_editors.rs, shape_clip.rs, text_clip.rs
│       └── timeline_panel/
│           ├── mod.rs       — ruler/tracks/clips layout + interaction + dispatch
│           ├── draw.rs      — pure paint functions (filmstrip, waveform, markers, playhead)
│           └── snap.rs      — ClipDrag, snap target math
└── components/               — frame.rs, section.rs, property.rs, combo.rs, tag.rs (plain fns)
```

For each, responsibility/state-ownership/render-sensitivity is already established by the
existing convention (state+logic in `app/`, drawing in `screens/`, reusable non-timeline pieces
as plain functions in `components/`, timeline-specific painting in `timeline_panel/draw.rs`).
**This plan does not propose changing this split** — it's sound and should be extended, not
replaced.

## 5. Timeline Plan

State categories, using the real distinction already present in the codebase:

- **DOMAIN STATE** (`avcore`, persisted): `Timeline.tracks/playhead_secs/markers`,
  `Track.clips/text_clips/shape_clips`, `ClipInstance` fields.
- **EDITOR STATE** (`App`, not persisted): `selected_clip_id`, `multi_selected_clip_ids`,
  `timeline_px_per_sec`, `tool`.
- **TRANSIENT INTERACTION STATE** (frame/gesture-scoped): `ClipDrag`, `trim_requests`,
  `multi_select_requests`, `split_at_playhead_requested`.
- **RENDER CACHE**: `App.thumbnail_state.thumbnail_textures`, `ExportPreviewCache`,
  `NestedSequenceCache`.

**Proposed work, in priority order:**

1. **Viewport culling (High priority, Medium risk).** Before the per-track/per-clip draw loops
   in `timeline_panel/mod.rs`, compute a visible time range from the enclosing scroll offset +
   `rect.width()`/`timeline_px_per_sec`, and skip clips whose `[start_secs, start_secs+duration)`
   doesn't intersect it (binary-search or a cheap linear skip, since clips are usually
   start-time-ordered per track — verify that invariant before assuming it). Do the same for
   vertical track visibility if track count can be large. This directly addresses the one
   confirmed real performance gap; nothing else in this plan is as load-bearing.
2. **Marquee/rubber-band multi-select (Medium priority, Medium risk).** Add a drag-select
   rectangle on empty timeline background (not on a clip body), collecting intersecting clip ids
   into `multi_select_requests` the same way ctrl-click already does — reuses the existing
   request-buffer-then-apply shape, doesn't need a new commit mechanism.
3. **Zoom-dependent LOD (Low priority, unconfirmed need).** Only pursue if profiling after (1)
   still shows cost at low zoom on a large project — don't add LOD complexity speculatively.

Caching: any new per-clip derived visual (e.g. a simplified LOD shape) should follow the
established value-equality convention, not a version counter.

## 6. Design System Plan

Semantic tokens already exist in `theme.rs` (§ design/DESIGN_SYSTEM.md has the full table) —
this plan does not propose new tokens speculatively. One concrete proposal: add a
`SPACING_SCALE`-style set of named spacing constants (currently only two ad-hoc values,
`item_spacing`/`button_padding`, are centralized; per-screen `ui.add_space(N)` calls use bare
literals like `4.0`/`6.0`/`8.0` scattered throughout — e.g. seen repeatedly in
`app/modals.rs`/`transcript_panel.rs`). Consolidating these into 3-4 named constants
(`SPACE_XS/SM/MD/LG`) in `theme.rs` would reduce inconsistency risk as more panels are added —
**Low priority, Low risk**, mechanical.

## 7. Custom Egui Components

Real gaps identified against actual usage patterns (not a generic wishlist):

| Component | Purpose | Notes |
|---|---|---|
| `Splitter` | Resizable divider between media library / preview / properties columns | Does not exist today (§3's resizable-column proposal needs it) — new. |
| `ToolbarButton` (grouping) | Group the growing toolbar's buttons (split, silence, speech-edit, chapter, ...) with visual separation and possibly an overflow menu | Toolbar buttons currently appear to be added flat/sequentially — verify visually in a screenshot pass before committing to a specific grouping scheme. |
| `ReviewModalList` | The checkbox-per-item list pattern already duplicated between `show_silence_review_modal` and `show_transcript_proposals_modal` | Genuine duplication — worth extracting once a third consumer appears; not urgent with only two. |

Existing reusable pieces (`components/frame.rs`, `section.rs`, `property.rs`, `combo.rs`,
`tag.rs`) already cover generic grouping/property-row/tag needs — do not duplicate them.

## 8. Implementation Order

### Phase 1 — Foundation
- Spacing token consolidation (§6).
- `Splitter` component + make the media-library/properties columns resizable (§3, §7).

### Phase 2 — Toolbar scaling
- Group toolbar actions (edit tools vs. AI-detection tools vs. view tools) with visual
  separation; consider an overflow affordance once a screenshot pass confirms it's needed.

### Phase 3 — Timeline performance
- Viewport culling (§5.1) — the single highest-value item in this whole plan.

### Phase 4 — Advanced interaction
- Marquee select (§5.2).

### Phase 5 — Polish
- Empty-state treatment for the properties panel when nothing is selected (needs a screenshot
  pass to confirm current behavior first — see §10 risk notes).

### Phase 6 — Performance validation
- Profile timeline frame time before/after Phase 3 on a project with a large clip count; confirm
  the culling change actually reduces frame time (don't assume it does without measuring).

This order deliberately puts the confirmed real performance gap (Phase 3) ahead of the
speculative visual polish items (Phase 1-2, 5) in terms of *value*, but Phase 1's `Splitter` is
listed first because §5's culling work and §7's `Splitter` are independent and Phase 1 is lower
risk to land first as a warm-up; reorder if the team prefers value-order over risk-order.

## 9. File-Level Implementation Plan

**Phase 1 — Splitter + resizable columns**
```text
crates/ui/src/components/splitter.rs (new)
- pub fn splitter(ui: &mut egui::Ui, id: egui::Id, ratio: &mut f32) -> egui::Response
  Draggable divider, updates *ratio in place; caller reads it back for column width.

crates/ui/src/screens/editor/mod.rs
- Replace fixed-width media-library/properties column allocation with a persisted ratio
  (new field on PrefsState or App, following the existing panel_layout precedent on Project
  if it should persist per-project, or PrefsState if it's a global editor preference — decide
  based on whether panel_layout: Option<PanelLayout> on Project already covers this, since it
  may already exist for exactly this purpose; check before adding a new field).
```

**Phase 2 — Toolbar grouping**
```text
crates/ui/src/screens/editor/mod.rs
- Split the flat toolbar() function's button sequence into grouped sub-sections with
  ui.separator() between groups (edit tools / AI-detection tools / view tools), no new
  abstraction needed unless a screenshot pass shows overflow is a real problem.
```

**Phase 3 — Viewport culling**
```text
crates/ui/src/screens/editor/timeline_panel/mod.rs
- Add a visible_time_range(scroll_rect, timeline_px_per_sec) -> (f64, f64) helper near the top
  of the track/clip draw loop (around the existing loop at the line documented in
  domain/TIMELINE.md's "iterates every track's every clip" finding).
- Guard the per-clip painter calls (video clips, text_clips, shape_clips) with a visibility
  check against that range before the existing rect_filled/text calls.

crates/ui/src/screens/editor/timeline_panel/timeline_panel_test.rs
- Add a test asserting clips fully outside the visible range are skipped (construct a Timeline
  with clips far outside a small viewport, assert the culled draw-call count / returned
  visible-clip list, whichever the implementation exposes as testable).
```

**Phase 4 — Marquee select**
```text
crates/ui/src/screens/editor/timeline_panel/mod.rs
- Detect a drag starting on empty track background (not on a clip's Sense::click_and_drag()
  region) and draw a selection rectangle; on release, compute intersecting clips and push their
  ids into the existing multi_select_requests buffer.
```

## 10. Risk Assessment

**High risk**
- Viewport culling touching the timeline's core draw loop — could visually break clip rendering
  or interaction hit-testing if the visible-range check is applied inconsistently between the
  paint call and the `Sense`/hit-test region for the same clip. **Mitigation**: cull only the
  paint calls first, verify hit-testing still works for partially-visible clips (don't cull
  interaction regions in the same pass), add the test in §9 Phase 3, and manually verify
  drag/trim/select still work at the edges of the viewport before considering it done.
- Persisted-state changes for resizable columns, if using `Project.panel_layout` — a schema
  change to an already-persisted field needs `#[serde(default)]` and a round-trip test (per
  `.claude/domain/VIDEO_EDITOR.md`'s persistence section), or it risks breaking existing saved
  projects.

**Medium risk**
- Toolbar grouping — purely visual/layout, but touches a file (`screens/editor/mod.rs`) with
  inline keyboard-shortcut handling right above the toolbar; a careless refactor could
  accidentally disturb shortcut wiring. **Mitigation**: touch only the toolbar-drawing section,
  leave the shortcut-handling block untouched.
- Marquee select — new interaction on the same draw loop as existing drag/trim; risk of
  conflicting `Sense` regions (a marquee drag starting where a clip's own drag region ends).
  **Mitigation**: only arm marquee-select when the drag start point doesn't hit any existing
  clip/track-header interactive region.

**Low risk**
- Spacing token consolidation — mechanical, no behavior change, easy to verify by visual diff.
- `Splitter` component itself (the widget, not its wiring into `screens/editor/mod.rs`'s layout)
  — new, isolated code with no existing callers to break.

## 11. Acceptance Criteria

**Workspace**
- Media library and properties columns resize via the new `Splitter` without layout jitter.
- The ratio persists across app restart (survives a save/reload cycle without corrupting
  `.ocproj`, if stored on `Project`; or across `PrefsState` reload if stored there).
- Minimum column widths are respected (preview never gets crushed to unusable width).

**Timeline**
- With culling: a project with clips scattered across a timeline 10x wider than the viewport
  renders in bounded time regardless of total clip count (measured, not assumed — Phase 6).
- Drag, trim, split, and select still work correctly for clips at the visible-range boundary
  (partially on-screen).
- Marquee select: dragging from empty timeline background selects exactly the clips whose
  bounding rect intersects the marquee rect; a drag starting on a clip still performs a normal
  clip drag, not a marquee.

**UI**
- No new hardcoded color or UI string is introduced outside `theme.rs`/`i18n.rs`.
- Toolbar remains scannable (grouped, not one undifferentiated row) after grouping lands.
- No new per-frame full-`Project`/`Timeline`/`Vec<ClipInstance>` clone is introduced without an
  explicit justification recorded in the commit message, per `.claude/CLAUDE.md`'s "Rust
  principles."

---

## Final Summary

**A. Repository architecture discovered**: 4-crate workspace (`avbridge` → `core`/`avcore` →
`ui`, plus isolated `ytbridge`), strict one-way dependency. `ui` = eframe/glow + `App` (thin
dispatcher) + `app/` (state/logic, ~30 files) + `screens/` (drawing) + `components/` (reusable
plain-fn widgets). Timeline is custom-painted, not widget-composed. Undo is full-`Sequence`-
snapshot, not commands. Async work is `std::thread::spawn` + `tokio::sync::mpsc` + per-frame
`pump_*`, not tokio tasks. Caching is value-equality (`PartialEq`), not version counters.
Compound clips are `ClipInstance.nested_sequence_id`, not a separate type. Full detail in
`.claude/domain/{VIDEO_EDITOR,TIMELINE}.md` and `.claude/engineering/{ARCHITECTURE,EGUI,
TIMELINE_PERFORMANCE,QUALITY}.md`.

**B. Changes made to `.claude/`**: Rewrote `CLAUDE.md`, `domain/VIDEO_EDITOR.md`,
`domain/TIMELINE.md`, `engineering/ARCHITECTURE.md`, `engineering/EGUI.md`,
`engineering/TIMELINE_PERFORMANCE.md`, `design/DESIGN_SYSTEM.md` from generic/hypothetical
content to ground-truth content with real file/struct/function references. Added
`engineering/QUALITY.md` (didn't exist; explicitly listed as required). Lightly amended
`ux/WORKFLOWS.md` (real module references, added the "reviewable batch suggestion" modal
exception). Left `design/PROFESSIONAL_UI.md`, `ux/UX_PRINCIPLES.md`, `README.md`, and
`agents/*.md`/`commands/*.md` unchanged — they're generic principles/agent personas that don't
make any claim about this repo's actual architecture and don't conflict with what was found;
rewriting them would be documentation for its own sake.

**C. Current UI problems identified** (code-level audit; no live screenshot pass was done in this
session — see risk note below): (1) confirmed timeline viewport-culling gap; (2) toolbar is
flat and will keep growing as features are added, same shape as the already-once-fixed "122 flat
fields" `App` problem; (3) no resizable columns for media library/properties, which may pinch
preview width on smaller displays (needs visual confirmation); (4) keyframes animate export but
not live preview — a real UX-visible parity gap when a user keyframes an effect and doesn't see
it move during preview.

**D. Proposed UI architecture**: No structural change to the existing `app/`+`screens/`+
`components/` split — it's sound. Add one new reusable component (`Splitter`), extend the
existing timeline draw-loop pattern with a visibility check, extend the existing
request-buffer-then-apply interaction pattern with marquee select.

**E. Implementation phases**: Foundation (splitter/spacing tokens) → Toolbar scaling → Timeline
performance (culling) → Advanced interaction (marquee) → Polish (empty states) → Performance
validation (profiling). See §8 for the full ordering rationale.

**F. Highest technical risks**: viewport culling accidentally breaking hit-testing for
partially-visible clips; a persisted-layout schema change breaking old `.ocproj` files without
`#[serde(default)]`.

**G. Recommended first implementation step**: Phase 3 (viewport culling) has the highest
confirmed value (it's the one performance gap actually found, not speculated), but Phase 1's
`Splitter` is lower-risk and independent — recommend starting with the culling fix specifically
because it's the only item in this plan backed by a *confirmed* rather than *judgment-call* gap,
and land it behind the mitigation in §10 (cull paint only, not hit-testing, verify boundary
interaction manually) before touching anything layout-related.

**Limitation to flag explicitly**: Phase 3 (UI audit) of the requested task was done via code
reading, not a live screenshot/running-app pass — repo-root `CLAUDE.md`'s own guidance says UI
changes should be visually verified in a running instance, and this document's "needs a
screenshot pass to confirm" notes (properties-panel empty state, toolbar overflow, column
pinching) are exactly the places that guidance applies. Recommend an actual `make run` +
visual pass before finalizing Phase 1-2/5 scope, even though nothing here has been implemented
yet.
