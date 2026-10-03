# Bush — Preview Engine Replacement Plan

## Objective and boundary

Create **Bush**, a native, Rust-owned preview engine that can eventually replace the GStreamer
implementation. The first delivery is a standalone workspace crate: no `core`, `ui`, or
`apps/desktop` caller changes, no feature flag, and no removal of GStreamer. Its public API and
tests must be usable in isolation before an adapter is proposed.

The engine is for interactive preview only. Export remains on `avbridge`'s existing FFmpeg filter
and encode path; preview shortcuts must never change export output.

## Editor-first performance requirement

Bush is an NLE playback engine, not a generic media player or an export renderer. Performance for
editing is its critical product requirement: a user's newest scrub, trim, effect, or layer change
must take priority over obsolete decode work, and the editor must remain responsive while media
work happens elsewhere. Correct output delivered too late to support an edit is a failure.

The engine is therefore optimized for repeated non-linear seeks, paused-frame accuracy,
playhead-driven multi-source composition, live property changes, proxy/original switching, and
continuous A/V playback. Sequential full-file throughput is secondary. It must explicitly favor
preview latency over completing stale work, while preserving export-quality semantics in the
separate export pipeline.

Phase 0 establishes the release-build budgets from the current implementation and target editing
hardware/media classes. Each subsequent phase reports p50/p95/p99 for open-to-first-frame,
scrub-to-correct-frame, live-update-to-presented-frame, steady-playback frame time, memory high
water mark, dropped/obsolete work, and A/V drift. No implementation advances on an unmeasured
claim of optimization, and a regression needs either correction or an explicit approved budget
change.

## Current contract to preserve

`avcore::preview::Preview` currently provides:

- single-source opening, hardware-decode preference with one software retry, play/pause, rate
  seeking, duration/position, RGBA `VideoFrame`, and audio level;
- composited opening with independent source offsets and rates for background, video overlays,
  audio-only clips, text, and shapes;
- live text-overlay updates without recreating unchanged full-canvas buffers;
- a worker-owned pipeline with latest-value mailboxes, generation IDs, coalesced live updates,
  bounded result storage, and cooperative shutdown.

The replacement must treat these as behavior to characterize, not signatures to copy. The new
crate owns opaque engine types and data-only requests/responses. A later `core` adapter will map
the old API to the new one only after parity is demonstrated.

## Proposed ownership

```text
bush (new, standalone workspace member)
  engine/       lifecycle, state machine, commands, diagnostics
  demux_decode/ FFmpeg-backed demux/decode, seek index, HW/SW policy
  scheduler/    clock, frame selection, bounded queues, cancellation
  compose/      scene evaluation and GPU/CPU compositor boundary
  audio/        per-source decode, mix, resample, device sink
  text/         consumes already-shaped/rasterized RGBA overlays as input
  testkit/      deterministic clock, fixtures, frame/audio assertions
        |
        +-- avbridge extensions (small safe Rust wrappers over narrow C FFmpeg calls)
```

Bush must not depend on `core`, `ui`, egui, Tauri, or GStreamer. It may depend on
`avbridge`; `avbridge` remains the only crate allowed to own raw FFmpeg FFI. Timeline types are
not imported: `preview-engine` defines request DTOs containing paths, trims, transforms, effect
parameters, timing, and raster-overlay bytes. A future mapper in `core` is the only location that
knows both timeline data and engine DTOs.

## Competitive capability survey (2026-09-28)

This survey uses vendor/framework documentation as capability evidence, not comparable performance
proof. Bush should copy the editor-facing constraints below, not the competitors' APIs or their
product-specific timeline models.

