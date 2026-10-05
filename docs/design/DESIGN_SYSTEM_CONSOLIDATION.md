# Design System Consolidation Review

Analysis only — no code changed by this document. Follows `UI_DESIGN_AUDIT.md`'s finding that
the problem is enforcement, not absence of infrastructure. This review inspects the actual
existing source of truth (`crates/ui/src/theme.rs`, `crates/ui/src/components/{mod,frame,
section,property,combo,tag}.rs` — full contents read directly, not sampled) and defines the
minimum architectural change to make it the single enforced source, favoring consolidation over
a second system.

## 0. What already exists (read in full)

`components/mod.rs` re-exports exactly six functions: `enum_combo`, `card_frame`,
`property_block`/`property_section`/`property_toggle`, `section_label`,
`tag_accent`/`tag_outline`/`tag_error` (plus the underlying generic `tag`). All are
presentation-only free functions (no `Widget` impls, no `App` access) — this is a sound,
intentionally small surface, not an under-built one. `theme.rs` defines 12 color constants and
sets exactly two spacing values (`item_spacing`, `button_padding`) plus a single global corner
radius (8, with window radius 10). There is **no icon-button component, no typography-constant
module, and no named spacing/radius scale** — these three are genuine gaps, not
bypassed-but-existing infrastructure, and are addressed in §3-4, not §2's enforcement question.

---

## 1. Sources of Truth

| Design concern | Current implementations | Canonical implementation | Migration action |
|---|---|---|---|
| Color tokens | `theme.rs` constants (`BG`/`SURFACE`/`SURFACE_2`/`BORDER`/`TEXT_*`/`ACCENT`/`ACCENT_2`/`ERROR` + tints) — consistently reused for structural borders/fills; occasional raw `Color32`/local vars in canvas-painted code (`timeline_panel/mod.rs`, `editor/mod.rs`) | `theme.rs` (unchanged) | Refactor the ~6 confirmed raw-`Color32` emphasis-stroke sites in `editor/mod.rs`/`timeline_panel/mod.rs` to name a token instead of a local variable — no new tokens needed for this subset, just naming discipline |
| Panel/page fill | `theme::BG`/`SURFACE` direct references throughout | `theme.rs` (unchanged) | none — already canonical and consistently used |
| Card/list-row frame | `components::card_frame()` (radius 10, `SURFACE`, `BORDER` stroke, 14px margin) — used correctly by Home, Queue, Sound Library rows; **bypassed** by `library.rs` with a hand-rolled `egui::Frame` (radius 8, 8px margin, otherwise equivalent) | `components::card_frame()` | Refactoring (Category C, §2) — `library.rs`'s bypass is the only confirmed offender; delete its local `Frame` construction and call `card_frame()` instead. This also fixes the radius-10-vs-8 disagreement between Home and Library cards as a side effect, no separate radius fix needed for this pair |
| Section headers (subsection level) | `components::section_label()` (11.0, muted, strong, uppercase, fixed 6px trailing space) — correctly used in the video-clip properties panel and elsewhere | `components::section_label()` (unchanged) | none for this tier — the gap is one level up, at page/modal titles, which have no equivalent component at all (see next row) |
| Page/modal title | No shared component. Four independent literals: 22.0 (`home.rs`), 20.0 (`library.rs`/`queue.rs`/`sound_library.rs`/`prefs.rs`, mutually consistent), 15.0 (~20 modals in `modals.rs` + `transcript_panel.rs`, mutually consistent), 13.0 (`breadcrumb.rs`), plus one 28.0 outlier (About dialog) | **New**: two small helper functions, `components::page_title()` (20.0 strong) and `components::modal_title()` (15.0 strong) — not a generic "typography system," just the two tiers that already have de facto majority conventions | API improvement (Category B) then refactor (C) — add the two functions, then point every title call site at the matching one. Home's 22.0 and breadcrumb's 13.0 conform to `page_title()`/get their own justified exception (breadcrumb is chrome, not a page — keep its 13.0 but name it, see §3). About's 28.0 either becomes a deliberate, separately-named exception or is folded into `modal_title()` — this review recommends folding it in, since a 28.0-vs-15.0 hero moment inside a small settings dialog isn't earning its own tier |
| Property-row layout | `components::property_block`/`property_section`/`property_toggle` — used consistently by the video-clip section of the properties panel; **bypassed** by `shape_clip.rs` and `text_clip.rs` for all non-keyframe base fields (raw `ui.label(RichText::new(...).size(12.0).color(TEXT_MUTED))` + bare widget, no separator, no export note) | `components::property_section`/`property_block`/`property_toggle` (unchanged) | Refactoring (Category C) — this is the highest-value single migration in this review: it fixes the properties panel's confirmed "visual rhythm changes when switching clip type" finding from the UI audit with zero new code, purely by routing existing bypassed call sites through the existing wrapper |
| Icon buttons | Four independent conventions (toolbar inline `format!`, window-chrome `window_button()` helper, transport large colored `RichText`, delete-action bare `small_button`) — **no shared component exists at all** | **New**: `components::icon_button()` | This is a real gap, not a bypass — API improvement (Category B), building new infrastructure, not consolidating existing infrastructure. Scoped separately in §4 since it's the one component this review recommends actually adding, not just enforcing |
| Enum dropdowns | `components::enum_combo()` — no bypass found in the audit's sampling (Prefs uses `selectable_label` pill-groups deliberately for a different interaction shape, not as an `enum_combo` bypass — pill-groups and dropdowns are different, both legitimate, for different option-count/frequency tradeoffs) | `components::enum_combo()` (unchanged) | None — no confirmed bypass; do not force Prefs' pill-groups into `enum_combo` without evidence that's actually the better interaction for that screen |
| Status tags | `components::tag`/`tag_accent`/`tag_outline`/`tag_error` — used correctly in Queue; no bypass found elsewhere in the audit sampling | `components::tag*` (unchanged) | None |
| Spacing | No scale — 10 distinct literals observed (2/4/6/8/10/12/14/16/20/24) at `ui.add_space` call sites, `theme.rs` only centralizes `item_spacing`/`button_padding` | **New**: 4 named constants on `theme.rs` (§3) | API improvement (B) then gradual refactor (C) — see §3 for the exact mapping, not a blanket rewrite |
| Corner radius | `theme.rs` global default (8) constantly overridden: 10 (`card_frame`, window), 6 (several screens' inline cards/thumbnails), 4 (10+ sites in `timeline_panel/{mod,draw}.rs`), 200 (pills, semantically distinct, not a bug) | **New**: 3 named constants (§3) | API improvement (B) then refactor (C) |

---

## 2. Enforcement Strategy — why bypass is currently possible

The root cause is structural, not carelessness: **every shared component in `components/` is an
opt-in free function that returns/paints into a `ui: &mut Ui` the caller already has full access
to.** Nothing stops a screen from calling `egui::Frame::new()...` directly instead of
`card_frame()` — both are equally reachable, equally typed, and egui itself provides no
compiler-level distinction between "the app's card" and "any frame." This is a normal
consequence of egui's immediate-mode, function-based API, not a design mistake specific to this
codebase — and it means Category D (structural enforcement, making direct usage impossible) is
largely not available without fighting egui's own architecture (e.g. there is no practical way
to make `egui::Frame::new()` itself unavailable to a module without also blocking legitimate
one-off uses).

