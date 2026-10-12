# Performance work, October 2026

What changed in export, preview and the editor UI, how each was measured, and what is left.
Numbers come from a 4 vCPU sandbox without a GPU, release builds, synthetic clips (1080p60 at
50 Mbps unless noted); compare before and after on the same machine, do not read them as
absolute. The older bush figures live in [performance-profile.md](performance-profile.md) and
[bush-performance-plan.md](bush-performance-plan.md).

## Export

| change | effect |
|---|---|
| Mask, opacity, brightness/contrast keyframes without `geq` (extractplanes + alphamerge, `sendcmd` + `lutyuv`) | 5-12 fps -> about 100 fps |
| Zoom in, zoom out (pad + `zoompan`) and pan/zoom crop keyframes through `zoompan` | under 10 fps -> 78-89 fps |
| Text, shape and privacy-blur overlays inside the encode (post filter chains) | no second decode and re-encode of the file |
| OpenH264 slices (one per core, up to 8) and an encoder worker thread | plain 1080p12M 144 -> 160 fps, 4K60 28 -> 38 fps |
| Lossless cut (`smart_cut.c`): copy the GOPs between IDR keyframes, re-encode only the ends | 21.9 s cut at 50 Mbps: 18 s -> 3.4 s; also for several ranges of one source |
| Audio normalization is optional (`NO_NORMALIZATION`, a NaN target); off keeps gain and a peak limiter only | no afftdn/loudnorm cost |

Still slow: the zoom transition (42 fps, `geq` for its first frames), picture in picture (58 fps,
two decodes and an overlay). Hardware encoders are present but untested here.

Export is bound by decode (about 220 fps for 1080p60 at 50 Mbps on 4 cores) and the encoder
(about 140 fps), so effects that stay in the graph land near 90 fps.

## Preview (bush)

- `avbridge_decode_next_rgba` converts straight into a Rust-owned buffer instead of a native
  allocation that was then copied: 4K decode 38.7 -> 6.7 ms/frame, 1080p 7.8 -> 1.6 ms.
- A layer stretched to the canvas (no pixel-level effects, no animated size or crop) is converted
  by swscale to the size it is drawn at. 4K source on a 720p canvas: p50 15.9 -> 8.7 ms.
- A layer an opaque, canvas-covering layer above completely hides is neither decoded nor composited
  (`Contributor::covers_canvas_at`, `Engine::first_visible`). 16 stacked layers: 54 -> 5 ms.
- swscale left the alpha of the last pixels of each row unwritten when the width is not a multiple
  of 8, so the default 854 pixel wide 480p proxy had a transparent right edge. Fixed in
  `copy_rgba`; `a_width_that_is_not_a_multiple_of_eight_is_opaque_to_the_last_pixel` guards it.
- CPU compositor: unrotated layers compute the source column once per destination column;
  translucent normal blends and brightness/contrast use 256-entry tables built with the same
  expressions (byte-identical). 4 layers at 720p: opacity 42 -> 22-25 ms, colour with opacity
  67 -> 45 ms.

Per extra visible layer the CPU compositor costs about 3.5 ms at 720p, plus 7 ms for colour
adjustments and 17 ms for blur. GPU compositing (`OCA_BUSH_RENDERER=gpu`) is the remaining lever
and has not been measured on this setup.

Second round (4 cores, Linux, release, FFmpeg 8.1.3, `layers_probe` with 4 layers at 720p):

- Masks: a pixel far from the mask edge gets its factor (exactly one or zero) from a comparison
  instead of a square root and a division (`LayerMask::alpha_factor_fast`, equal bit for bit to the
  full expression over a dense grid), a fully transparent pixel skips the blend, and which effects
  apply is decided once per layer (`Shading`). Mask 44 -> 35 ms, rotate 31 -> 27 ms, colour
  7.8 -> 5.8 ms. What is left of the mask cost is mostly decoding four sources.
