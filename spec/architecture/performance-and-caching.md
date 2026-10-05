# Performance & Caching Patterns (ported from nimble)

nimble (sibling project, a from-scratch browser engine) formalized architecture rules that
apply here too — the shapes are the same: a mutable document (DOM there, timeline here) feeding
an expensive derived structure (layout tree there, filter graph / GStreamer pipeline here) that
gets rebuilt far more often than it needs to. Ported 2026-08-27; see nimble's
`spec/architecture/primitives.md` and `spec/RULES.md` for the originals.

## 1. Dirty flags, not blanket invalidation

A mutation should classify *what* needs recomputing (position vs. content vs. effect vs. track
membership), not force a full rebuild of everything downstream.

**Where this applies in oca:** moving a clip on the timeline (position only) shouldn't force
`preview.rs` to tear down and reopen the whole GStreamer pipeline the way changing its *content*
or *effect chain* does. A per-mutation classification (position / effect / trim / track) lets
the preview layer choose: reseek only, rebuild one branch, or rebuild the whole composited
pipeline. Today every edit likely triggers the same "reopen" path — worth confirming and fixing
if so (see `ROADMAP.md`).

## 2. Versioned cache, recomputed on demand

Keep the last expensive result plus the exact inputs that produced it; a new request only
recomputes if an input actually changed.

**nimble's shape** (`Page::layout_cache` in `profile-worker/page.rs`): keyed on
`(width, dom.layout_version(), adopted_stylesheet_version())` — a cache hit just clones the
stored tree instead of rebuilding it.

**Where this applies in oca:** `ClipInstance::video_filter_chain()` /
`resolve_timeline_segments_multi()` rebuild the avfilter graph from scratch on every call. A
per-sequence version counter (bumped on real clip/effect/track edits, *not* on playback
position changes) lets the resolved filter graph be reused across a scrub or a paused
preview tick instead of rebuilt every frame.

## 3. No allocation/parsing in the hot path

No string building, no repeated parsing, no repeated map lookups in a path that runs every
frame — intern it, cache it, or hoist it out.

**Where this applies in oca:** `keyframe_video_filter_chain()` / `position_overlay_xy_expr()`
build avfilter expression strings from scratch. Fine for export (runs once per job); if the same
path ever drives live preview per-frame for a keyframed clip, cache the built expression and
only rebuild it when the keyframe list itself changes, not every frame it's evaluated against.

## 4. No permanent fake APIs

A stub presented as working, silently, is worse than an explicit error. Every scope cut gets
documented as incomplete, tested for what *is* real, and never described as "done."

**Already the dominant culture in this codebase** — `CLAUDE.md`'s old Status section (now
`matrix/changelog.md`) is full of "Verification caveat," "hard wall, not just unattempted,"
"confirmed working end-to-end" language throughout. This file exists to make that an explicit,
citable rule rather than an implicit convention each entry re-derives independently. Follow the
same standard for every new feature going forward — see `rules-and-dod.md`.

## 5. Reuse an existing primitive before building a new one

Before adding a new state machine/cache/sampler for a feature, check whether an existing shared
one already does the job.

**Concrete candidate found in oca, now extracted:** frame-sampling-via-seek-and-poll against a
`Preview` pipeline was independently reimplemented in `ui`'s auto-reframe, motion tracking, and
background-removal matte generation background-thread functions (plus a fourth copy in
`import.rs`'s thumbnail extraction). `avcore::FrameSampler` (`crates/core/src/av/frame_sampler.rs`)
is now the one shared primitive — open once, `sample(at_secs, deadline)` seeks and polls
`current_frame()` until it decodes or the deadline passes (`None` on timeout, the same "skip
this sample, don't fail the whole run" tolerance every caller already had), plus
`even_sample_times` for the "N evenly spaced timestamps across a duration, clamped" bookkeeping
motion-tracking/background-removal each did inline. All four call sites migrated (2026-08-27).
**Left open, a separate concern**: `avcore::background_removal::segment_person` builds a fresh
`ort::session::Session` on every call (confirmed by reading it, 2026-08-27 — no doc comment
currently calls this out) — session-reuse-across-samples is a different optimization (amortizing
model load, not frame decoding) than what `FrameSampler` covers, and wasn't in scope for this
extraction.

## 6. Central mutation pipeline

Every document mutation should flow through one classification point, not scattered
ad hoc invalidation calls sprinkled across call sites.

**nimble's shape:** `Dom::mark_dirty(flags)` — every mutation method goes through it, classifying
into `DirtyFlags` (structural / collection / selector / style / layout / paint / a11y).

**Where this applies in oca:** if dirty-flags (pattern 1) gets built, it should live as one
method every `Timeline`/`Track`/`ClipInstance` mutator calls through — not a per-call-site
`invalidate_preview()` sprinkled by hand at each edit operation, which drifts out of sync as new
edit operations get added.

---

[← back to spec/INDEX.md](../INDEX.md)