Per bypass category, the actual determination:

| Bypass | A: Docs only | B: API improvement | C: Refactor | D: Structural |
|---|---|---|---|---|
| `library.rs`'s hand-rolled card frame | — | — | **Yes** — single call site, trivial swap | Not needed, C fully resolves it |
| Shape/text-clip panels bypassing `property_section` | — | Maybe — see below | **Yes** — the real fix | Not practical (same egui limitation) |
| Page/modal titles' 4 literal sizes | — | **Yes** — no component exists yet to enforce against | **Yes**, after B | — |
| Nav rail's two icon conventions (state-conditional vs. permanent box) | — | — | **Yes** — one file, pick the state-conditional convention (matches the rest of the app's interaction-state philosophy) and convert the avatar | — |
| Icon buttons (4 conventions app-wide) | — | **Yes** — this is the one place this review recommends new infrastructure | **Yes**, after B, but large surface — see §4 rollout notes | — |
| Corner radius spread | — | **Yes** — name the 3 tiers | **Yes** | — |

**On why B (API improvement) sometimes beats C alone for the property-row bypass**: reading
`shape_clip.rs`/`text_clip.rs`'s actual field list (position/size/color/stroke, mostly single
plain-value rows with no separator/note wanted for every single field), it's plausible the
current `property_section`'s per-field separator+note ceremony is *too heavy* for a dense list of
simple fields — that's exactly why those files started bypassing it in the first place, not pure
oversight. Before mechanically forcing every field through `property_section`, this review
recommends checking whether a **lighter sibling**, e.g. `components::property_row()` (label +
widget, no separator, no note, matching what those files already hand-roll but as one named
function instead of six duplicated inline patterns) would better serve dense field lists while
`property_section` stays reserved for grouped/keyframe-bearing blocks that genuinely want the
separator+note ceremony. This keeps the *rhythm* consistent (both go through a named,
theme-driven helper) without forcing an ill-fitting heavier component onto a lighter use case —
a real API improvement, not just enforcement of the existing shape. Recommend this as a design
decision to confirm with the user/maintainer before implementing, since it slightly changes
`components/property.rs`'s public surface rather than just its call sites.

