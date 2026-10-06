# Repository code review: quality, organization, and resource use

Date: 2026-10-06. Reviewed revision: `c17e53b0`.

## Scope and evidence

This is a static review across Bush, avbridge, core, the Tauri backend, React, ytbridge,
and packaging. It follows the owning state and data flow through critical paths; it is
not a claim that every source line or execution path was individually validated.
An inventory of production Rust/C/TypeScript/Python/shell files in these directories
contains 372 files and 73,946 lines, excluding tests, benches, and dependency directories.
Large generated/data tables were not treated as orchestration code.

Findings below are supported by source contracts, allocation sizes, or complexity arguments.
No runtime CPU percentage, peak RSS, frame latency, or before/after speedup was measured.
No application tests or native pipeline benchmarks were run for this review. An earlier
frontend dependency installation was stopped when this task changed to a repository review;
its results are not validation evidence. Only this report was added to the workspace during
the initial review.

The existing architecture review and performance matrix were consulted. Historical claims
in those documents are not evidence that the current checkout passes those checks.

## Highest-priority corrections

### F01 — P1: Bound the source length copied by the loudness log callback

Location: `crates/avbridge/csrc/loudness.c:46-56`.

`av_log_format_line2` returns the length that would have been written, even when its
1,024-byte destination truncates the message. The callback bounds `memcpy` only by the
remaining space in the 4,096-byte capture destination. A log message longer than 1,023
characters therefore permits reading beyond the local `line` array. This can crash or
copy stack contents into the report.

The contract was verified in the installed FFmpeg header,
`/opt/homebrew/include/libavutil/log.h:377-387`. Clamp the copy to both the actual local
buffer contents and destination capacity. Add a long-message regression under ASan.

### F02 — P1: Synchronize log capture with every callback invocation

Locations: `crates/avbridge/csrc/loudness.c:34-56`, `:213-240`;
`crates/avbridge/src/loudness.rs:58-91`.

The Rust mutex serializes loudness measurements, but the installed FFmpeg callback is
process-global. Concurrent decoding, probing, or exporting can invoke that callback
without acquiring the Rust mutex. Those invocations race on the output pointer, length,
capacity, and prefix flag; cleanup also clears them without callback synchronization.
Logs from unrelated operations can fill or contaminate the captured JSON.

Use a capture mechanism with synchronized lifetime and callback writes, restrict capture
to the intended filter/context, and preserve normal logging for unrelated operations.
Verify parallel import/export/measurement, including callback teardown, with race detection
where supported. Serializing measurements alone does not establish callback safety.

### F03 — P1: Process analysis frames incrementally with a byte budget

Locations: `apps/desktop/src-tauri/src/commands/privacy_blur.rs:148-193`,
`background_removal.rs:137-180`, `scene_detection.rs:113-133`,
`motion_tracking.rs:146-185`; `crates/core/src/av/frame_sampler.rs:64-94`.

Analysis decodes full-resolution frames and collects them before processing. Sample-count
caps do not provide a practical RAM limit because resolution multiplies each sample:

| Retained samples | Pixel representation | Approximate retained data at 3840x2160 |
| --- | --- | ---: |
| Privacy blur: 400 | RGBA8 | 12.36 GiB |
| Scene detection: 600 | Gray8 | 4.63 GiB |
| Background removal: 40 | RGBA8 | 1.24 GiB |

These are arithmetic estimates for sample data alone. Privacy blur additionally creates
grayscale frames and matte data while retaining RGBA frames. Its RGBA samples alone can
also reach 3.09 GiB at 1920x1080. Independent feature guards allow different analyses to
run together, increasing the combined peak.

Stream scene-cut detection and segmentation. Make tracking retain only its required
history, downscale analysis inputs to a specified working resolution, and bound retained
bytes and concurrent analysis work. Preserve coordinate mapping back to source resolution.

### F04 — P1: Correlate background results with project, sequence, and input revision

Locations: `apps/desktop/src-tauri/src/commands/background_removal.rs:103-112`,
`privacy_blur.rs:110-117`, `motion_tracking.rs:115-124`,
`auto_reframe.rs:130-140`, `scene_detection.rs:96-100`, `transcribe.rs:125-132`.

