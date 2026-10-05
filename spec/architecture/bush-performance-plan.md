# Bush Performance Validation and Optimization Plan

Validated 2026-10-05 from the current `crates/bush` implementation, existing performance records,
and fresh Criterion runs. This document separates measured evidence from proposed work and records
the environment and limits of each measurement.

## Validation result

The crate already has useful performance instrumentation: bounded submit-to-publish latency
percentiles and counters in `Engine`, benchmark suites for scheduling/cache/decode/composition,
and app playback telemetry. Its main architectural safeguards are also present: latest-value
request coalescing, cancellation at decode boundaries, byte-bounded frame caching, limited open
decoders, frame reuse, source-rate pacing, and GPU upload reuse.

The available measurements in [the Bush matrix](../matrix/bush.md) show these remaining costs:

| Workload | Recorded result | User impact |
| --- | ---: | --- |
| 1080p CPU engine playback, sequential | 10 ms/frame average | Within a 33 ms 30 fps budget in the recorded run |
| 4K CPU engine playback, sequential | 37 ms/frame average | Misses 30 fps; p95 decode latency recorded at 47 ms |
| 3 x 1080p CPU multicam | 25 ms/frame average | Little headroom for UI, encode, or heavier effects |
| 1080p H.264 random scrub | p50 7.9 ms, p95 27.6 ms | Tail dominated by key-unit seek and decode-forward |
| 4K GPU composition, 2 layers | 24 ms/frame | Just inside 30 fps, before app JPEG delivery |
| 4K, 6 layers with effects, CPU / GPU | 594 / 65 ms | GPU helps substantially; neither sustains real-time preview |
| Hardware decode with CPU download | roughly 2x slower than software | Keep opt-in; it is not an optimization on the measured machine |

The table above contains the repository's earlier measurements (release profile, Ryzen 9
5950X, RTX 4070, Windows 11, FFmpeg 8.1). A fresh, narrower CPU compositor sample was collected
on 2026-10-05:

| Workload | Current host result | Environment |
| --- | ---: | --- |
| 1080p, 2 untransformed CPU layers | 2.45 ms (95% CI: 2.26-2.73 ms) | Release Criterion, Apple M4 (10 cores), macOS 26.6, FFmpeg 9.0 |
| 1080p, 2 scaled/rotated/multiply CPU layers | 4.41 ms (95% CI: 3.87-4.90 ms) | Same run |

Testing a cap of four instead of eight produced mixed results. In the smaller
`composite_1080p` workload, four workers measured 2.42 ms untransformed (no significant change)
and 3.42 ms for scaled/rotated/multiply, with Criterion reporting a 15.6% improvement. But the
broader full-frame CPU workloads consistently favored eight workers:

| CPU compositor workload | 4 workers | 8 workers | Criterion change at 8 vs 4 |
| --- | ---: | ---: | ---: |
| 1080p, 2 layers | 12.82 ms | 8.78 ms | 34.6% faster |
| 1080p, 4 layers with effects | 63.43 ms | 52.19 ms | 17.7% faster |
| 4K, 2 layers | 45.80 ms | 27.29 ms | 37.5% faster |
| 4K, 6 layers with effects | 657.27 ms | 349.44 ms | 46.8% faster |

Four-worker 1080p/4K measurements were repeated after the eight-worker run and remained slower:
11.88 ms (1080p, 2 layers), 64.33 ms (1080p, 4 layers with effects), 34.47 ms (4K, 2 layers),
and 449.02 ms (4K, 6 layers with effects). This supports keeping the existing eight-worker cap
for full-canvas CPU composition. Four workers only won the narrower small-overlay transformed
benchmark, so that result does not justify changing the global cap.

Commands: `cargo bench --locked -p bush --bench engine_workloads -- composite_1080p` and
`cargo bench --locked -p bush --features gpu --bench engine_workloads -- gpu_vs_cpu`, comparing
Criterion baselines with four and eight workers. Environment: Apple M4, macOS 26.6, FFmpeg 9.0,
release profile. These compositor-only results do not include decode, JPEG encoding, or
presentation and are not directly comparable to the end-to-end numbers above. Additional
end-to-end 1080p H.264, 4K HEVC, and three-angle tests used generated 1.2 s synthetic clips in a
temporary directory. Their results varied substantially between runs: even decode-only timings
(which do not call the compositor) shifted by 20-50% across runs. Treat those end-to-end deltas as
inconclusive for worker-count attribution; multicam likewise showed no significant difference.
The initial compile exposed a missing `libavutil/opt.h` include in
`crates/avbridge/csrc/proxy.c`; adding the header fixed the build. Cargo also printed YARA
synchronization 503 warnings, which did not affect the benchmarks.

## Optimization sequence

Priorities target the largest documented user-visible gaps. Keep each change independently
measurable and preserve Bush's frame-accuracy and output-parity contracts.