**Structural enforcement (D) is not recommended anywhere in this codebase.** The two mechanisms
that would actually work in Rust — a lint (clippy custom lint disallowing raw
`egui::Frame::new()` outside `components/`) or a `#[deprecated]`-style compiler nudge — are both
disproportionate to the actual bypass count found (a handful of confirmed sites, not dozens) and
would add tooling maintenance burden this project doesn't otherwise carry (no existing custom
lints). Category C (refactor the confirmed sites) plus a documentation note in
`.claude/engineering/EGUI.md` (already present: "reuse design tokens and components") is
sufficient at this scale — reserve D for if a second audit later finds the bypass count growing
despite that guidance.

---

## 3. Frozen Design Foundation

### Typography — two new tiers, not a generic scale

Per §1, only two genuinely missing tiers justify a new function: `page_title()` (20.0, strong —
already the de facto standard across 4 of 5 non-editor screens) and `modal_title()` (15.0,
strong — already the de facto standard across ~20 modals). `section_label()` (11.0) already
covers the subsection tier and needs no change. Body text (13.0, egui default), property labels
(12.0, muted — already consistent per the properties-panel audit), and metadata (10.0-10.5,
muted) are already de facto consistent and do not need a named wrapper function to stay that
way — adding one for tiers that already agree would be ceremony without enforcement benefit,
inconsistent with this review's "avoid abstraction layers without a clear enforcement benefit"
mandate. Breadcrumb's 13.0 is chrome-scoped (window title bar), not a page title — keep it
distinct, don't force it into `page_title()`.

### Spacing — 4 named constants, mapped from the existing 10 literals

```
SPACE_XS = 4.0   // absorbs: 2.0, 4.0 (nearest-neighbor round up for the rare 2.0 sites)
SPACE_SM = 8.0   // absorbs: 6.0, 8.0, 10.0 (already theme.rs's item_spacing default)
SPACE_MD = 12.0  // absorbs: 12.0, 14.0, 16.0
SPACE_LG = 20.0  // absorbs: 20.0, 24.0
```
This is a deliberate lossy collapse, not a catalog of what exists — the goal is a scale
disciplined enough to stop new arbitrary values, not a token for every historical number. Sites
currently using 6.0 for what's clearly a tight inline gap (matching `SPACE_XS`'s intent better
than `SPACE_SM`'s) should be judged case-by-case during the Stage-2 refactor from
`UI_DESIGN_AUDIT.md`, not mechanically bucketed by nearest numeric value alone.

### Radius — 3 named constants, not 4 preserved

```
RADIUS_SM   = 4.0    // dense inline chips: timeline clips, small badges
RADIUS_MD   = 8.0    // theme.rs's existing global default — cards, panels, buttons, modals
RADIUS_PILL = 999.0  // status tags, nav avatar if kept circular — semantically distinct, keep
```
`card_frame()`'s current `10` and the scattered `6`s are **not** preserved as a fourth tier —
both collapse into `RADIUS_MD` (8). This is a real, visible (if small) change to `card_frame()`
itself (10→8), justified because 10 vs. 8 has no expressed semantic reason to differ from the
global default it's already 2px away from — freezing it as a permanent third near-identical
value would repeat exactly the problem this review exists to close. Confirm this specific
2px change with the user before implementing, since it's the one place this document recommends
altering an existing canonical component's visual output rather than just its call sites.

### Surfaces — confirm one open question before locking

`theme.rs:54-61` currently uses `SURFACE_2` for both the inactive-widget fill and the
hovered-widget fill (`visuals.widgets.inactive.bg_fill` and `.hovered.bg_fill` are both
`SURFACE_2`). Before defining a canonical "hover surface" token distinct from "elevated static
surface," confirm whether this overlap is intentional (hover has no fill change, only the border
color changes to `ACCENT` — a deliberate, minimal hover treatment) or accidental. Reading
`theme.rs` in full, the border-color-only hover change combined with `SURFACE_2` reused for
inactive fill reads as **intentional** — egui's default `Visuals` shape naturally separates
"resting elevated fill" from "hover feedback via stroke," and this codebase's choice to keep fill
constant and let stroke carry hover state is consistent with the brief's own "restrained
emphasis" design principle. **Recommendation: do not add a new `SURFACE_HOVER` token** — the
existing `SURFACE`/`SURFACE_2` pair plus stroke-based hover is already the canonical, sufficient
answer; document this reasoning in `.claude/design/DESIGN_SYSTEM.md` so a future contributor
doesn't "fix" the apparent duplication.

### Interaction states — already canonical, no new definition needed