- Scrubbing a long-GOP original (1080p60, 50 Mbps, 100-frame GOP): random seeks were p50 274 to
  476 ms, p95 1.1 to 1.5 s, because a seek decodes forward from the key frame (about 4.5 ms a
  frame). Skipping non-reference frames while decoding forward saved 3 % in an ffmpeg test, so it
  was not pursued. `Engine::set_seek_budget` lets a seek stop decoding forward at a deadline and
  show the newest frame it has (never cached; the exact request after it carries on from there):
  with a 20 ms budget p50 64 ms, p95 91 ms. The editor sets a 30 ms budget while the playhead is
  dragged (timeline ruler and the monitor slider) and renders the exact frame when it ends.
  Proxies (short GOP, no B-frames) were already fast (p50 16 ms) and are unaffected.
- Playback jitter (p95 51 ms against p50 12 ms, max 260 ms in one run) was the machine: the
  outliers were scattered rather than periodic (`PROBE_TRACE=1` on `playback_probe` lists them),
  decode-only runs were steady at 4.9 ms/frame, and in quiet runs p50 was 10 ms, p95 21 ms,
  max under 50 ms. Emulating a frame buffer pool (no mmap churn, `MALLOC_MMAP_THRESHOLD_`)
  changed nothing, so no pool was added.
- Blur (about 11 ms a layer at 720p, 4 cores) already uses running sums, vectorized column
  bands and all cores, and costs the same at any sigma. Only an approximation (blur a half-size
  copy and scale it back, for strong blurs) would be faster, by about 1.5x after the scaling, and
  it would give up the exact result the CPU and GPU paths share; it was left out.
- Preview JPEG (`cargo test --release -p desktop jpeg_cost -- --ignored --nocapture`, noisy
  synthetic frames): 854x480 3.7 ms, 1280x720 8.6 ms, 1920x1080 19.6 ms, 4:2:0 with SIMD, on its
  own thread so it overlaps the next render; below the 33 ms frame interval at every size.

Import is bound by decoding (the 480p proxy of 22 s of 1080p60 takes 8 s, ffmpeg itself 7.4 s).
Random scrub on the 480p proxy: p50 16 ms, p95 31 ms; on a 1080p source with a 2 s GOP: p50 60 ms,
p95 133 ms on the earlier machine, 274 ms and 1.1 s on this one before the seek budget (see above).
No prefetch exists; the budget made one unnecessary for dragging.

## Editor UI

At 3000 clips on 30 tracks, scroll p50 158 -> 34 ms and zoom p50 173 -> 33 ms (Vite dev build).
The minimap drew one element per clip on every render, the screen-reader clip list was rebuilt and
sorted on every render, and track headers and library rows re-rendered on every zoom. The minimap
now joins touching clips into runs above 300 clips, the clip list is memoized and windowed above
200 clips, and headers and rows are memoized. Backend work per edit stays small: at 3000 clips a
project clone is 0.4 ms, the diff after one move 1.7 ms, scene mapping 1.1 ms; the full JSON
snapshot (first load only) is 50 ms. JS heap: 35 MB (48 clips), 55 MB (6000 clips).

### What is built and fetched only when shown

Entry chunk 271 -> 235 KB (gzip 85 -> 73), Editor chunk 124 -> 95 KB (gzip 33 -> 27); the rest is
split into chunks of 1 to 6 KB that arrive when first needed.

- An inspector section builds its body when it is first opened and its tab is on show (a closed
  section, or one on another tab, costs its header only). The text and shape inspectors and the AI
  actions (Effects tab) are chunks of their own.
- Menu entries that bring a dialog (layer templates, transcript, silence review, sound library,
  YouTube download, text to speech) are built, and their code fetched, when their menu first
  opens (`Menu deferred`); the effects browser when its tab is chosen.
- Only the language in use is loaded (`loadLocale`, about 7 to 8 KB gzip each).
- The Editor is fetched while the home screen is idle and the export dialog as the pointer or focus
  reaches its button (`chunks.ts`); a lazy component then renders at once.
- Media thumbnails are `loading="lazy"` (a library of hundreds of files no longer asks for every
  thumbnail at once), and library cards, queue jobs, shortcut rows and project rows use
  `content-visibility: auto` so what is scrolled out of sight is neither laid out nor painted.
- `visual/lazy-loading.visual.ts` watches the module requests: an open project fetches none of the
  on-demand code, and each piece arrives with its trigger.

## Backend (analysis of the Rust side, 4 cores, release)