Start an analysis, then open another project or switch sequences before it finishes.
Results are applied to the currently active timeline by clip ID, or transcription is
inserted into the currently active timeline. IDs can be reused in another project/sequence.
The result can modify unrelated content; transcript sidecars also use the project current
at completion. Imports already have a project-epoch guard, demonstrating an existing seam
to reuse.

Capture project epoch, sequence ID, source identity, and the relevant input revision in
one job token. Validate them before applying results and before writing sidecars. Cancel
obsolete work where possible, and apply successful user edits through an undoable mutation.

### F05 — P1: Bound aggregate import and media-request work

Locations: `apps/desktop/src-tauri/src/commands/media.rs:83-97`, `:129-148`;
`preview/protocol.rs:96-133`; `preview/tile_workers.rs:49-52`, `:76-86`.

Importing N files creates N threads and concurrent decode pipelines. Each file may perform
waveform, loudness, and proxy work; waiting on the loudness mutex leaves the thread alive.
Large imports compete with playback for CPU, decoder memory, and disk bandwidth.

Thumbnail requests have four decoder workers, but the dispatcher creates a thread per
request and queues jobs without a capacity or queue deadline. Waveform requests for different
files similarly start independent full decodes. The frontend limits its frame requests,
but the backend has no aggregate bound across request types and producers.

Introduce bounded queues and resource-aware admission shared across imports and media
extraction. Coalesce repeated keys, prioritize visible/current work, and discard queued
jobs made obsolete by a project or viewport change. Choose capacity through workload measurements.

### F06 — P1: Release and bound waveform and thumbnail caches

Locations: `apps/desktop/src-tauri/src/preview/protocol.rs:50-54`, `:209-247`;
`apps/desktop/src/editor/timeline/timelineMedia.ts:38-66`, `:209-249`;
`crates/core/src/av/waveform.rs:53-77`.

Backend `waves` and poster-frame `entries` have no eviction policy. Frontend detailed and
overview waveforms also remain in global Maps. `syncAssets` notices a changed source for an
existing ID, but does not prune removed IDs or reliably clear a switch with non-colliding IDs.
Closing or replacing a project can therefore leave its media retained until application exit.

One maximum-resolution detailed waveform holds up to 12,000,000 bytes in the backend.
The frontend pyramid adds roughly 1/7 to the original pair array. Both processes can retain
these per file, excluding temporary copies during transport. Individual bucket limits do
not bound the sum over files.

Use a byte-budgeted LRU with project/source versions, prune on asset removal/project replacement,
and bound concurrent waveform fetches. Keep successful visible entries instead of clearing
the entire tile cache when its entry limit is reached.

### F07 — P1: Move nested rendering outside the authoritative session lock

Location: `apps/desktop/src-tauri/src/commands/export.rs:46-71`.

Starting an export with an uncached compound sequence calls its synchronous render while
holding the session mutex and nested cache mutex. An encode that takes seconds or minutes
blocks edits, session reads, and autosave access. Export processing being threaded later
does not make this preparation path asynchronous.

Capture an immutable export snapshot under a short lock, prepare/materialize it on a bounded
worker, and enqueue the resulting export. Define cancellation and stale-request behavior.

### F08 — P1: Include transitive inputs in the nested-sequence cache key

Locations: `crates/core/src/model/nested_sequence.rs:137-144`, `:150-172`;
`apps/desktop/src-tauri/src/session/state.rs:76-82`, `:220`.

A cache hit checks only a nested sequence's timeline, before recursing into its descendants.
Editing a child sequence referenced by that timeline, relinking its media, or changing export
settings can leave the parent's timeline unchanged and reuse an outdated rendered file.
The AppState cache also survives project replacement; sequence IDs alone are not globally unique.

Key or invalidate by project epoch, sequence/content revision, transitive sequence/media inputs,
render settings, and output availability. Add regressions for descendant edits, media relinking,
and switching projects with reused IDs. Avoid solving staleness by re-rendering every edit.

### F09 — P1: Validate collaboration bundles before replacing a destination project

Location: `crates/core/src/project_io/collab_bundle.rs:147-158`.

The public import API reads a ZIP project entry without a bound and overwrites the destination
before parsing the project. An invalid bundle can destroy an existing destination while returning
an error. A large entry can exhaust RAM before persistence's decompression limit is checked.
Proxy extraction also has no total extracted-byte budget.

