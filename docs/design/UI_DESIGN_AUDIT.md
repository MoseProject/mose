# UI/UX and Visual Design Audit — oca

Analysis only. Nothing in this document has been implemented. Every claim is grounded in direct
reading of `crates/ui/src/{app,screens,components,theme.rs,i18n.rs}` — file:line references are
real, not illustrative. This audit covers the **whole application** (Home, Editor, Library, Sound
Library, Prefs, Queue, all modals, all context menus), not the timeline fragment discussed
earlier — that fragment turned out to be a symptom of the same systemic gaps found everywhere
else, confirming the brief's premise.

## Verdict

The application does **not** currently have a coherent, enforced design system. It has the
*ingredients* of one — `theme.rs` tokens, `components/{frame,section,property,combo,tag}.rs`
shared primitives, a real i18n catalog — but every screen and most modals bypass those
ingredients at least partially and re-declare their own literals (font sizes, spacing, corner
radii, icon styling). The result reads exactly like the brief describes: "a collection of
independent widgets" wearing one color palette, not one designed product. The icons look generic
*because* they're one visible symptom of a broader missing enforcement layer, not the root cause
themselves — per the brief's own instruction, icon replacement is addressed last (§6, Phase 4-5),
after the systemic gaps.

---

## PHASE 1 — Application UI Inventory

### Application chrome
- **Breadcrumb** (`screens/breadcrumb.rs`) — window drag region, project/sequence path, current
  screen title (`size 13.0 strong` — a value used nowhere else, see §2), window
  minimize/restore/maximize/close buttons via a dedicated `window_button()` helper with its own
  hover-color theming path, separate from every other button convention in the app.
- **Nav rail** (`screens/nav_rail.rs`) — Home/Editor/SoundLibrary/Queue/Prefs, `rail_button()`
  (icon-only, background appears only on hover/active) plus one always-boxed profile avatar
  (`Frame`, `corner_radius(8)`, permanent background) — two different icon-container
  conventions in the same 130-line file.

### Workspace (Editor screen)
Toolbar, media library / preview / properties three-column row, timeline strip — already
inventoried in `.claude/domain/TIMELINE.md` and `UI_IMPLEMENTATION_PLAN.md`. New findings from
this pass: the toolbar (`screens/editor/mod.rs::toolbar()`, lines 261-374) mixes **three**
button conventions in one function — icon+label (`tool_button()` calls, Cut, Timeline
Index/Transcript toggles), icon-only with tooltip (Undo/Redo, `↺`/`↻`), and label-only with no
icon (`AddVideoTrack`, `AddTextTrack`, `AddShapeTrack`, `MergeIntoComposite`, `Templates`).

### Timeline
Track headers, tool icons (`🎮`/`🎤`/`🎵` audio-role glyphs, `👁` visibility toggle falling back to
a plain `—` rather than a closed-eye glyph when hidden), clip badges (`❄` freeze, painted
directly onto the clip rect in `draw.rs:179`, not through any shared icon-button). Corner radius
on clip rects is hardcoded `4` at 10+ call sites in `timeline_panel/mod.rs`, diverging from
`theme.rs`'s global `8`.

### Media (Library screen, separate from the in-editor panel)
4-column grid, 210px cards, hand-rolled `egui::Frame` (not the shared `components::card_frame()`
Home uses for its own cards) — the same conceptual "media card" is built two different ways in
two different screens (`home.rs` vs `library.rs`).

### Sound Library
List, not grid, no search/filter, no selection state, category-grouped rows via
`components::section_label`.

### Preferences
Single scroll column, `card_frame()` sections (Language/Audio/Export/Project/Shortcuts/About),
`selectable_label` pill-groups for enums, a 3-column shortcut-remapping `egui::Grid` with inline
key capture. The one screen using explicit green/red (`ACCENT`/`ERROR`) success/error text
instead of muted-only text.

