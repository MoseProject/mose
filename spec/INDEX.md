# spec/ — Start Here

Split out of `CLAUDE.md`'s old single giant "Status" section (2026-08-27), same pattern used
in nimble (sibling project): a task only needs the 2-3 small files relevant to it, not one
huge file.

## Reading order

1. **[ROADMAP.md](ROADMAP.md)** — the actionable queue. Pick your next item here first.
2. **[RULES.md](RULES.md)** — mandatory rules + definition of done. Short, read every time.
3. The specific `matrix/*.md` (current status + file references) and/or `architecture/*.md`
   (the _how_/_why_) file(s) ROADMAP.md points you to.

For the post-P4 product-growth queue, read
**[architecture/competitive-feature-plan.md](architecture/competitive-feature-plan.md)** after
ROADMAP and RULES. It contains the current competitor evidence, ordered implementation slices,
acceptance criteria, security constraints, and deliberate non-goals.

For anything touching the desktop shell (`apps/desktop`), read
**[architecture/tauri-migration.md](architecture/tauri-migration.md)** first: decisions, data
flow, and the migration's own history (the original `egui`/`eframe` shell, `crates/ui`, is
deleted — this doc records why parity was reached and when).

Before broad beta distribution, read
**[architecture/client-error-reporting.md](architecture/client-error-reporting.md)** for ER-01's
consent, sanitization, offline delivery, symbolication, and native-crash rollout requirements.

Before implementing accounts, billing, feature gates, or commercial packaging, read
**[architecture/monetization-and-licensing.md](architecture/monetization-and-licensing.md)** for
MON-01's Free/Pro boundary, monthly price, downgrade guarantees, entitlement protocol, GPL
distribution obligations, security controls, metrics, and rollout gates.

For the expanded offline text catalog, read
**[architecture/built-in-font-catalog.md](architecture/built-in-font-catalog.md)** for FONT-01's
curated families, official sources, package budget, licenses, renderer migration, and cache design.
Read **[architecture/complex-text-shaping.md](architecture/complex-text-shaping.md)** with it when
working on bidi, ligatures, Unicode line breaking, timed highlight clusters, or international
fallback.

For the plan to reach 100% test coverage (baseline numbers, exclusion policy, phases), read
[`architecture/test-coverage-plan.md`](architecture/test-coverage-plan.md).

For the standalone replacement of GStreamer preview playback (done, GStreamer removed; plan in
[`architecture/gstreamer-removal-plan.md`](architecture/gstreamer-removal-plan.md)), read
**[architecture/preview-engine-replacement.md](architecture/preview-engine-replacement.md)**.
It intentionally stops before any `core` or UI integration. Current status, verification
and measured numbers: **[matrix/bush.md](matrix/bush.md)**.

Do not read every file here for one task. Read `matrix/changelog.md` only when you need the
historical "why was it built this way" detail (verification caveats, real bugs found, empirical
discoveries) — it's a narrative log of already-completed work, not a task list.

## Directory map

```text
spec/
├── INDEX.md                          ← you are here
├── ROADMAP.md                        ← actionable P0-P6 queue, START HERE
├── RULES.md                          ← mandatory rules + definition of done
├── matrix/                           ← WHAT is done/needed, per area
│   ├── engine.md                     (avbridge: probe/export/loudness/proxy/GPU encode)
│   ├── timeline-and-editing.md       (timeline model, editing ops, sequences, undo/snap gaps)
│   ├── effects-and-color.md          (visual effects, keyframes, masks, transitions, LUTs)
│   ├── ai-features.md                (bg removal, auto-reframe, motion tracking, TTS, Whisper)
│   ├── preview-pipeline.md           (historical: GStreamer pipeline, replaced by Bush)
│   ├── bush.md                       (Bush native preview engine: status, tests, benchmarks)
│   ├── robustness.md                 (autosave, crash recovery, logs, prefs, export queue)
│   ├── performance.md                (proxy, filmstrip, hw decode, telemetry)
│   ├── packaging.md                  (installers, bundling, auto-update)
│   ├── competitor-parity.md          (real feature research: CapCut/DaVinci/Premiere/FCP gaps)
│   └── changelog.md                  (large historical log — read only for "why", not "what's next")
└── architecture/                     ← HOW to build it (principles, not per-feature status)
    ├── tauri-migration.md            (egui -> Tauri + React: decisions, parity, remaining gaps)
    ├── differentiators.md            (D1-D7: the proposed differentiator feature specs)
    ├── competitive-feature-plan.md   (CF-01-CF-10: next competitive implementation plan)
    ├── monetization-and-licensing.md (MON-01: Free/Pro subscription and GPL model)
    ├── client-error-reporting.md      (ER-01: secure remote error and crash collection)
    ├── built-in-font-catalog.md       (FONT-01: 43-family offline font catalog)
    ├── complex-text-shaping.md        (TEXT-01: bidi, OpenType shaping, cluster-safe layout)
    ├── performance-and-caching.md    (dirty flags, versioned cache, hot-path rules — from nimble)
    ├── undo-redo.md                  (P0 item 1: what's done, what's left, why per-sequence)
    ├── mobile-support.md             (Android/iOS ADR: proposed, not started — decision only)
    ├── editor-ui-visual-redesign.md  (OCA mockup → Editor screen mapping, sampled colors)
    └── rules-and-dod.md              (full text behind RULES.md's summary)
```

## Keeping this in sync

When you finish a `ROADMAP.md` item:

1. Flip its `- [ ]` to `- [x]` (or `[~]`) in the relevant `matrix/*.md` file, one-line note +
   file reference.
2. Update its line in `ROADMAP.md` too — matrix files are the detailed source, ROADMAP is the
   reconciled summary; they can drift, ROADMAP wins on "what's next."
3. Doc-update commit, separate from the code commit (`docs: mark <item> done`).

Never overwrite a spec file wholesale — edit the specific line/section touched.

---

Also relevant, outside `spec/`: `CLAUDE.md` (repo-wide conventions, crate architecture, build
gotchas), `AGENTS.md` (same content, Codex-facing mirror), `features/request.md` (the original
phased plan this queue picks up after — kept as-is, historical).