Bound archive entry/total sizes, parse and validate the project first, extract into staging,
then atomically commit the validated project. Preserve the existing file on every failure.
This API is currently exported by core; no Tauri command calling it was found in this review.

## Performance and maintainability corrections

### F10 — P2: Coalesce preview content and frames before enqueueing

Locations: `apps/desktop/src-tauri/src/preview/preview_engine.rs:108`, `:169-182`, `:210`;
`preview/bush_adapter/backend.rs:273-310`, `:538-550`.

Both preview command channels and the JPEG-frame channel are unbounded. Consumers drain
commands/frames after receiving them, but that does not limit producer-side retention while
a consumer is rendering nested content or encoding. Content updates retain distinct full
project snapshots until consumed; frames retain full pixel buffers.

Use replaceable latest-content/latest-frame slots and a wake notification. Preserve transport
ordering deliberately; aggregate step operations rather than silently dropping them.

### F11 — P2: Reduce full-project cloning and comparisons inside edit locks

Locations: `apps/desktop/src-tauri/src/session/state.rs:288-328`;
`session/session_sync.rs:92-111`; `commands/mod.rs:62-72`;
`preview/bush_adapter/backend.rs:146-183`.

Each published edit clones the complete project, diffs the previous project, and hands the
snapshot to a preview comparison that also walks media/tracks. Undoable operations clone
the active sequence before discovering whether an edit changes anything. The initial snapshot
serializes waveforms into a JSON tree and then strips them, so payload savings do not avoid
the temporary serialization/allocation cost.

Editing a small property costs work proportional to total project data, while the session
lock is held through publication. Separate mutation revisions for model, preview, and UI
metadata; borrow a waveform-free serialization view; reuse mutation knowledge for patches.
Measure before choosing persistent model containers or operation-based undo.

### F12 — P2: Avoid project-wide recomputation at preview-tick frequency

Locations: `apps/desktop/src/editor/PreviewPanel.tsx:73-136`;
`editor/timeline/Timeline.tsx:104-165`, `:318-326`.

PreviewPanel subscribes to the complete preview tick and recalculates timeline duration and
selection using allocated arrays on each render. Timeline repaints on every tick and scans
every clip in each visible track before horizontal culling; visible clips then look up media
with a linear scan. Work is O(total clips + visible clips * media count) per paint, even when
only the playhead changed.

Memoize derived model data by immutable timeline/media identity, use ID indexes, and render
the playhead separately from static clip content. Add a visible-interval index where large
timelines justify it. Measure repaint cost and allocations at 60 Hz on large projects.

### F13 — P2: Cache GPU LUT buffers and measure synchronous readback

Locations: `crates/bush/src/gpu_renderer.rs:455-474`, `:539-561`.

Every rendered layer with a LUT rebuilds its packed table and allocates/uploads a GPU buffer
even when the LUT is unchanged. A 33-cube table packs 574,992 bytes per upload; a 65-cube packs
4,394,000 bytes. At 60 frames/s that is approximately 34.5 or 263.6 MB/s per layer of repeated
host packing/upload data, excluding other uploads. These are calculated traffic estimates.

Cache LUT buffers by stable content/version with a GPU-memory budget. Separately profile the
synchronous full-canvas readback and JPEG path before considering pipelined readback or native
texture presentation; that larger change has platform and synchronization costs.

### F14 — P2: Preserve effect configuration changes and prune temporal state

Locations: `crates/bush/src/engine.rs:584-608`, `:705-709`.

Stabilizer and deflicker instances are keyed only by contributor ID and created with
`or_insert_with`. Changing a nonzero smoothing value keeps the old instance/configuration.
Entries are removed only when that same contributor is visited without the effect; deleting
or hiding it leaves its temporal state retained for the engine lifetime.

Include source/configuration identity in effect state and update or recreate it when those
inputs change. Prune removed contributors and define whether inactive contributors retain
history. Verify live parameter edits and scene replacement without restarting the engine.

### F15 — P2: Budget undo history by retained bytes, not clip count alone

Location: `crates/core/src/model/undo.rs:27-41`, `:70-77`.

The history budget counts clips but ignores text, paths, vertices, keyframes, markers, and
track payloads. A few clips with large dynamic fields can retain many expensive snapshots
without approaching the clip limit. `push` also walks all retained snapshots to count clips
and shifts the Vec for each oldest eviction.