### Queue
Vertical `card_frame()` job rows, drag-handle glyph `⠿`, colored status pills (`tag_accent`/
`tag_outline`/`tag_error`), inline `ProgressBar`, per-status icon-button actions. One
non-neutral tinted background block (`ACCENT.gamma_multiply(0.10)` info banner) — otherwise the
only screen using a colored surface fill rather than a neutral one.

### Inspector (properties panel)
Video-clip section uses the shared `components::property_section`/`property_block` wrapper
consistently (separator → header → content → export note). Shape-clip and text-clip sections
mostly bypass it for their base (non-keyframe) fields, hand-rolling label+widget pairs with no
separator/spacing rule — switching selection from a video clip to a shape/text clip changes not
just content but the panel's visual rhythm.

### Controls inventory
Buttons (3 conventions, above), icon buttons (4 different styling paths — toolbar unstyled
inline, window-chrome themed helper, transport large colored `RichText`, delete-action bare
`small_button`), dropdowns (`selectable_label` pill-groups, no true dropdown/combo widget
observed outside `components/combo.rs`), 20 `egui::Modal` dialogs (consistent template, see
below), 1 context menu outside the editor (Home's project-card right-click), sliders
(`egui::Slider`/`DragValue`, no custom-painted control except one bespoke color-swatch button in
`text_clip.rs`), tabs (none found — Prefs uses a single scroll column instead), tooltips
(`.on_hover_text`, used inconsistently — present on icon-only buttons like Undo/Redo, absent on
most other icon-only affordances).

### States
- **Hover/active/selected/focused/disabled**: governed globally by `theme.rs`'s `Visuals` for
  standard widgets; custom-painted surfaces (timeline clips, nav rail) implement their own
  state logic per-component, with at least one confirmed inconsistency (nav rail's
  conditional-background icons vs. its own always-boxed avatar).
- **Empty**: consistently plain muted text, zero icon/illustration anywhere sampled (Library,
  Sound Library ×2, and implicitly Queue which has no empty-state message at all — a genuinely
  missing state, not just an unstyled one).
- **Loading**: inline muted status text next to action buttons (Library's "importing…"),
  `ProgressBar` in Queue rows, a pulsing indicator convention exists in the separate
  `ui.html` watched-folder mockup but has no equivalent in the actual egui app.
  **Error**: three different mechanisms — transient toast (Home), colored status pill (Queue),
  colored inline text (Prefs) — no shared "error state" component.

---

## PHASE 2 — Current Design Language

### Typography
No enforced type scale. Page/section titles alone use **four unrelated literal sizes**: 20.0
(Library/Queue/SoundLibrary/Prefs, mutually consistent), 22.0 (Home, sole outlier), 15.0 (all
~20 modals, internally consistent with each other but not with screen titles), 13.0 (breadcrumb
current-screen label, unique), plus a 28.0 ACCENT-colored outlier in the About dialog that
breaks even its own modal convention. `components/section.rs::section_label()` (11.0, muted,
strong, uppercase) is the one genuinely shared subsection-header primitive and is used
correctly where it's used — but page/modal titles never reach for an equivalent shared constant,
each screen re-declaring its own number.

### Spacing
`theme.rs` centralizes exactly two values (`item_spacing = 8,8`, `button_padding = 10,6`).
Everything else is ad hoc: a sample of ~30 `add_space` call sites turned up 10 distinct literal
values (2/4/6/8/10/12/14/16/20/24) with no discernible rhythm — adjacent, visually-equivalent
gaps use different numbers in different files (e.g. a title-to-content gap is 6.0 in one modal
and 8-10.0 in another). This is "arbitrary spacing values" exactly as the brief's Phase 2
predicted, not a hypothetical risk.

### Color
Genuinely disciplined at the token level (`theme.rs`'s BG/SURFACE/SURFACE_2/BORDER/TEXT_*/
ACCENT/ACCENT_2/ERROR) and correctly reused for structural borders throughout. Two real gaps:
(1) the timeline/editor drawing code (`timeline_panel/mod.rs`, `editor/mod.rs`) introduces raw,
un-tokenized `Color32` values and local variables (`peak_color`, `border_color`) for emphasis
strokes rather than a named semantic token; (2) Queue's tinted info banner and Prefs' green/red
success/error text are the *only* two places semantic (non-neutral, non-selection) color is used
at all — everywhere else, "feedback" collapses to either a colored pill (Queue's status tags,
already fine) or plain muted text, meaning there is no consistent warning/success/info token
applied application-wide, only ad hoc appearances of `ACCENT`/`ERROR` repurposed for that job.

### Borders
Structural borders are the strongest-governed part of the system: 1.0px + `theme::BORDER` is
correctly reused almost everywhere sampled (`sound_library.rs`, `nav_rail.rs`, `frame.rs`,
`library.rs`, `breadcrumb.rs`, most of `editor/mod.rs`). The exceptions are concentrated in
canvas-painted emphasis strokes (selection outlines, waveform peak markers) at 1.5/2.0/3.0px
with occasional raw colors — defensible for custom painting, but ungoverned by any shared
constant, so a future addition has no obvious weight/color to copy.

### Rounding
The one area with a real, measurable inconsistency: **at least four different corner radii (4,
6, 8, 10) are used for visually equivalent "card/chip" surfaces**, against a single global
default of 8 set in `theme.rs`. Timeline clips hardcode 4 at 10+ sites; several screen-level
cards use 6; pills correctly use 200 (semantically distinct — full pill, not a bug). This is not
"excessive rounding" in the brief's generic-AI-aesthetic sense (values are modest, 4-10px, not
oversized `rounded-2xl`-style radii) — it's **unmanaged** rounding, several near-identical
surfaces looking subtly different from each other for no expressed reason.

### Shadows
None found anywhere in the sampled code (`theme.rs` sets no shadow tokens, no screen adds one).
Not a problem — the brief explicitly warns against decorative shadow overuse, and this
application has the opposite issue (zero elevation cues anywhere, including in genuinely
overlapping contexts like modals over the workspace) — worth a deliberate, minimal exception for
modal elevation specifically (see §5), not a wholesale shadow system.

### Density
Reasonably dense already (13px base font, tight `item_spacing`, no oversized touch-target
padding) — this is closer to "appropriate for professional software" than to the generic-SaaS
failure mode the brief warns about. The inconsistency is in *rhythm*, not baseline density: the
same panel changes visual density depending on which sub-path (wrapped `property_section` vs.
raw label+widget) a given clip-type inspector section happens to use.

### Component consistency
The clearest, most fixable finding in this audit: **shared primitives already exist
(`components/{frame,section,property,combo,tag}.rs`) and are correctly used in roughly half the
places that need them, and silently bypassed in the other half** — Home uses `card_frame()`,
Library hand-rolls an equivalent `egui::Frame` instead of reusing it; the properties panel's
video-clip section uses `property_section`, its shape/text-clip sections mostly don't; nav
rail's own two icon styles disagree with each other in one file. This is not a case of missing
infrastructure — it's a case of the infrastructure not being the mandatory path.

---

## PHASE 3 — Iconography Audit

**No icon system exists.** Confirmed via `Cargo.toml` (no `egui_phosphor`/material-icons/any
icon-font crate) and a full grep for font-loading code (zero matches for `FontData`/
`insert_font`/icon-font patterns). Every glyph in the application is a literal Unicode/emoji
character typed directly into `RichText`/button strings and, in many cases, duplicated straight
into `i18n.rs` translation entries — meaning the icon and the translated label are fused into one
untyped string with no separation between "which icon" and "what it says."

**Four incompatible styling conventions coexist for what is conceptually one thing (an icon
button):**
1. Toolbar: `format!("{glyph} {label}")` inline concatenation, default size, no color.
2. Window chrome: bare glyph through a dedicated `window_button()` helper with named
   hover-color params — its own theming path.
3. Transport controls: glyph-only, explicitly enlarged (48-64pt) and colored
   (`theme::TEXT_MUTED`) `RichText` labels.
4. Delete/utility actions: `ui.small_button(glyph)`, icon-only, default chrome, no color.

**Mixed glyph vocabularies within the same concept**: emoji (`🗑`, `🎬`, `🤖`, `📦`, `🎨`, `✂️`
with variation selector) sit beside plain Unicode symbols (`▶`/`⏸`/`⏮`/`⏭`, `↺`/`↻`, `⇄`/`↕`/`⇉`)
and, for the same conceptual action, both forms appear in different files — `✂` (U+2702, no
emoji presentation) in the toolbar/nav-rail vs. `✂️` (with variation selector, renders as full
emoji) in `i18n.rs`'s `DetectSpeechEdits` string, for what is the same "cut/edit" concept.
Several actions are icon-only with no label anywhere but a hover tooltip (Undo/Redo); several
others are label-only with no icon at all (`AddVideoTrack`, `MergeIntoComposite`, `Templates`) —
so recognizability is inconsistent even within one toolbar.

**Role taxonomy found** (per the brief's categories): navigation (nav rail), action (toolbar,
delete buttons), state (visibility eye, freeze badge, status pills — these already correctly use
color+shape rather than icon alone in most cases, e.g. Queue's status pills), object identity
(media-kind glyphs `▶`/`♪` on cards), track/media type (audio-role `🎮`/`🎤`/`🎵`), feedback
(none dedicated — folded into color tokens, see §2 Color).

**Where icons are genuinely unnecessary or actively confusing**: the `👁` visibility toggle
falling back to a bare `—` character (not a closed-eye glyph) when off is a real ambiguity — a
user has no glyph-level cue for "hidden" versus e.g. "no icon available." The `✂` vs `✂️`
duplication is redundant complexity for zero benefit.

---

## PHASE 4 — Information Architecture Audit

Per-region assessment, since the brief asks this explicitly rather than folding it into the
visual audit:

| Region | Primary task | Always visible | Contextual (correctly) | Contextual (currently isn't, should be) |
|---|---|---|---|---|
| Toolbar | Tool/action selection | Select/Trim/Cut/Undo-Redo | — | AI-detection actions (silence, speech-edit, chapter, highlight) currently sit in the same flat row as core edit tools regardless of editing mode — these are lower-frequency and already flagged for grouping in `UI_IMPLEMENTATION_PLAN.md` §8 Phase 2 |
| Properties panel | Edit selected object | Panel frame | Content by clip type (correct, already works) | Visual *rhythm* isn't contextual when content is — a shape/text clip gets a visibly sparser, unseparated layout than a video clip for no intentional reason |
| Media library (editor) | Find/insert media | Search, grid | — | — |
| Timeline toolbar | Zoom, track count | — | — | — |
| Nav rail | Screen switching | 5 icons + prefs | — | — |
| Home | Project selection | Cards | Right-click menu (correct) | — |
| Prefs | Settings | All sections at once (single scroll column) | — | Six unrelated concern areas (Language/Audio/Export/Project/Shortcuts/About) always fully expanded — no progressive disclosure; a settings screen is exactly where the brief's "not all functionality simultaneously" principle applies most directly and currently doesn't |

**Overall IA finding**: most per-screen information architecture is already reasonably sound
(context-aware inspector, search in media library, right-click menus where useful) — the
toolbar's flat AI-detection-tools-mixed-with-core-tools problem and Prefs' un-sectioned
single-scroll layout are the two concrete violations of progressive disclosure found.

---

## PHASE 5 — Global Design System (proposed tokens)

This section defines what should become the *enforced* source of truth — extending, not
replacing, `theme.rs`.

### Visual personality
Professional, precise, fast, focused, mature, calm, powerful — reaffirmed from
`.claude/design/DESIGN_SYSTEM.md`; this audit found no reason to change the target personality,
only to actually enforce it.

### Surfaces (already correct in `theme.rs` — no change needed)
`BG` / `SURFACE` / `SURFACE_2` / `BORDER`, plus a new **`SURFACE_HOVER`** distinct from
`SURFACE_2` if hover and elevated-static currently share a color by coincidence rather than
design (verify: `theme.rs:54,60-61` currently use `SURFACE_2` for both inactive widget fill and
hover fill — confirm this is intentional before adding a new token; if it's accidental overlap,
split it).

### Text (already correct — `TEXT_PRIMARY`/`TEXT_SECONDARY`/`TEXT_MUTED`, add nothing)

### Interaction (already correct via `Visuals` — no new tokens needed, only enforcement that
custom-painted surfaces reuse the same four states rather than inventing per-component logic)

### Feedback — new, real gap
Currently `ACCENT`/`ERROR` are overloaded to mean both "selection/brand" and "success/failure."
Proposal: keep `ACCENT` for selection/brand (unchanged), keep `ERROR` for destructive/failure
(unchanged, already used this way in Queue/Prefs), and add two new explicitly-named tokens:
- **`WARNING`** — currently nonexistent; nothing in the sampled code has a warning state, but
  the brief's token list requires one and Queue's "Paused" status is arguably a warning-adjacent
  state currently rendered as a neutral outline pill.
- **`INFO`** — Queue's info banner currently reuses `ACCENT.gamma_multiply(0.10)` ad hoc; give
  it a real name so a second consumer doesn't reinvent the multiply-by-0.10 convention.

### Editor semantics — new, real gap
Track/clip category color currently has **no semantic token layer** — `color_label:
Option<[u8;3]>` on `Track` is free-form user-assigned RGB (correct, intentional, don't change),
but the *default* colors used before a user assigns one (video track teal, audio track olive/
tan in the mock, seen as inconsistent ad hoc choices across `timeline_panel/mod.rs`,
`draw.rs`, and the properties panel) should be named tokens (`TRACK_VIDEO_DEFAULT`,
`TRACK_AUDIO_DEFAULT`, `PLAYHEAD`, `MARKER_STANDARD`/`MARKER_TODO`/`MARKER_HIGHLIGHT`/
`MARKER_CHAPTER`, `KEYFRAME_DIAMOND`) rather than the current mix of literal `Color32`s and
`ACCENT`/`ACCENT_2` reused without a semantic name specific to "this is a marker" vs. "this is
selection."

### Spacing — new, the highest-leverage single fix in this document
Replace the current 10-distinct-literal free-for-all with a named scale on `theme.rs`:
```
SPACE_XS = 4.0   // inline glyph-to-label gaps, tight rows
SPACE_SM = 8.0   // already the global item_spacing default
SPACE_MD = 12.0  // section-to-section gaps
SPACE_LG = 20.0  // page-level top/bottom padding
```
Four values covers the observed range; anything currently using 2/6/10/14/16/24 should collapse
to its nearest neighbor during the rollout (§6), not gain a fifth token to match every existing
literal — the point is discipline, not cataloging what already exists.

### Corner radius — new, second-highest-leverage fix
```
RADIUS_SM = 4.0   // dense inline chips (timeline clips at close zoom, tags)
RADIUS_MD = 8.0   // the existing global default — cards, panels, buttons
RADIUS_PILL = 999.0  // status pills, nav avatar — already correctly distinct
```
Collapse the current 4/6/8/10 spread into these three. `RADIUS_MD` should be the default for
every card-like surface (Home cards, Library cards, modal frames) — the fact that Home and
Library currently disagree (`card_frame()` vs. hand-rolled Frame) should resolve to both using
`RADIUS_MD` through the same shared component, not two components independently agreeing on the
same number.

### Typography — new, third-highest-leverage fix
```
TEXT_TITLE   = 20.0  strong   // screen-level page titles (Library/Queue/SoundLibrary/Prefs
                               // already agree here — Home's 22.0 and breadcrumb's 13.0 should
                               // conform, not gain their own tier)
TEXT_MODAL_TITLE = 15.0 strong // already the de facto standard across ~20 modals — formalize it,
                               // fix the About dialog's 28.0 outlier to use it or a deliberately
                               // named TEXT_DISPLAY tier if a hero moment is truly wanted there
TEXT_SECTION = 11.0 strong uppercase muted  // = existing section_label(), unchanged
TEXT_BODY    = 13.0            // default egui base size, unchanged
TEXT_LABEL   = 12.0 muted      // property-panel field labels, unchanged, just named
TEXT_META    = 10.0-10.5 muted // timestamps, codec tags, unchanged, just named
```

### Component system — enforcement rules, not new components
1. **Card/frame surfaces** (project cards, media cards, list rows, modal bodies) must go through
   `components::card_frame()` — Library's hand-rolled `Frame` is the one confirmed violation to
   fix first, since it's a single, well-scoped call site.
2. **Icon buttons** must go through one new shared `icon_button(glyph, tooltip, size) -> Response`
   in `components/` — collapsing today's four conventions (toolbar inline, window-chrome helper,
   transport large-label, delete small_button) into one, with background-on-interaction-state as
   the *only* place a permanent box may appear (nav rail's avatar exception should be
   deliberately re-justified or converted to match).
3. **Property rows** must go through `components::property_section`/`property_block` — the shape/
   text-clip panels' raw label+widget stacks are the confirmed violation.
4. **Section/page titles** must reference the new `TEXT_TITLE`/`TEXT_MODAL_TITLE` constants
   defined above rather than inline `.size(N.0)` literals.

---

## PHASE 6 — Redesign Plan

Deliberately systemic-first, matching the brief's own instruction not to start with icons.

### Stage 1 — Token consolidation (Foundation)
Add `SPACE_*`, `RADIUS_*`, `TEXT_TITLE`/`TEXT_MODAL_TITLE` constants to `theme.rs`. Zero visual
risk on its own (additive); real value comes from Stage 2 adopting them.
**Files**: `crates/ui/src/theme.rs`.

### Stage 2 — Component enforcement sweep
Fix the four confirmed component-bypass violations in priority order (highest-visibility first):
1. Library's hand-rolled card `Frame` → `components::card_frame()`.
2. Shape/text-clip property panels → wrap base fields in `components::property_section`/
   `property_block` to match the video-clip section's rhythm.
3. Nav rail's two disagreeing icon conventions → pick one (state-conditional background,
   matching the rest of the app's interaction-state philosophy) and convert the avatar.
4. Corner radius sweep: replace hardcoded 4/6/10 literals with `RADIUS_SM`/`RADIUS_MD` at the
   ~15 confirmed sites (10+ in `timeline_panel/mod.rs` alone, plus `home.rs`, `library.rs`,
   `sound_library.rs`, `modals.rs`, `nav_rail.rs`).
**Files**: `crates/ui/src/screens/library.rs`, `crates/ui/src/screens/editor/properties_panel/
{shape_clip,text_clip}.rs`, `crates/ui/src/screens/nav_rail.rs`,
`crates/ui/src/screens/editor/timeline_panel/{mod,draw}.rs`, `crates/ui/src/screens/home.rs`,
`crates/ui/src/app/modals.rs`.

### Stage 3 — Typography and title consolidation
Replace the four unrelated title-size literals (22.0/20.0/15.0/13.0 + the 28.0 About outlier)
with the new named tiers. Fix Home's 22.0 → `TEXT_TITLE` (20.0); resolve the About dialog's
28.0 deliberately (either conform to `TEXT_MODAL_TITLE` or keep a named, deliberate
`TEXT_DISPLAY` exception, not an unlabeled outlier).
**Files**: `crates/ui/src/screens/home.rs`, `crates/ui/src/app/modals.rs`,
`crates/ui/src/screens/breadcrumb.rs`.

### Stage 4 — Feedback token rollout
Introduce `WARNING`/`INFO` tokens; convert Queue's `ACCENT.gamma_multiply(0.10)` info banner and
its "Paused" pill to reference them by name.
**Files**: `crates/ui/src/theme.rs`, `crates/ui/src/screens/queue.rs`.

### Stage 5 — Icon system (only now, per the brief's explicit ordering)
Design one coherent icon language: pick a single stroke-weight/fill philosophy, define a fixed
size scale (matching `RADIUS_SM`/`RADIUS_MD` container sizes), and route every icon-button
through the new shared `icon_button()` component from Phase 5 §Component system item 2. Resolve
the confirmed `✂`/`✂️` duplication and the `👁`-falls-back-to-`—` ambiguity as part of this pass,
not before it — replacing icons before the container/sizing/state system exists would just
produce a different set of inconsistent glyphs in the same inconsistent containers.
**Files**: new `crates/ui/src/components/icon_button.rs`; every call site currently listed in
Phase 3's four-conventions breakdown (toolbar, window chrome, transport, delete actions,
timeline badges, nav rail).

### Stage 6 — Progressive disclosure fixes
Group the toolbar's AI-detection actions separately from core edit tools (already scoped in
`UI_IMPLEMENTATION_PLAN.md` §8 Phase 2 — this audit confirms it independently from the IA angle,
not just the earlier growth-risk angle). Section Prefs into collapsible or tabbed groups instead
of one always-fully-expanded scroll column.
**Files**: `crates/ui/src/screens/editor/mod.rs`, `crates/ui/src/screens/prefs.rs`.