`theme.rs`'s `Visuals` block already defines idle/hover/active/selected consistently for every
*standard* egui widget. The only real gap is that **custom-painted surfaces** (timeline clips,
markers, nav-rail icons) each reimplement their own version of this state logic rather than
reading from a shared state-to-visual mapping. This review does not recommend inventing a new
abstraction for that (e.g. a generic `InteractionVisuals` struct) — the audit found only two
confirmed inconsistencies (nav rail's internal disagreement; timeline selection using
box-shadow-via-stroke while nothing else does), both narrow enough for Category C refactor
without new shared code.

---

## 4. Component Inventory

| Component | Classification | Rationale |
|---|---|---|
| `components::card_frame()` | **IMPROVE** | Radius 10→8 per §3; otherwise sound and correctly used by 3 of 4 candidate screens |
| `components::section_label()` | **KEEP** | Correctly scoped, correctly used, no changes |
| `components::property_block()` | **KEEP** | Correct as the "grouped block with note" primitive |
| `components::property_section()` | **KEEP** | Correct as the "labeled grouped block" primitive |
| `components::property_toggle()` | **KEEP** | Correct, narrow, no issues found |
| `components::enum_combo()` | **KEEP** | No bypass found; do not conflate with Prefs' intentionally different pill-group pattern |
| `components::tag`/`tag_accent`/`tag_outline`/`tag_error` | **KEEP** | Correctly scoped, correctly used, `WARNING`/`INFO` variants deferred to `UI_DESIGN_AUDIT.md`'s Stage 4, not needed at the component level, only at the token level |
| `property_row()` (new, lighter sibling of `property_section`) | **ADD** (pending confirmation, see §2) | Only if shape/text-clip's dense single-field pattern is confirmed as a legitimately different use case rather than pure bypass |
| `page_title()` (new) | **ADD** | Closes the 4-literal title gap identified in §1 |
| `modal_title()` (new) | **ADD** | Closes the same gap for the ~20-modal tier |
| `icon_button()` (new) | **ADD** | The one genuinely missing component — see rollout note below |
| Window-chrome `window_button()` helper (`breadcrumb.rs`) | **MERGE** (into `icon_button()`, deferred) | Once `icon_button()` exists and supports named hover-color overrides, `window_button()`'s distinct theming path becomes redundant — do not merge before `icon_button()` exists, and do not force window-chrome buttons (minimize/maximize/close) to lose their platform-conventional red-on-hover-close behavior in the process; `icon_button()`'s API needs to support that override from day one, not retrofit it |
| Nothing else in `components/` | — | The inventory is genuinely small (6 functions) — this review found no candidate for **DEPRECATE** or **REMOVE**; every existing component is used correctly somewhere and has no redundant sibling competing with it |

**On `icon_button()` specifically**: this review recommends defining it now at the design-plan
level (not implementing yet, per the task's own "do not modify code" instruction) with this
minimum shape: `icon_button(ui, glyph: &str, tooltip: &str, opts: IconButtonOpts) -> Response`,
where `IconButtonOpts` carries an optional size override (covering the transport controls'
48-64pt case) and an optional hover-color override (covering window-chrome's close-button-red
case) — two escape hatches, not open-ended configurability, keeping it a single canonical
component rather than becoming a fifth convention with extra steps. Confirm this shape before
implementation, since `UI_DESIGN_AUDIT.md`'s Stage 5 already scoped this component from the
audit side — this review's job was to check *whether a second, competing definition would
otherwise emerge*, and it would not: both documents converge on the same one component.

---

## Summary

No second design system is proposed. Every "new" item in this review is either (a) a small
number of named constants filling a confirmed token gap (spacing, radius, two typography tiers),
or (b) one genuinely missing component (`icon_button`) already scoped identically by
`UI_DESIGN_AUDIT.md`, or (c) refactors of existing bypass sites onto components that already
work. The two decisions this review flags as needing explicit confirmation before implementation
(collapsing `card_frame()`'s radius 10→8; whether `property_row()` is a legitimate lighter
sibling or shape/text-clip should simply be forced into `property_section` as-is) are the only
places this review's recommendation changes an existing canonical component's behavior rather
than purely adding or enforcing — everything else is additive or a call-site migration.

**Recommended implementation order** (once code changes are authorized): (1) the two confirmed
zero-risk refactors — `library.rs` → `card_frame()`, nav-rail icon convention unification; (2)
`page_title()`/`modal_title()` plus the title call-site migration; (3) spacing/radius constants
plus the `card_frame()` radius change (after confirmation); (4) the `property_row()` decision,
then the shape/text-clip migration; (5) `icon_button()`, matching `UI_DESIGN_AUDIT.md`'s Stage 5
sequencing (after the container/state system exists, icons last). This order front-loads the
zero-ambiguity, zero-confirmation-needed items and defers every item this review flagged as
needing a decision to later stages, so implementation can start immediately on (1)-(2) without
waiting on those decisions.