Maintain an incremental estimated-byte budget, including dynamic fields, with explicit
oversized-step behavior. Use deque eviction or an equivalent non-shifting container.
Preserve undo/redo semantics and measure edit latency before adopting a larger undo redesign.

### F16 — P2: Index overlay segments during sequential multi-track export

Location: `crates/avbridge/csrc/timeline_export_multi.c:811-825`.

For each base video frame, every overlay track searches its segment list from index zero.
The search cost grows as O(frames * total overlay segments), even though export progresses
through timeline time in order.

Maintain monotonic cursors or an interval index while preserving overlap-selection semantics.
Compare frame counts and pixel output as well as CPU time on dense overlay timelines.

## Organization opportunities

- `editor/timeline/Timeline.tsx` is 931 lines, `editor/PreviewPanel.tsx` 653, and
  `commands/timeline.rs` 883. Extract the timeline interaction state into focused hooks,
  separate menu construction from painting, and separate preview transforms from transport.
  Split Rust commands by editing responsibility. Keep stable ownership boundaries; file size
  alone is not a defect and public barrel re-exports should not conceal cycles.
- Media jobs repeat sampling, feature guards, thread creation, and result application. A
  shared bounded scheduler and job token are justified by F03-F05; inference and per-feature
  algorithms should remain separate rather than becoming one generic feature manager.
- `cx.ts:5-17` resolves arbitrary class tokens dynamically and creates several arrays/Sets for
  every call, including high-frequency PreviewPanel renders. Direct CSS Module references
  would make missing class names detectable with generated declarations and reduce this
  work. Measure its contribution before presenting it as a material CPU bottleneck.
- `@components/*` currently names `src/editor/*`, which includes hooks and painting utilities
  while topbar components live elsewhere. Prefer domain aliases or explicit component entry
  points. An alias is a resolver convention, not an architectural boundary.
- The architecture review dated 2026-10-05 contains both failing-test caveats and an unqualified
  all-tests-pass claim. Several module docs still describe removed egui/GStreamer paths.
  Keep one dated validation record and update current architecture guidance; preserve older
  results as explicitly historical evidence.
- `perf/` duplicates core implementation for standalone benchmarking. Track the mirrored
  revision or share the pure computation source so benchmarks cannot silently measure obsolete
  algorithms. Do not refactor library boundaries just to reduce file counts.

## Proposed execution order and measurement plan

1. Correct F01/F02, F04, F08/F09 first: memory safety, stale results, stale renders, and safe writes.
   Add long-log, concurrent callback, project-switch, nested-dependency, and failed-import regressions.
2. Correct F03/F05/F06/F07/F10: streaming, bounded work/caches, short lock ownership, and cancellation.
   Add capacity/cancellation tests and verify retained memory after projects are closed.
3. Measure F11-F13/F15/F16 on identical inputs, then optimize one cost model at a time.
4. Extract UI/backend responsibilities alongside those changes, with behavior checks at each boundary.

Use production frontend builds and release Rust binaries. Record OS, CPU, RAM, decoder/renderer,
codec/resolution, frame rate, sample count, and p50/p95/max latency. Compare:

| Workload | Evidence to record |
| --- | --- |
| 1/20/100 media imports while playing | active workers, queue length, RSS, CPU, disk, preview frame latency |
| 1080p/4K privacy blur and scene detection | peak RSS, retained sample bytes, total duration, cancellation latency |
| 3,000/10,000 clips with slider edits | mutation/clone/diff/emit time, lock duration, webview allocations and repaint time |
| Dense nested/overlay export | preparation latency, session responsiveness, cache hits, frame/output correctness |
| GPU preview with 0/1/4 LUT layers | LUT allocations/uploads, GPU readback time, JPEG time, dropped/replaced frames |
| Repeated project open/close and scroll | Rust/webview retained memory, cache eviction, pending obsolete requests |
| Undo with dense keyframes and text | history bytes, edit/undo latency, retained RSS |

Existing Bush single-slot requests, byte-limited frame cache, bounded audio queues, export
snapshotting, atomic ordinary project saves, and verified packaging model digests are useful
primitives to preserve. The highest-value next work is resource ownership and correctness,
followed by measured hot-path optimization; no quantified speedup is claimed by this review.

## Follow-up: PR #202 merged on 2026-10-06