| Engine/product | Documented capabilities | What Bush must take from it |
| --- | --- | --- |
| Adobe Mercury Playback Engine | GPU effects, image processing, color conversion, and scaling; GPU-assisted timeline playback, scrubbing, and full-screen output. | A GPU compositor and color-aware scaling are baseline editing capabilities, not a late visual enhancement. |
| DaVinci Resolve | Proxy/half-resolution playback, optimized media, Smart/User render cache, and background caching of expensive timeline work. | Cache derived frames by precise scene inputs; offer a quality ladder and schedule cache work below interactive seeks. |
| Final Cut Pro | Original/optimized/proxy media, proxy-first multicam playback, automatic optimized media for multicam, and background render only while idle. | Source variants and multicam must be first-class; background work must yield immediately to an edit. |
| MLT | Multi-track compositions, keyframes with easing, time/speed effects, preview scaling, hardware decode/encode, GPU processing, 10-bit, linear-color processing, audio mix, masking, and transitions. | Bush needs a data-only scene graph with arbitrary layers, time-aware effects, quality-aware preview, and a color/bit-depth strategy. |
| GStreamer Editing Services | Timeline tracks/layers, clips, effects, text overlays, transitions, snapping, and batched `commit` of edits; it restricts same-track overlaps. | Preserve immutable scene snapshots and batched edit commits, but do not inherit GES overlap restrictions: Bush must support the editor's arbitrary visible layers. |
| libopenshot | FFmpeg-backed multi-layer composition, broad media support, and experimental hardware encode/decode; its documentation warns hardware acceleration can be slower on some GPUs. | Hardware decode is an optional measured path with software fallback, never an assumed optimization. |