### Stage 7 — Missing states
Add an explicit empty-queue message (Queue currently has none at all — the one confirmed
missing, not just unstyled, state). Give empty states a slightly stronger visual treatment than
bare muted text — a small glyph from the new icon system plus the existing muted-text pattern,
once Stage 5 exists to draw from (sequenced after icons deliberately, not before).
**Files**: `crates/ui/src/screens/queue.rs`, `crates/ui/src/screens/library.rs`,
`crates/ui/src/screens/sound_library.rs`.

---

## Risk Assessment

**High risk**: none of the above touches domain logic, undo/redo, or timeline interaction —
every item in this plan is presentation-layer only. The nearest thing to a high-risk item is the
corner-radius sweep inside `timeline_panel/{mod,draw}.rs`, purely because that file is dense and
frequently touched — mitigate by changing only the radius argument at each site, nothing else,
one focused commit.

**Medium risk**: the icon-button consolidation (Stage 5) touches the largest number of call
sites of anything in this plan (5+ files, dozens of sites) — mitigate by landing the shared
`icon_button()` component first with zero callers, then migrating call sites in small batches
per screen, verifying visually after each batch (per repo `CLAUDE.md`'s own guidance to check UI
changes in a running instance).

**Low risk**: token additions (Stage 1), typography consolidation (Stage 3), feedback tokens
(Stage 4) — additive or single-value swaps with no structural change.

## Acceptance Criteria

- No two visually-equivalent card/chip surfaces use different corner radii without a named,
  deliberate reason (pill vs. card is a reason; "nobody normalized it" is not).
- Every page-level title and every modal title reads its size from a named constant, not an
  inline literal.
- Icon buttons across the app share one component, one size scale, and one interaction-state
  background rule — verified by grepping for direct `RichText::new(<emoji>)` construction outside
  the new `icon_button()` component returning zero matches in toolbar/transport/nav-rail/modal
  contexts.
- Queue shows an explicit empty state.
- Nothing in this plan changes any `avcore` domain type, undo semantics, or timeline hit-testing
  behavior — a regression in any of those during this work is a scope violation, not an
  acceptable side effect.

## Relationship to `UI_IMPLEMENTATION_PLAN.md`

That document (functional/performance-oriented: viewport culling, splitter, marquee select)
and this one (visual-system-oriented) are complementary, not competing — Stage 6 here explicitly
reuses that plan's toolbar-grouping proposal rather than re-deriving it. Sequence them
independently; neither blocks the other.