PR head: `be564d4de1accfc1a6167309cb342b88e3e1ac06`.
Merge commit on `main`: `a8e2347255dbc3c75f6bc37f3c53e05f917ad0fd`.
The remote confirmed that `main` points at the merge commit. The GitHub CLI account could
not access the repository API, so the PR's UI state and CI checks were not independently
queried. The merge was performed and pushed through the authorized SSH remote.

The effective merge changes 14 files. It adds a debounced frontend request for the displayed
preview size in device pixels, clamps the backend's longest side to 480-1280, and preserves
that request across worker restarts. Timeline JPEG tiles shrink from 320 to 192 pixels on
their longest side; library posters remain at 320. Downscaling changes from one nearest
sample to four averaged samples. The only merge conflict was the moved `Icon` import in
`PreviewPanel`; the current `./chrome/Icon` path was preserved.

| Finding | Effect of PR #202 | Remaining issue |
| --- | --- | --- |
| F03: analysis frame retention | Unchanged | Analysis still samples and retains full-resolution frames; preview sizing does not bound this RAM use. |
| F05: aggregate media work | Smaller output tiles | Request threads and the tile job queue remain unbounded; full-resolution sampling still occurs before JPEG downscaling. |
| F06: waveform/poster caches | Smaller raster dimensions for timeline tiles | Waveform and poster maps retain their existing lifetime and lack of a byte budget; poster dimensions remain unchanged. Compressed JPEG size savings are not measured. |
| F10: preview queues | Less data per output frame when a smaller canvas is requested | Content and encoder channels remain unbounded; obsolete snapshots are not coalesced. |
| F12: frontend scans | Unchanged | Full clip/media scans still run during preview updates. |
| F13: LUT uploads/readback | Smaller canvas can reduce output readback bytes and pixel work | LUT buffers are still packed and created per frame; synchronous readback remains. Source uploads and source-space effects need separate measurement. |
| F01/F02/F04/F07/F08/F09/F11/F14/F15/F16 | Unchanged | Their implementation paths are not corrected by this merge. |

None of the 16 findings is fully resolved by this PR. Reducing a 1280x720 output canvas to
640x360 cuts its output pixel count and tightly packed RGBA storage by 75%; reducing a
same-aspect tile's longest side from 320 to 192 cuts its pixel count by approximately 64%,
subject to integer rounding. These are size calculations, not measured CPU, RSS, JPEG size,
or latency improvements. Four-tap downscaling also adds sampling work per output pixel.

Organization follow-up: the PR introduces `src/previewResolution.test.ts` alongside source;
it should follow the existing separated frontend test directory in a later cleanup.

The merged diff passed `git diff --cached --check`, and the AST graph was refreshed.
No application tests, builds, or runtime benchmarks were run for this comparison.

## Correction batch 1: loudness capture

Branch: `codex/code-review-fixes`, based on merged `main` at `a8e23472`.

- F01 implemented: clamp the formatted source length to the local buffer's actual capacity
  as well as the destination's remaining capacity. Preserve NUL termination and avoid reading
  the output when the caller provides zero capacity.
- F02 implemented: protect capture setup, callback writes, prefix state, and detachment with
  a statically initialized native mutex (POSIX mutex on Unix, SRW lock on Windows). Match the
  exact loudnorm filter context of the active graph; forward unrelated messages to FFmpeg's
  default callback outside the capture lock. Teardown waits for active buffer writes before
  returning ownership of the output to Rust.
- Retain the existing measurement serialization and global callback contract. Concurrent
  C measurements and independent replacement of the FFmpeg callback during measurement remain
  unsupported. No process-wide serialization of decoding/exporting was added.
- Update the C/Rust API documentation to distinguish measurement serialization from callback
  buffer synchronization and to identify graph teardown as the report-emission stage.

Validation on macOS arm64: `FFMPEG_DIR=/opt/homebrew cargo check -p avbridge`,
`clang -fsyntax-only -Wall -Wextra -Werror -I /opt/homebrew/include
crates/avbridge/csrc/loudness.c`, Rust formatting check, and `git diff --check` passed.
The AST graph was refreshed. Runtime regressions, ASan, race detection, and Windows compilation
have not been run; these fixes have compilation evidence rather than runtime validation.
The remaining findings are still pending.

## Correction batch 2: collaboration bundle import