### Rejected experiment: reduce CPU compositor worker fan-out

Reducing `MAX_WORKERS` from eight to four improved one smaller transformed 1080p case, but
regressed all four full-frame CPU compositor workloads in two runs. Keep the existing eight-worker
cap. The end-to-end comparisons were too noisy to attribute to worker count; repeat them under
controlled thermal/load conditions and on other core counts before revisiting the cap.

### P0: 4K decode and conversion cost

**Evidence:** 4K software decode is 21-26 ms/frame depending on codec; 4K engine rendering is
37 ms/frame. Current decode converts frames to full-resolution RGBA before the compositor scales
them to the preview canvas. Hardware decode plus CPU download loses to threaded software decode.

**Plan:** measure proxy versus original end-to-end first, then prototype scaling during pixel
format conversion so the decoder emits only the requested preview dimensions. Retain source
timestamps and aspect ratio, and use identical output dimensions for comparisons. Treat zero-copy
GPU decode as a separate exploratory option only if the scaled software path is insufficient;
include upload/readback, CPU-only effects, JPEG delivery, startup latency, and device support in
that cost model.

**Acceptance:** on recorded 4K H.264 and HEVC fixtures, compare release decode-only and full-engine
throughput, p50/p95 frame time, first-frame time, and peak retained bytes against the current
baseline. A 30 fps target requires p95 below 33.3 ms for the complete preview path, not just decode.
Verify frame selection, color conversion, aspect ratio, and cancellation at exact seeks.

### P1: long-GOP seek tail

**Evidence:** 1080p scrub p95 is 27.6 ms; decode-forward from a preceding key unit is the known
dominant cost. p50 is already 7.9 ms.

**Plan:** profile seek, packet decode, frame conversion, and composition separately on long-GOP
H.264/HEVC samples. Compare GOP lengths and random-seek distributions. Only then consider a small,
byte-bounded keyframe/decoded-frame prefetch window, sharing existing source identity and cache
invalidation rules. Drop prefetched work immediately when the seek intent changes.

**Acceptance:** report p50/p95/p99 for cold and warm random seeks, memory high-water mark, and
superseded work. Preserve exact target-frame semantics and prove stale prefetch cannot occupy the
decoder needed by active playback.

### P1: multicam and heavy-effect throughput

**Evidence:** three 1080p sources consume 25 ms/frame on CPU; 4K six-layer GPU effects consume
65 ms/frame. Existing independent source decode is parallel, but composition and device transfer
remain costly.

**Plan:** use stage timings to distinguish decode saturation, CPU composition, GPU upload/readback,
and effects. Evaluate proxy-first decode for concurrent angles and retain GPU composition for
effect-heavy scenes. Avoid adding more workers or parallelizing per-layer work until profiling
shows available CPU/GPU headroom; current cache, decoder, and request bounds must remain explicit.

**Acceptance:** run one-, two-, and three-angle 1080p playback and 4K effect-heavy scenarios with
release builds. Record delivered fps, dropped/deadline-missed frames, p95 frame time, CPU/GPU
utilization, memory high-water mark, A/V drift, and angle-switch latency. Compare CPU and GPU
using the same input and include transfer costs.

### P2: app delivery and latency attribution

**Evidence:** the earlier app run recorded 6.7 ms JPEG encoding per frame at 1280x720, and the
GPU renderer returns CPU frames, requiring readback. Engine submit-to-frame latency alone does
not include the entire webview delivery path.

**Plan:** expose or log stage-level timings for request wait, decode, conversion, effects,
composition, GPU transfer, JPEG encode, and presentation. Use these to decide whether JPEG format,
quality, resolution, or transport should change. Keep telemetry bounded and avoid per-frame string
formatting on the worker path.

**Acceptance:** compare full submit-to-visible-frame latency and frame delivery rate in the real
app, alongside engine latency. Verify telemetry does not expose paths/media identifiers and has
bounded storage.

## Measurement protocol

Use release builds and the existing `make bench-bush` / Criterion targets. For large-media runs,
set `BUSH_BENCH_MEDIA_DIR` to the documented 1080p H.264 and 4K HEVC fixtures. Record CPU, GPU,
OS, FFmpeg build, codec, resolution, frame rate, GOP, bitrate, cache state, renderer, and decode
policy. Warm up before sampling; report repetitions and p50/p95/p99 where relevant. Pair
microbenchmarks with real-app playback because a component win may move cost to GPU transfer,
JPEG encoding, or presentation. Keep baseline and candidate results side by side in
`spec/matrix/bush.md`.

Do not promote hardware decode based on decode throughput alone. Do not claim real-time 4K based on
average frame time alone. Every optimization must preserve exact seek behavior, cancellation,
bounded caches/queues, and CPU/GPU output parity within the existing contract.
