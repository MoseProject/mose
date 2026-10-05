# Performance Analysis — Report

See `PLAN.md` for methodology. This is the results document: what was found, what was fixed, what
was measured, and what's recommended but not attempted this pass.

**2026-09-07 follow-up pass**: added an isolated `Lut3D::sample` benchmark (quantifies "the bigger
finding" below with a real number instead of a guess) and corrected two items this report
originally listed under "Recommended, not attempted" — timeline viewport culling turned out to
already be implemented (landed via an unrelated concurrent change since this report's first
version), and `preview.rs`'s per-track cloning turned out to not be a hot path at all once its
actual call frequency was checked. Both corrections, and the reasoning behind them, are in that
section below rather than silently edited away.

**2026-09-07, second follow-up**: re-ran `cargo clippy --workspace --all-targets -- -W
clippy::perf` to confirm nothing was missed. Two more real (if functionally inert)
`unnecessary_cast` findings turned up in production code (not the test-file/`theme.rs`-style ones
this report already scoped out) — `error_reporting.rs`'s event counter and `timeline/mod.rs`'s
multicam angle-switch duration calc were both casting an already-correctly-typed value to itself.
Fixed those, and — since two were already touched — finished the set by fixing the six `theme.rs`
occurrences and one `draw.rs` occurrence this report previously filed under "out of scope" for
being functionally inert; consistency won over re-litigating the same judgment call twice. This
closes out `clippy::perf`'s findings across the entire workspace: every remaining warning that
lint group produces is either in `#[cfg(test)]`/`tests/` code (real but not worth the diff noise
in a performance pass, per `PLAN.md`'s own criteria) or a `clippy::style`-adjacent nit
(`type_complexity`, `ptr_arg`, `too_many_arguments`, `enum_variant_names`) clippy files under the
same flag but isn't actually about runtime cost.

**2026-09-07, third follow-up — a real algorithmic fix, not a lint-driven one**: found by manual
inspection (`clippy::perf` doesn't catch this shape of thing) that `App::pump_preview_frame`
always calls `avcore::luma_waveform_rgba` and `avcore::vectorscope_rgba` back to back whenever
`scopes_enabled`, each independently iterating over the *entire* decoded source frame. Added
`avcore::render_scopes_rgba` (`crates/core/src/graphics/scopes.rs`), computing both scopes' per-pixel
accumulation in one shared pass over the source instead of two, and switched the one real call
site to it. See "Scopes: two passes merged into one" below for the measured result and how
correctness was confirmed.

## Summary

- Ran `cargo clippy --workspace --all-targets -- -W clippy::perf` across the whole workspace.
  Cross-referenced every finding against whether it sits on a real per-frame/per-call hot path.
- Fixed 3 genuinely hot findings in `crates/core/src/preview_effects.rs` (the CPU-side live-preview
  vignette/glitch/deflicker/LUT effects, applied to every decoded preview frame — see that file's
  own doc comment) plus a matching one in `crates/core/src/preview.rs`'s live-preview compositor,
  swapping `chunks_exact`/`chunks_exact_mut` for `as_chunks`/`as_chunks_mut` (chunk size becomes a
  compile-time const generic instead of a runtime value clippy's `chunks_exact_to_as_chunks` lint
  exists specifically to catch).
- Fixed 4 more small, real, `clippy::perf`-flagged issues elsewhere in `core`: an unnecessary
  `Vec` allocation+copy on every AI-background-removal inference (`&data.to_vec()` → `data`), a
  `contains()`-vs-manual-`iter().any()` in error-report validation, two `Vec::extend(repeat().
  take(n))` → `extend(repeat_n(n))` calls, and one `if let` inside a `for` loop → `.flatten()`.
- Built `perf/`, a real, runnable Criterion benchmark harness (see `PLAN.md` for why it has to be
  a standalone crate) and measured the three `preview_effects.rs` fixes' actual before/after cost
  at a realistic Full HD (1920x1080) frame size — not assumed from the lint's own rationale.
- **The measured result is more nuanced than "clippy said so, so it's faster"** — see below.

## Measured results (Criterion, 1920x1080 RGBA8 buffer, `perf/benches/preview_effects_bench.rs`)

Run with `cargo bench` from `perf/` (200 samples, 15s measurement window per benchmark — see that
file's own comment on why). Numbers are the reported median (Criterion's middle estimate of a
three-point confidence interval); this sandbox's CPU is shared/virtualized and visibly noisy (see
"A necessary caveat" below), so treat anything under roughly 10% as inside that noise floor, not as
a property of the code.

| Function | Before (`chunks_exact`) | After (`as_chunks`) | Delta |
|---|---|---|---|
| `apply_lut_to_rgba` | 83.7 ms | 76.3 ms | **~9-13% faster**, reproduced in every run |
| `apply_glitch_to_rgba` | 20.1 ms | 22.8 ms | inconclusive — direction flipped across repeated runs (see below) |
| `apply_deflicker_to_rgba` | 2.83 ms | 2.96 ms | inconclusive — within noise floor |
| `apply_vignette_to_rgba` (unchanged) | 22.6 ms | 23.6 ms | **same code, two runs** — this ~4% swing is the sandbox's own noise floor |

**`apply_lut_to_rgba` is a real, reproducible win.** Every one of 4 independent full benchmark runs
this session had `after` faster than `before` for this function specifically, by 9-14% each time —
consistent direction even though the exact magnitude moved around, which is the signature of a
real (if modest) effect rather than noise. It's also the most expensive of the four effects by far
(70-90ms per Full HD frame vs. 2-25ms for the other three) — see "The bigger finding" below for why
that absolute cost matters more than the relative improvement.

**`apply_glitch_to_rgba` and `apply_deflicker_to_rgba` did not show a reliable improvement.**
Across repeated runs, `glitch`'s before/after delta ranged from "after 21% slower" to "after 7%
faster" depending on the run — direction wasn't consistent, unlike LUT's. `deflicker`'s two
measured runs disagreed on direction too (one showed `before` "regressing" by 15% relative to a
prior run of the *same* unchanged-in-between code). Given `apply_vignette_to_rgba` — literally
unchanged between the two benchmark runs — itself shows a ~4% swing run-to-run, a single-digit or
low-teens delta on `glitch`/`deflicker` cannot be attributed to the `chunks_exact`→`as_chunks`
change with any confidence in this environment.

**Kept the fix anyway, for both.** `as_chunks`/`as_chunks_mut` is not just "maybe faster" — it also
moves a runtime `len % N == 0` bounds computation the old `chunks_exact*` calls performed on every
call into a compile-time fact (`N` is a const generic), which is a real, if here-unmeasurable,
reduction in what the function does per call, and it's `clippy`'s own current default-lint-level
recommendation (not a nursery/pedantic-only opinion) — reverting to a form clippy itself flags
because one noisy sandbox run showed a fluke regression would be overfitting to measurement noise,
not responding to a real signal. The `_before`/`_after` equality tests in `perf/src/lib.rs`
(`cargo test` from `perf/`, 3/3 passing) confirm every fix is behavior-preserving, independent of
the performance question.

## The bigger finding: LUT preview cost, in absolute terms

83.7ms (before) to apply one 3D LUT lookup pass to one Full HD frame is the more important number
here, independent of the idiom-level fix. At 30fps, a frame budget is ~33ms; at 60fps, ~16ms. A
user with a LUT applied to a clip and live preview at Full HD is **already well outside frame
budget on CPU alone**, before compositing, decoding, or anything else `pump_preview_frame` does
that same frame — matching `TIMELINE_PERFORMANCE.md`'s existing "known gap" style of finding
(confirmed-and-quantified, not hypothetical). The ~9-13% improvement from this pass's fix is real
but doesn't change that headline conclusion.

**Follow-up, same session: `Lut3D::sample` isolated and confirmed as the dominant cost, not
assumed.** `perf/benches/preview_effects_bench.rs`'s `bench_lut_sample` calls `Lut3D::sample`
alone (varying input per call so the optimizer can't hoist it into a constant): **38.1ns per
call**, measured directly. A Full HD frame is 2,073,600 pixels → `2,073,600 * 38.1ns ≈ 79.0ms` —
matching the measured whole-function cost (76-84ms across runs) almost exactly. This confirms,
with a real number rather than a guess, that `Lut3D::sample`'s trilinear interpolation (8 array
reads + 7 lerps per call) *is* essentially the entire cost of `apply_lut_to_rgba` — the
surrounding loop, byte↔float conversions, and clamping are negligible by comparison. Two real
follow-up directions, unattempted this pass since both are product/design decisions, not
mechanical fixes:
- A coarser preview-only LUT size — this LUT was parsed at 17³ (4,913 entries), a common real-world
  size; each `sample()` call touches 8 of those entries regardless of size, so the entry count
  itself isn't the lever — the *pixel count* is. This points toward the second option instead:
- Apply the LUT (and the other three effects) to a **downscaled preview frame**, not the full
  source resolution — the common real-NLE compromise (Resolve/Premiere both scale live-preview
  processing to viewport resolution, not source resolution). Halving each dimension alone would
  cut this cost to roughly a quarter (~20ms), likely enough to clear a 30fps budget on its own.

## Scopes: two full-frame passes merged into one

`App::pump_preview_frame` renders the waveform and vectorscope monitors together, every frame,
whenever `App::preview_state.scopes_enabled` — never just one or the other. Before this fix, that
meant two independent calls (`avcore::luma_waveform_rgba`, `avcore::vectorscope_rgba`), each doing
its own full iteration over every pixel of the decoded source frame. `avcore::render_scopes_rgba`
computes both scopes' accumulation in one shared pass instead, sharing the per-pixel index
arithmetic and RGBA byte reads between them.

**Correctness confirmed before performance**: `render_scopes_rgba`'s output for both scopes must
be byte-identical to calling the two original functions separately, on every input shape — not
just a solid-color test case, which could pass by coincidence. `crates/core/src/graphics/scopes/
scopes_test.rs` gained `render_scopes_matches_calling_both_standalone_functions` (a non-uniform,
non-square, non-power-of-two 37x23 test frame, checked against both `luma_waveform_rgba` and
`vectorscope_rgba` called independently) plus two edge-case tests (empty source; one output side
requested with the other zero-sized, confirming the two scopes are computed independently within
the shared pass, not accidentally coupled). Run for real via the same throwaway-scratch-crate
technique this repository already uses for a zero-heavy-dependency module (`scopes.rs` depends on
nothing but `std`): 9/9 passing, not just type-checked.

**Measured** (`perf/benches/scopes_bench.rs`, same 1920x1080 methodology, real 256x128 waveform /
128x128 vectorscope output sizes — `App::pump_preview_frame`'s own constants): **~9-11% faster**,
reproduced across two independent full benchmark runs (60.3ms → 53.5ms, then 59.2ms → 54.1ms) —
consistent direction both times, clearing this sandbox's own ~4-5% noise floor (established in the
`apply_vignette_to_rgba` control measurement earlier in this document) with room to spare, same
confidence level as the `apply_lut_to_rgba` fix. The improvement is more modest than merging two
loops into one might suggest because each scope still does its own real per-pixel math (`luma()`'s
weighted sum, `chroma_cb_cr()`'s two dot products) — only the shared iteration/indexing overhead
(bounds checks, the `(y * src_w + x) * 4` index computation, the RGBA byte reads themselves) is
actually saved, not the arithmetic.

`avcore::luma_waveform_rgba`/`avcore::vectorscope_rgba` themselves are unchanged and still public
— `render_scopes_rgba` is additive, not a replacement, so any caller that only ever needs one
scope (none currently exist, but nothing stops one existing later) isn't forced to compute both.

## Fixes applied to `crates/core`

All verified via `cargo check --workspace --all-targets` + `cargo clippy --workspace --all-targets`
(both clean — via the documented temporary `filters.c`/`text_overlay.c` shim, reverted before any
commit; see `CLAUDE.md`). `core`'s own test suite still can't *run* in this sandbox (the
`libonnxruntime` link gap `PLAN.md` describes) — same "type-checked, not run from this repo's own
test suite" caveat every other `core`-side change made from this sandbox carries; the *specific*
functions changed here are additionally verified for real via `perf/`'s own equality tests, which
is strictly more evidence than a typical sandboxed `core` change gets.

1. **`preview_effects.rs`** — `apply_lut_to_rgba`, `apply_glitch_to_rgba`,
   `apply_deflicker_to_rgba`: `chunks_exact`/`chunks_exact_mut` → `as_chunks`/`as_chunks_mut`.
   Also `rgba.len() % 4 != 0` → `!rgba.len().is_multiple_of(4)` in `apply_deflicker_to_rgba`
   (`clippy::manual_is_multiple_of`) — same computation, clippy's preferred spelling.
2. **`preview.rs`** — the live-preview compositor's per-pixel blend loop (`blend_branches`, called
   once per decoded frame whenever an overlay/blend-mode track is active): same `chunks_exact_mut`/
   `chunks_exact` → `as_chunks_mut`/`as_chunks` swap, zipping the base and overlay planes.
3. **`background_removal.rs`** — `resize_matte(&data.to_vec(), ...)` → `resize_matte(data, ...)`.
   `data` was already `&[f32]` (straight from `try_extract_tensor`); `.to_vec()` allocated and
   copied a full `model_w * model_h`-sized float buffer on every single AI-background-removal
   ONNX inference call for no reason — `resize_matte` only ever needed the slice.
   `clippy::unnecessary_to_owned`.
4. **`error_reporting.rs`** — `STATE_TRANSITIONS.iter().any(|known| *known == state)` →
   `STATE_TRANSITIONS.contains(&state)`. Not a hot path (breadcrumb validation, called rarely), but
   a genuine `clippy::perf`-flagged case (`manual_contains`) with zero risk — same result, no
   manual iteration where the standard slice method already does it.
5. **`privacy_blur.rs`** — two `std::iter::repeat(x).take(n)` → `std::iter::repeat_n(x, n)` calls
   in the CF-09 matte-padding function (`clippy::manual_repeat_n`) — `repeat_n` is the direct,
   already-bounded iterator `repeat().take()` builds indirectly via composition.
6. **`dynamic_reframe.rs`** — a `for c in &centers[..] { if let Some((x, y)) = c { ... } }` loop
   in the auto-reframe center-smoothing pass → `for (x, y) in centers[..].iter().flatten()`
   (`clippy::manual_flatten`) — removes a per-iteration branch the `Iterator::flatten` adapter
   already handles.
7. **`scopes.rs`** — new `render_scopes_rgba`, merging the two full-frame passes
   `App::pump_preview_frame` always ran back to back into one. Not a `clippy::perf` finding — found
   by inspection, not the lint sweep — see "Scopes: two full-frame passes merged into one" above
   for the measured ~9-11% improvement and how correctness was confirmed independently of the
   performance question.

## Not fixed this pass, and why

- **`crates/core/tests/*.rs` and `#[cfg(test)]` helper `chunks_exact` findings** (`preview_test.rs`,
  `overlay_render_test.rs`, `persistence_test.rs`, `collab_bundle_test.rs`, `multicam_sync.rs`'s
  test-only `burst_signal` helper) — these run once per test invocation, not per frame or per
  inference. Real but not worth the diff noise in the same pass as genuine hot-path fixes; a
  reasonable follow-up for a "clean up remaining clippy::perf noise" pass, not a performance one.
- **`clippy::unnecessary_cast`** — a same-type cast costs nothing at runtime (the compiler already
  elides it); this is a readability nit clippy happens to file under a lint name that sounds
  perf-related, not an actual perf finding, so it stayed out of scope for *this* pass's own
  fix-for-measured-impact criteria. Fixed anyway in the second follow-up pass (see the top of this
  document) once two production-code instances were touched for unrelated reasons — filed as a
  separate `chore:` commit, not folded into the perf work it happened alongside.
- **`clippy::ptr_arg`** (`&PathBuf` vs `&Path`, 2 occurrences) — `clippy::style`, not
  `clippy::perf`; a `&PathBuf` argument costs the same one word of stack space as `&Path` (both are
  thin pointers to the caller's existing allocation) — no allocation or copy either way. Out of
  this pass's own "obvious == perf-tagged" scope (see `PLAN.md`).

## Recommended, not attempted this pass (real, larger, needs a real dev machine)

Two items previously listed here turned out, on closer inspection during a later pass, to already
be handled or lower priority than first stated — corrected below rather than left stale (same
"confirm before assuming" standard the rest of this document holds itself to):

1. ~~Timeline panel viewport culling~~ — **already implemented**, confirmed by reading the current
   `timeline_panel/mod.rs`/`shape_overlays.rs`/`text_overlays.rs` source: a one-sided horizontal
   cull skips a clip's paint/interaction/thumbnail cost once it's scrolled past the visible right
   edge, in all three per-track loops (video, text, shape), landed via an unrelated concurrent
   refactor sometime between this report's first version and this correction. See
   `.claude/engineering/TIMELINE_PERFORMANCE.md`'s own updated section for the exact mechanism.
   **Still open**: no *vertical* (track-row) culling for a project with many tracks — not
   confirmed as a real cost, just unconfirmed either way (see that same doc).
2. ~~`preview.rs`'s per-track cloning~~ — **confirmed not a hot path**, not merely "not confirmed
   as a problem" as this report first said. `current_preview_overlay_clips`/
   `current_preview_audio_clips` (`preview_selection.rs`) are only reached from `App::
   ensure_preview_loaded`'s branch behind a cheap id-only fast path that returns early on the
   common no-op case (nothing at the playhead changed this frame — true for every frame of
   uninterrupted playback). The expensive clone only runs when the playhead actually crosses into
   a different clip set, inherently rare relative to frame rate. Still worth the same
   "clone only the field actually read" treatment eventually, just not urgent — downgraded from a
   recommendation to a known low-priority cleanup.
3. **`Lut3D::sample`'s absolute per-pixel cost** — see "The bigger finding" above. Now measured in
   isolation, see the updated numbers there; the remaining open question (preview-resolution LUT
   vs. downsampled preview frame) is still a real design decision, not a same-pass mechanical fix.

## How to reproduce

```bash
cd perf
cargo test              # 4/4 before/after equality checks
cargo bench              # full Criterion run, ~2 minutes
```

No `FFMPEG_DIR`/`PKG_CONFIG_PATH`/toolchain setup needed — `perf/` depends on nothing from this
repository's own workspace (see `PLAN.md`).