Initial corrections were committed separately: `93f437da` (avbridge) and `289319f7` (review).

F09 implementation:

- Bound the ZIP project entry both by advertised size and actual bytes read. Derive its encoded
  ceiling from the existing 256 MiB persistence payload limit and LZ4's maximum encoded size,
  keeping the format's header allowance in the persistence module.
- Deserialize the project and reject a missing active sequence before touching destination files.
- Stage all proxy files in a fresh temporary directory on the destination filesystem. Preserve
  basename sanitization, reject duplicate staged filenames, and verify reads/checksums before
  publishing. Unknown entries are inspected without opening an unnecessary decoder.
- Provide explicit import budgets, with defaults of 10,000 archive entries and 64 GiB of total
  extracted proxies. These are safety policies rather than measured optimal values; callers can
  select smaller budgets or explicitly permit larger valid handoffs. Actual streamed byte counts
  enforce the disk budget independently of advertised ZIP sizes.
- Flush staged files, retain the previous proxy cache as a backup, and publish the project with
  an atomic tempfile replacement only after the new proxy cache is installed. Restore the cache
  if publication fails; retain recovery files and report their location if restoration also fails.
- Successful imports replace the cache with bundled proxies, so unrelated old proxy files cannot
  silently satisfy the imported project's lookup. Concurrent imports to one destination must be
  serialized by the caller.

Limitations: project/cache publication spans multiple filesystem operations and is not a single
crash-atomic transaction. The ZIP library still parses its central directory before the entry
count is checked; this batch bounds project reading and extraction, not every internal ZIP parser
allocation. Runtime fault-injection and Windows validation remain pending.

Validation: `FFMPEG_DIR=/opt/homebrew CARGO_BUILD_JOBS=1 cargo check -p core --offline`,
Rust formatting checks for the changed project I/O modules, and `git diff --check` passed.
The first compilation attempt failed while spawning a native compiler with
`Resource temporarily unavailable`; the single-job retry completed successfully.
The AST graph was refreshed. No tests or performance benchmarks were added or run.

## Correction batch 3: nested render cache inputs

F08 invalidation changes:

- Traverse descendants and check cycles before accepting a cache hit. Record descendant render
  generations so a grandchild edit or re-render invalidates its ancestors even when their own
  timeline text has not changed.
- Include the sequence's export settings and referenced media's render fields (source path,
  resolution, frame rate, bitrate, audio presence). The key excludes waveforms and display data.
- Record size/mtime revisions of referenced source files and active LUT/mask files. Missing or
  unreadable revisions prevent reuse. Also verify the output file's revision before reusing it.
- Partition entries by project ID, project path, and cache directory; remove entries from other
  project namespaces and deleted sequences before traversal. Equivalent input data with the same
  namespace remains reusable rather than relying on sequence ID alone.
- Apply the child sequence's export aspect ratio to its render canvas, matching ordinary export
  preparation. Nested render normalization already used its own target LUFS.
- Rebuild desktop preview when export settings of a referenced nested sequence change. Preserve
  the existing behavior of ignoring export-only settings for an unreferenced active sequence.

Remaining limits: size/mtime revisions are not content hashes and require another materialization
call to observe filesystem changes; this does not introduce a file watcher. Nested output names
are still shared between preview and export caches, so concurrent writers and the lifetime of
files consumed by queued exports need a separate artifact-ownership correction. Session-lock
duration in F07 is unchanged. Runtime cache/cycle/aspect-ratio regressions remain pending.

Validation: `FFMPEG_DIR=/opt/homebrew CARGO_BUILD_JOBS=1 cargo check -p desktop --offline`
passed, including core and the desktop preview integration. Rust formatting checks and
`git diff --check` passed, and the AST graph was refreshed. No tests or benchmarks were run.

## Correction batch 4: export preparation

F07 lock-duration and dispatch changes:

- Capture a project snapshot under the session lock, then perform nested rendering and export
  preparation on a named native worker. The asynchronous Tauri command awaits a oneshot reply;
  FFmpeg work no longer runs while holding the session mutex or on the command executor.
- Admit one preparation per application before cloning or spawning. The shared nested-render
  cache already serializes this work; explicit admission avoids accumulating waiting workers
  and project snapshots. Translate the busy error in both frontend languages. This is an
  admission policy, not a benchmark-derived optimal worker count.