Primary sources: [Adobe Mercury](https://helpx.adobe.com/premiere/desktop/get-started/download-and-install/mercury-playback-engine-gpu-accelerated-in-premiere.html), [DaVinci Resolve cache/proxy guide](https://documents.blackmagicdesign.com/UserManuals/DaVinci-Resolve-17-Beginners-Guide.pdf?_v=1624593614000), [Final Cut Pro playback settings](https://support.apple.com/en-ie/guide/final-cut-pro/verb8e60ab7/mac), [MLT features](https://www.mltframework.org/features/), [GES timeline](https://gstreamer.freedesktop.org/documentation/gst-editing-services/gestimeline.html), and [libopenshot](https://github.com/OpenShot/libopenshot).

### Bush capability priorities

| Priority | Capability | Why it is critical for video editing |
| --- | --- | --- |
| P0 | Frame-accurate seek, paused preroll, generation-tagged cancellation, and bounded latest-intent queues | Scrubbing and rapid edits must show the newest correct frame rather than complete stale work. |
| P0 | Proxy/original/optimized source variants plus preview-resolution and effect-quality tiers | Editors need responsive timelines before all source media and effects can be played at final quality. |
| P0 | Multi-source clocking, transforms, alpha/layer ordering, audio mix, and live parameter updates | These are the minimum real-time timeline composition path, not post-MVP extras. |
| P1 | Versioned decoded-frame, GPU-upload, and render caches with byte bounds, invalidation, and hit/miss diagnostics | Effects-heavy or multicam edits require reuse without incorrect frames or unbounded memory. |
| P1 | GPU composition with explicit CPU reference path, color metadata, 8/10-bit policy, and hardware-decode fallback | Visual correctness and low latency must coexist across supported hardware. |
| P2 | Background pre-render/cache warming, multicam source policy, and adaptive quality selection | Useful only after they demonstrably do not compete with the active edit path. |
| Deferred | Generic third-party effect/plugin runtime and export unification | They enlarge the ABI and failure surface without improving Bush's first editor-preview contract. |

The priority table changes the roadmap's interpretation: no phase is complete merely because it
decodes or renders media. It must meet the applicable Bush editor-performance gate while handling
the corresponding scene/edit workload.

## Mose capability map for Bush

**Mose is the renamed Oca product.** This map is based on the implemented Mose model and feature
matrices, not a generic NLE checklist. `Execute` means Bush owns the real-time preview work;
`Consume` means another Mose subsystem prepares immutable data that Bush renders or plays;
`Share` means Bush exposes a bounded decode/sampling primitive but does not own the feature;
`Outside` means the feature must not migrate into Bush.

| Mose capability | Bush role | Performance design |
| --- | --- | --- |
| Imported media metadata, codec/fps/duration probe, YouTube download, project persistence, collaboration bundles | Outside | Bush receives validated immutable asset descriptors and resolved local paths only; no project I/O, download, or persistence on its real-time threads. |
| Editing proxies (360p/480p/720p) and original media | Consume + select | Choose a source variant per scene/quality tier; retain per-source decoder sessions and indexes; switch variants only at a generation boundary, never during a frame. |
| Thumbnails, filmstrips, auto-reframe, matte generation, motion tracking, scene-cut detection | Share | Provide a bounded, cancellable off-line frame-sampling service with lower priority than active preview; batch jobs cannot occupy preview decoder/GPU capacity. |
| Sequences, tabs, undo/redo, copy/paste, split/trim/drag, magnetic snap, markers, comments, layer templates, panel layout | Outside | `core` resolves an edit into one immutable `SceneSnapshot`; Bush never mutates timeline state or holds an undo snapshot. This keeps UI interaction and preview scheduling independently bounded. |
| Compound/nested sequences and composite blocks | Consume, then Execute | Phase 1 consumes the existing materialized nested-sequence asset so preview stays behavior-compatible. A later measured slice may evaluate nested `SceneSnapshot`s directly with cycle-free resolved inputs; do not add recursive live composition before its cache and memory budget are proven. |
| Multiple overlapping video, overlay, text, shape, and audio tracks | Execute | Evaluate only contributors visible/audible at the playhead; use a compact layer list sorted once per scene version, GPU texture pooling, and a bounded audio mixer. Hidden/muted tracks request no decode. |
| Trim, source in/out, freeze frame, speed factor/ramp, per-branch seek | Execute | Maintain source-time mapping and keyframe-aware seek indexes; cache frozen frames and make seek/decoder flush cancellable at every generation change. |
| Position, scale, rotation, anchor, crop, flip, opacity, blend mode, masks, chroma key, and keyframes | Execute | Evaluate keyframes into a compact per-frame parameter buffer and run transforms/compositing as GPU passes. Rebuild only when source/scene data changes, never when just the playhead advances. |
| Brightness, contrast, saturation, vignette, sharpen, pixelize, glitch, 3D LUT, and deflicker | Execute | Use color-managed shader passes and effect fusion where correctness allows; use bounded temporal history for deflicker. No full-canvas CPU copy per visible frame. |
| Shake, transitions (fade/slide/zoom), stabilization | Execute, phased | Shake/transitions are timeline shader operations. Stabilization needs motion-history/flow data and remains a separately measured temporal stage; it must degrade explicitly rather than blocking scrubbing. |
| Text, shapes, 43-font catalog, subtitle word highlighting, complex shaping/bidi | Consume raster/layout, Execute composition | Mose's text system owns shaping/layout and supplies versioned RGBA/vector overlay commands. Bush caches glyph/overlay textures by content/layout/style version and uploads only dirty regions; text changes must not reopen video decoders. |
| Background-removal matte, privacy masks, auto-reframe, motion-tracked position keyframes | Consume, Execute composition | AI/analysis remains outside Bush. Bush decodes matte streams and applies masks/transforms on GPU, holding the last valid analysis result only under an explicit staleness rule. |
| Whisper subtitles, SRT export, TTS | Consume for preview; Outside for generation/export | Bush renders timed subtitle overlays and mixes generated TTS audio as ordinary sources. Model inference, text generation, and SRT/export serialization never run in Bush. |
| Clip gain/keyframes, audio roles, multi-track audio mix, audio level meter, voice-cleanup preview | Execute | Real-time audio gets an independent bounded callback path with resample/mix/gain/DSP stages and no filesystem or GPU waits. Meter events are downsampled/coalesced for the UI. |
| Loudness measurement/normalization, audio noise reduction, final limiter, export queue | Outside | These are offline/export contracts. Bush may preview a declared effect only when its low-latency approximation is specified and labelled; it never changes export DSP output. |
| Multicam groups and angle switching | Execute after source selection | Pre-open or bounded-prefetch candidate angles around the playhead; angle changes select an already prepared source and must discard obsolete decode/upload work. Proxy-first policy is required for heavy multicam scenes. |
| Render/export, GPU encoding, scopes, waveform persistence | Outside, with optional Share for source frames | Keep final-quality rendering and encoding on `avbridge`. Bush may offer decoded frames to a low-priority scopes/waveform consumer only through a bounded copy/lease contract. |
| Local telemetry, structured errors, crash recovery | Execute diagnostics only | Emit redacted generation-tagged timings, cache statistics, queue pressure, fallback causes, and resource bounds. Recovery policy, persistence, and user messaging stay with Mose. |

### Bush scene boundary

`SceneSnapshot` is the performance and ownership boundary for all Execute/Consume entries. It
contains resolved source variants, visible/audible contributors, source-time mappings, evaluated
or evaluable keyframes, effect parameters, overlay/matte references, color/quality policy, and a
monotonic `scene_version`. It contains neither `Project`, `Timeline`, UI handles, mutable model
state, nor callbacks into Mose.

Every Mose mutation is classified before producing a new snapshot:

| Mutation class | Bush action |
| --- | --- |
| Playhead/transport only | Reuse scene/decoder/GPU resources and reschedule frame selection. |
| Live parameter or text/shape overlay update | Replace only the parameter/overlay version and affected GPU pass; preserve source decoders. |
| Clip trim, effect graph, visibility, layer, source variant, or audio-route change | Rebuild only affected contributor state and invalidate dependent caches. |
| Sequence/asset structural change | Submit a new scene generation; cancel the previous generation cooperatively. |

This boundary is the rule that lets Bush support Mose's broad editor surface at maximum performance
without absorbing unrelated product responsibilities or rebuilding the complete pipeline for a
small edit.

## Competitive benchmark program

Bush must be benchmarked against the editor experience offered by competing products, not only
against its own previous revision. Proprietary products do not expose comparable engine internals,
so this program has two complementary tracks:

1. **Bush white-box:** a headless runner records decode, scheduling, upload, composition, cache,
   audio, and memory telemetry with a monotonic clock and trace IDs.
2. **Competitor black-box:** a reproducible operator/UI-automation protocol records the same
   user-visible milestones in Premiere, Resolve, Final Cut Pro, and one open reference engine
   (MLT or libopenshot) where a supported licensed installation is available.

The comparison is a workflow benchmark, not a claim that internal CPU/GPU work is identical. A
result must identify the product/version, operating system, driver, CPU/GPU/RAM, storage, power
profile, display/preview resolution, media fixture hash, project recipe, cache state, proxy mode,
and every unavailable or non-equivalent feature. Results from different hardware, preview quality,
or cache state are separate rows and must never be averaged together.

### Canonical workloads

| ID | Workflow | Primary measures |
| --- | --- | --- |
| B01 | Open an H.264/HEVC long-GOP source and show an accurate paused frame | open-to-first-correct-frame p50/p95/p99; decoded-frame accuracy; peak memory |
| B02 | Burst of random timeline scrubs, retaining only the final seek | final-scrub-to-correct-frame p50/p95/p99; stale work dropped; seek accuracy |
| B03 | Continuous 1080p and 4K playback with one, then multiple visible layers | presented FPS, missed-frame/deadline rate, p95 frame time, CPU/GPU/RAM, A/V drift |
| B04 | Layered scene with transform, opacity, blend, crop, color/effect stack, text, and shape | live-edit-to-presented-frame latency; pixel/reference correctness; cache hit rate |
| B05 | Multicam playback and rapid angle changes, with original and proxy media | angle-switch latency; dropped frames; decode/upload pressure; memory |
| B06 | Reopen/reseek after cache warm-up, then after a defined scene mutation | cold/warm latency, invalidation correctness, disk footprint, cache reclamation |
| B07 | Device/GPU decoder failure during play and repeated open/close cycles | recovery time, software-fallback outcome, leaked handles/memory, error quality |

Each workload has a versioned fixture pack and declarative scene recipe. The Bush runner consumes
the recipe directly; competitor runs recreate only the documented equivalent timeline. Where a
competitor cannot reproduce a Bush feature exactly, mark the case `not comparable`, record the
reason, and exclude it from ranking rather than silently simplifying Bush.

### Measurement protocol and gates

- Run a warm-up iteration, then a predeclared number of measured repetitions; report the full
  sample count plus p50/p95/p99, min/max, and failures, not only an average.
- Capture cold-cache and warm-cache variants separately. Clear only the application cache named by
  the protocol; do not delete user data or rely on ambiguous OS cache state.
- Pin the power plan, disable background workloads, use local media on declared storage, and
  record all environment changes. Competitor runs use their documented proxy/preview mode; Bush
  runs name the corresponding quality tier.
- Verify correct frame PTS and reference pixels for Bush. For black-box competitor runs, record a
  visible frame marker or timecode and label the lower precision explicitly.
- Publish raw traces/CSV, fixture hashes, scripts, configuration, and a short methodology note;
  summary charts must link to the raw run.
- A Bush change cannot claim a competitive win without a paired run on the same machine and
  equivalent workload. It can claim an internal regression improvement from white-box evidence
  alone, clearly labeled as such.

Phase 0 delivers the fixture pack, recipe schema, headless Bush benchmark contract, and
black-box protocol before any performance target is fixed. Re-run B01-B07 after every architectural
milestone and before proposing `core` integration. The initial competitive baseline is evidence,
not a release gate: Bush's hard gates remain frame correctness, bounded resources, and the
editor-performance contract even if a competitor is unavailable on the test platform.

## Decisions to settle with spikes

| Decision | Candidate | Exit evidence |
| --- | --- | --- |
| Decode boundary | Extend `avbridge` with RAII decoder/session and decoded-frame APIs | Seek, decode, flush, EOF, cancellation, and malformed-media tests run without GStreamer symbols. |
| Video compositor | `wgpu` renderer with CPU RGBA fallback only for deterministic tests | 1080p multi-layer scene renders correctly and stays within agreed frame/seek budgets on release builds. |
| Audio output | `cpal` behind an `AudioSink` trait | Device loss, no-device mode, mixing, resampling, and shutdown are deterministic; tests use a fake sink. |
| Hardware decode | FFmpeg hwdevice/hwframes through `avbridge`, software fallback per session | At least one supported hardware path and forced software fallback are both proven on real media. |
| Effects parity | Reuse pure-Rust preview effects first; add only effects proven necessary to the interactive path | A supported-effect matrix names native preview, CPU fallback, or explicitly unsupported status. |

Do not choose a GPU, audio, or FFmpeg wrapper dependency merely from API familiarity. Each spike
records license, Windows/macOS/Linux support, DLL/bundling impact, cancellation behavior, and a
release-build measurement before the dependency becomes part of the engine.

## Engine contract

The initial API should be intentionally narrow:

```rust
Engine::new(config) -> Result<Engine, EngineError>
Engine::submit(Request { generation, scene, playhead, transport }) -> SubmitResult
Engine::poll() -> Option<Event>
Engine::shutdown() -> ShutdownHandle
```

`Request` replaces obsolete pending work for the same engine and generation; it does not enqueue
unbounded seek or live-update backlogs. `Event` is one of `Ready`, `Frame`, `AudioLevel`,
`EndOfStream`, `RecoverableError`, or `FatalError`, all tagged with `generation`. Frames carry
format, dimensions, stride, color metadata, presentation timestamp, and owned/shared pixel
storage. Every command is cancellable at decode, upload, and composition boundaries. The engine
thread owns decoder, GPU, and audio native handles; callers never hold native references.

Scene evaluation has three explicit versions: `source_version`, `scene_version`, and
`presentation_version`. Source/scene changes may reopen or rebuild; presentation-only changes
(seek, play/pause, viewport) must not. Cache keys include source identity, trim, decode format,
output size, color space, and every effect/overlay input that changes pixels. Frame, decoder,
upload, command, and diagnostic queues all have a configured upper bound and specified eviction
policy. Their default eviction rule is editor-first: retain only work that can satisfy the latest
playhead/scene generation and discard obsolete prefetch, decode, composition, or upload work.

## Roadmap

### 0. Contract inventory and benchmarks

Create `crates/bush` with no caller and no production decoder. Catalogue each current
`Preview` and `PreviewWorker` behavior in contract tests: input, output, threading, error,
cancellation, and fallback semantics. Capture a release baseline for single-clip open, keyframe
seek, steady playback, 1080p two-layer compositing, memory high-water mark, and audio drift.
Define the editor performance contract per target hardware/media class, including percentile
latency and frame-accuracy criteria for scrubbing, live updates, playback, and source/proxy
switching. Implement the competitive benchmark harness and collect the first B01-B07 baseline
against each available competitor before fixing Bush's quantitative targets.

**Exit:** the standalone crate compiles; benchmark fixtures, a machine/media manifest, and the
editor performance contract exist; the compatibility matrix labels each behavior `required`,
`deferred`, or `not preview`; competitive results distinguish paired comparable runs from
unavailable or non-equivalent cases.

### 1. FFmpeg decode-session spike

Add the smallest `avbridge` decode-session surface needed for open, stream selection, seek,
decode-next, flush, metadata, and release. Keep raw `AV*` values, callbacks, and allocation
ownership private to C/Rust safe wrappers. Deliver a standalone engine source that opens one video
and emits timestamped RGBA frames by polling, without audio or composition.

**Exit:** real fixtures prove frame dimensions, monotonic timestamps, key-unit seek tolerance,
EOF, cancellation, corrupt/unsupported input errors, and deterministic teardown. Fuzzed or
bounded hostile metadata cannot cause unchecked dimensions, integer conversion, or allocation.

### 2. Transport and worker model

Implement the engine-thread state machine, latest-value request replacement, generation filtering,
bounded event delivery, paused preroll, play/pause, variable rate, and cooperative shutdown. Use a
test clock; no wall-clock sleeps in tests. Retain frames only under an explicit frame-memory
budget, dropping superseded preview work rather than delaying a newer seek.

**Exit:** unit tests prove stale generations are ignored, queues stay bounded, cancellation works
mid-decode, and shutdown releases sessions. A real-media integration test proves seek-and-poll
and normal playback. Release measurements show that a newer seek supersedes prior work instead
of waiting behind it.

### 3. Audio path

Decode and resample audio independently, then add a mixer and `AudioSink` abstraction. Start with
one source and gain; add multiple contributors only after clock ownership and drift correction are
measured. No-device/headless mode remains a supported engine configuration.

**Exit:** fake-sink tests prove mix, gain, mute, resampling, underflow reporting, and shutdown;
real-device validation measures A/V drift during play, seek, rate change, and device failure.

### 4. Scene and composition vertical slice

Introduce data-only scene DTOs and render one background plus one transformed video overlay to an
RGBA frame. Establish a GPU compositor behind a small renderer interface and retain a deterministic
CPU reference renderer for pixel tests. Add alpha, crop, opacity, anchor, and blend modes before
adding broad effect support.

**Exit:** golden pixel tests cover geometry, alpha, blend modes, aspect ratio, color handling, and
layer order; a live 1080p two-layer run meets the editor performance contract without UI-thread
work.

### 5. Preview feature parity, deliberately sliced

Add audio-only clips, text/shape raster inputs, keyframed transforms, masks/mattes, live overlay
updates, effect classes, transitions, and hardware decode in individually benchmarked slices. Each
slice expands the compatibility matrix and has real-pipeline evidence. Unsupported effects must
be explicit engine errors/status, never silently omitted.

**Exit:** every GStreamer preview capability is either parity-proven, intentionally deferred with
a user-visible fallback plan, or confirmed irrelevant to preview.

### 6. Reliability, packaging, and observability

Harden malformed media handling, device loss/restart, GPU reset, decoder fallback, memory
pressure, and structured redacted diagnostics. Document dependency licenses, native DLL packaging,
platform feature matrix, and hardware test evidence.

**Exit:** bounded-failure tests and soak runs pass; all native handles are released; packaging can
ship the engine without GStreamer being needed by the engine itself.

### 7. Separate integration proposal

Only after phases 0–6, write a small adapter RFC for `core`: API mapping, feature flag, telemetry,
side-by-side comparison, rollback, and deletion criteria. This is a new approval point, not part
of the engine build plan. Do not call the engine from `core` before that approval.

**Exit:** a reviewed integration plan shows measurable parity and an immediate rollback path;
GStreamer remains untouched until a controlled migration begins.

## Implementation status

Phases 0-6 are implemented in `crates/bush` (plus the audio/hardware decode surface in
`crates/avbridge`) and verified against real media; phase 7 (the `core` adapter) is deliberately
not started and needs its own approval. The per-capability checklist, what is intentionally
missing (stabilization, audio time-stretch, zero-copy GPU presentation, non-Windows hardware decode),
the measured performance numbers and how to reproduce them live in
[../matrix/bush.md](../matrix/bush.md). Two measured results changed the plan: hardware decode
was slower than threaded software decode on every tested stream (so it stays opt-in), and the
CPU compositor needed exact fast paths plus banded threading to reach the 1080p budget.

## Validation and release gates

- Unit tests: state transitions, version/cache keys, bounds, scheduling, conversion validation,
  and redaction.
- Native integration tests: fixtures through real FFmpeg decode, seek, cancellation, malformed
  input, and hardware/software fallback where available.
- Pixel/golden tests: composition against the CPU reference renderer; include text, alpha, color,
  and exact layer order.
- Performance: release-build scenario measurements on declared hardware and media; compare to the
  phase-0 baseline and editor performance contract, never debug timings. Include p50/p95/p99
  scrub, live-update, and playback results rather than an aggregate average alone.
- Competitive benchmark: run B01-B07 with the recorded fixture, cache, quality, and environment
  protocol. Keep black-box editor measurements distinct from Bush's white-box telemetry.
- Soak: repeated open/seek/play/close, queue saturation, device/GPU interruption, and leak checks.
- Security: reject invalid sizes/counts/timestamps before allocation or FFI; no panic can cross C
  callbacks; diagnostics avoid full paths and media metadata by default.

## Explicit non-goals for this plan

- Wiring the engine into `core`, egui, Tauri, export, or automated UI tests.
- Deleting GStreamer dependencies or the existing `Preview` implementation.
- Reimplementing export filters or changing `.ocproj`/timeline schemas.
- Claiming pixel, hardware, or device parity without corresponding real-media evidence.

## Work order and commits

Implement in dependency order: `avbridge` decode surface, `bush` contracts/testkit,
decode spike, transport, audio, renderer, scene slices, hardening, then a separately approved
`core` adapter. Keep commits narrowly conventional, e.g. `feat(avbridge): add decoder session`,
`feat(bush): add bounded transport`, and `test(bush): cover stale seeks`.

## Evidence anchors

- Current GStreamer surface: `crates/core/src/preview.rs` and
  `crates/core/src/preview_worker.rs`.
- Current live-pipeline capability and verification caveat:
  `spec/matrix/preview-pipeline.md`.
- Existing FFmpeg ownership boundary: `crates/avbridge` and `spec/matrix/engine.md`.
- Existing bounded-worker and cache rules: `spec/RULES.md` and
  `spec/architecture/performance-and-caching.md`.
- Mose feature inventory used by the capability map: `spec/matrix/timeline-and-editing.md`,
  `spec/matrix/effects-and-color.md`, `spec/matrix/ai-features.md`,
  `spec/matrix/performance.md`, `spec/matrix/robustness.md`, and
  `.claude/domain/VIDEO_EDITOR.md`.