Measured and fine: per-edit work at 3000 clips (clone 0.4 ms, diff 1.7 ms, scene mapping 1.4 ms);
saving a project 1.0 ms and loading it 4.6 ms (3000 clips, 28 KB file); importing one 22 s 1080p60
clip once the file is in the cache: probe 0.02 s, waveform 0.07 s, loudness 0.75 s, proxy 8.1 s
(decode-bound, 2.7x real time). The first run of `import_perf` on a file not yet read measured
8 s for the waveform: that is the disk, not the code.

Changed:

- **Importing several files** used a thread per file with no limit, so N proxies ran together and
  all finished after N times as long. With 4 files of 22 s: all at once, every proxy ready at
  29 s; one by one, 8, 17, 25 and 33 s; two at a time, 14.5, 14.5, 29 and 29 s (total 29 s, the
  best of the three). `import_gate` lets the audio passes (waveform, loudness) go a few at a time
  and the proxies two at a time on four cores, in the order the files were imported.
  `import_concurrency` (core tests) measures it.
- **Strip tiles (timeline thumbnails)** of a long-GOP original took 173 ms each (p50) and a random
  one could fail past the 800 ms deadline. They are now converted at the size asked for and decode
  forward for at most 40 ms (`FrameSampler::sample_small`, Bush's seek budget): 61 ms, and they
  always answer. Once the proxy exists tiles come from it, exactly. A full tile cache forgets its
  oldest quarter instead of everything.
- **Recent projects** decoded ten full project files, twice at startup, on the main thread with
  the preferences locked. Summaries are kept per file (revalidated by modification time in
  nanoseconds and size) and the command runs off the main thread.
- **Saving** encoded and flushed the file with the session locked; it now writes from a copy, and
  edits made meanwhile keep the project unsaved.
- **Queueing an export** with a compound clip rendered its nested sequence on the main thread; the
  command now runs off it.
- `cargo test -p core --lib` had not compiled since the per-export normalization change (a field
  listed twice in a test); fixed.

Not changed: the per-GOP cost of exact seeks (see the seek budget above), proxy encoding (decode
bound), and the AI analyses, whose compute (silence, scenes, highlights) is small next to decoding
the frames they sample; the ONNX and Whisper models are not available here to time.

## Tools

| what | how |
|---|---|
| Benchmark clip | `scripts/make-bench-video.sh` |
| Export timings | `OCA_BENCH_VIDEO=clip.mp4 cargo test --release -p core --test it export_perf -- --ignored --nocapture` (`OCA_BENCH_ONLY` filters) |
| Import steps | `import_perf` in the same test binary |
| Several imports at once | `OCA_BENCH_FILES=4 ... import_concurrency` in the same test binary |
| Strip tile cost | `OCA_BENCH_VIDEO=clip.mp4 cargo test --release -p desktop thumbnail_cost -- --ignored --nocapture` |
| Slow frames in playback | `PROBE_TRACE=1 playback_probe clip.mp4` lists frames over twice the median |
| Preview JPEG cost | `cargo test --release -p desktop jpeg_cost -- --ignored --nocapture` |
| Decode speed | `cargo run --release -p bush --example decode_throughput -- clip.mp4` |
| Scrub latency | `scrub_probe` example (`PROBE_BUDGET_MS=30` for the drag behaviour) |
| Playback cost per layer count | `layers_probe` example (plain, fade, colour, mask, blur, rotate) |
| Per-edit backend cost on a big project | `cargo test --release -p desktop stress -- --ignored --nocapture` |
| Frontend against a mocked backend | `apps/desktop/tools/ui-perf.mjs`, `ui-drag.mjs`, `ui-shots.mjs`, `ui-sizes.mjs`, `ui-a11y.mjs`; the session JSON comes from `dump_session_json` (see the header of `ui-perf.mjs`) |

## Open

- Hardware decode and encode (NVENC, Quick Sync, AMF) and GPU compositing: need a machine with a GPU.
- Dynamic preview quality for many effect layers: changes what the user sees, needs a decision.
- Zoom transition through `zoompan` needs the clip split in two graph segments.
- The white label on the cyan primary buttons has a 2.4:1 contrast ratio.