- Release admission through an ownership guard on success, errors, failed thread creation, and
  worker unwind. Release it before delivering the result so the next request can proceed.
- Preserve the project captured at request time even if the user edits or switches projects
  during preparation. Protect both its source/proxy paths and those of the currently open
  project when checking the output. Copy current paths under the session lock and perform
  filesystem canonicalization outside it. Reject destinations in either nested-render cache.
- Include preparation in the close guard before an export job has entered the queue. Adapt
  existing Rust test callers to await the command without introducing new test cases.

Remaining limits: preparation has no cancellation control; dropping the IPC reply does not stop
the worker or prevent the requested job from entering the queue. The snapshot still clones the
complete project, including waveform data (F11), and retains it during preparation. Shared
nested-render artifact ownership (F08) and broader output-path reservation races remain pending.
No runtime responsiveness, CPU, RAM, or throughput improvement has been measured.

Validation: `FFMPEG_DIR=/opt/homebrew CARGO_BUILD_JOBS=1 cargo check -p desktop --all-targets
--offline`, formatting checks for the five changed Rust files, and `git diff --check` passed.
This compiles existing test targets without executing them. The frontend error mapping and both
locale entries were inspected; frontend type checking was unavailable because the local
`node_modules/.bin/tsc` executable is absent. The AST graph was refreshed. No tests or benchmarks
were added or run.

## Correction batch 5: remaining implementation findings

The changes below complete the implementation pass for F01–F16. Earlier batches describe
historical intermediate states; their remaining implementation limits are superseded below.
Runtime acceptance and measurements listed at the end are separate from implementation status.

| Finding | Implemented correction |
| --- | --- |
| F03 | Pack sampled analysis frames directly into bounded working planes. Scene detection retains adjacent grayscale frames; segmentation processes samples immediately; privacy and motion retain grayscale working data. Reframe sampling requests its working resolution directly. |
| F04 | Admit one analysis across features with an ownership guard. Capture project epoch, sequence, source revision/path, clip, resolution, settings and output directory together; validate again before applying an undoable result. Release admission on errors and panic. |
| F05 | Share bounded interactive/background worker pools across protocol and import work. Bound tile jobs, reject overload, expire queued tile requests, and stop obsolete project jobs between stages. |
| F06 | Bound backend tiles/posters/waveforms by bytes and entries, promote successful hits, prune project/source changes, and reject stale insertions. Bound frontend image/waveform caches and concurrent waveform loads; clear on project epoch and source changes. |
| F07 | Cancel export preparation from the frontend or dropped command future, propagate cancellation through nested rendering, and avoid enqueueing cancelled preparation. |
| F08 | Give each nested render a unique owned temporary artifact. Queued exports own persistent linked/copied inputs independent of cache eviction; clean them after terminal queue state is persisted. |
| F10 | Coalesce content and dimension changes before enqueueing; aggregate adjacent steps; bound transport admission and retain only the latest pending encoded frame. |
| F11 | Borrow metadata for IPC serialization, omit waveform arrays from render/export snapshots, and move diffing, serialization and preview dispatch to a publication worker with one pending snapshot. Patch bases refer to the last emitted revision. |
| F12 | Memoize preview derivations and asset lookup, index immutable clip intervals, paint only visible clips, and repaint the playhead on a separate canvas. Preserve drag ghosts and overlay trim feedback. |
| F13 | Cache immutable LUT versions in a 32 MiB GPU cache. Expose cumulative LUT upload/hit and synchronous readback wait/copy counters; print them in the existing GPU benchmark. |
| F14 | Update temporal smoothing on reused effects, reset history on changed source/configuration, and prune inactive contributor state. |
| F15 | Use deques and incremental shared undo/redo byte accounting, including serialized dynamic payloads. Retain a single oversized newest snapshot explicitly. |
| F16 | Advance overlay search cursors for sequential export, preserve original overlap priority, handle unsorted segments, and reset on backward time. |

### Resource policies and practical limits

- Worker counts and budgets are safety policies, not measured optimal settings: two interactive
  and two background media workers, 64 pending jobs per class; four tile decoder workers with
  64 queued jobs, five-second queue expiry and 800 ms sampling deadline. Native decoders may
  still start their own threads. Overloaded protocol requests return 429 with a retry hint.
- Analysis uses a longest side of 320 pixels for scene/motion/privacy/matte working data and
  960 pixels for reframe. Native decoder allocations remain at source resolution. Privacy
  retains at most 400 grayscale samples plus masks (about 81.92 MB at square 320 pixels),
  motion at most 60 grayscale samples (6.14 MB), and matte at most 40 masks (4.10 MB), excluding
  native decoder/model/allocator memory. These are calculated payload bounds, not measured RSS.
  Downsampling's accuracy on representative real footage still needs runtime verification.
- Cache budgets cover payloads, not all allocator/browser overhead: backend tiles 32 MiB,
  posters 16 MiB, waveforms 64 MiB; frontend decoded frames 32 MiB and waveforms 64 MiB.
  Entry caps also bound negative entries. Size/mtime revision checks are not content hashes or
  file watchers; frontend invalidation follows project/source updates.
- Undo estimates a serialized payload with a conservative multiplier, not exact allocations or
  vector capacity. The default estimated budget is 100 MiB across undo and redo, with at most
  100 steps. One oversized newest step may exceed that budget. Timeline snapshot cloning
  remains; this pass does not introduce persistent data structures or per-operation undo.
- Publication still copies timeline and media metadata under the session lock. It no longer
  copies real waveform arrays there or computes/serializes patches there. Pending intermediate
  snapshots can coalesce; the frontend rejects older full snapshots and resets selection/cache
  state on project epoch changes. Existing test event counters now await publication completion.
- Export input ownership survives ordinary queue reloads. Files are released only after terminal
  queue persistence succeeds. Crash orphan collection and atomic destination reservation across
  concurrent external writers are not introduced. Cancellation after enqueue is a queue action.
- GPU readback remains synchronous. The measurements below justify profiling that boundary;
  they do not establish that a platform-specific texture or pipelining redesign is appropriate.

### Validation evidence

Two executions of the existing GPU-only `engine_workloads` quick benchmark completed on the
Apple M4 with identical current code, except printing counters in the second run:

| Scenario | First estimate | Second estimate |
| --- | ---: | ---: |
| 1080p, 2 layers | 1.7782 ms | 2.1202 ms |
| 1080p, 4 layers with effects | 3.2672 ms | 3.5450 ms |
| 4K, 2 layers | 7.3268 ms | 7.3427 ms |
| 4K, 6 layers with effects | 19.526 ms | 18.090 ms |

Second-run counters: 1 LUT upload, 1,015 LUT cache hits, 1,340 readbacks, cumulative readback
wait 4.462374955 s and mapped host copy 759.146262 ms. Counts span all scenarios and warmup;
these totals are not per-scenario latency. The quick runs vary substantially and have no
pre-change baseline, so no percentage improvement is claimed.

Desktop all-target compilation passed during implementation. Rust checks compile existing test
binaries without executing tests. Mechanical fixed-size chunk iterator changes adapt existing
code to the Rust 1.99 Clippy convention; no new test cases were added or run.

Frontend validation is partial: the interval-index module passed strict TypeScript checking,
and changed frontend files passed TypeScript syntax transpilation. Full typecheck/lint/build
could not complete because dependencies were unavailable: the configured npm mirror returned
403 and the public registry failed DNS resolution. The lockfile was restored unchanged.
Real-media analysis quality, project-switch concurrency, export restart/cancellation and C/GPU
pixel parity remain runtime acceptance work; they are not reported as proven by compilation.

Final static checks passed:

- `FFMPEG_DIR=/opt/homebrew CARGO_BUILD_JOBS=1 cargo clippy -p desktop -p bush -p core
  -p avbridge --all-targets --offline -- -D warnings`.
- `FFMPEG_DIR=/opt/homebrew CARGO_BUILD_JOBS=1 cargo clippy -p bush --features gpu
  --all-targets --offline -- -D warnings`.
- `cargo fmt --all --check`, `git diff --check`, and syntax transpilation of all ten changed
  frontend files (zero syntax diagnostics).
- `graphify update .` refreshed the AST graph. It reported six pre-existing JSON source files
  without graph nodes and refreshed community names; these warnings did not stop extraction.

The whole-workspace Clippy attempt was interrupted after resource exhaustion and a Python
binding build wait; the successful commands above cover every modified Rust crate. Frontend
build/typecheck/lint and runtime acceptance remain unverified, rather than marked complete.
