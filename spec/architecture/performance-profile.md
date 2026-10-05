# Performance profile (2026-10-04)

Measured with `cargo bench -p bush` and `cargo bench -p core --bench parsing` (LTO off,
codegen-units 16, release opt level). Raw output of the run lived in `target/perf/`.

## Bush engine (CPU renderer, 1080p unless noted)

| workload | median |
|---|---|
| open to first RGBA frame | 6.0 ms |
| seek to first RGBA frame | 6.2 ms |
| engine random seek, h264 software | 0.91 ms |
| composite two layers, untransformed | 9.6 ms |
| composite two layers, scaled+rotated+blended | 14.2 ms |
| live edit (opacity) on cached frame | 1.06 ms |
| playback h264 software / hardware fallback | 34.9 / 82.9 ms |
| decode policy software / hardware fallback | 13.6 / 58.1 ms |
| gaussian blur 1080p / 4K (independent of sigma) | 27 / 97 ms |
| sharpen 1080p / 4K | 46 / 185 ms |
| time stretch, 1 s stereo | 6.7 ms |

Findings: hardware decode is 2-4x slower than software (already off by default); blur and
sharpen are the heaviest per-frame effects and cannot hold 30 fps at 1080p on the CPU renderer
(candidates: separable/box-approximated blur, SIMD, or the GPU path); an untransformed
two-layer composite costing 9.6 ms points at a copy/blend that could be skipped for opaque
full-canvas layers.

## Cost of one edit (desktop shell)

| clips per track (3 tracks) | Project clone | Sequence clone (undo) | session JSON (emit) | save .ocproj |
|---|---|---|---|---|
| 10 | 17 us | 6 us | 0.21 ms | 0.21 ms |
| 200 | 111 us | 92 us | 2.9 ms | 2.6 ms |
| 1000 | 493 us | 468 us | 15.4 ms | 12.0 ms |

The JSON of the whole project sent on every `session-changed` dominates (30x a clone), then
the webview parses it and rebuilds the store. At 3000 clips one publish exceeds a 60 Hz frame.
Fix direction: send a patch or only the changed sequence/track, or throttle publishes from
continuous controls (inspector sliders). Timeline drags already commit once on pointer-up.

## Changed in this pass

| change | before | after |
|---|---|---|
| Session sync sends a diff (`session_sync.rs`, `session-patch` event, `sessionPatch.ts`) instead of the whole project | one clip edit in 3000 clips: 11.2 ms, 4.8 MB of JSON to serialize and parse | 0.5 ms, 1.7 KB; unchanged tracks/clips keep their identity in the webview |
| Blur without transposes, one thread round per direction, parallel sharpen | gaussian 1080p 27 ms, 4K 97 ms; sharpen 46 / 185 ms | 22 / 85 ms; sharpen 28 / 105 ms |
| CPU compositor copies opaque rows, skips the division over an opaque backdrop | 2 layers untransformed 9.6 ms | 6.5 ms |
| Preview scene is not rebuilt for edits it does not show (rename, export settings, layout, playhead catch-up) | every publish re-mapped and re-rendered | skipped, decided by `same_preview_content` |
| `Command::Content` carries `Arc<Project>` | two full project clones per edit | one |
| Undo history bounded by clips held (120k clips total) | 100 full sequence copies regardless of size (about 200 MB at 3000 clips) | about 40 steps at 3000 clips, 100 steps for normal projects |

The coarse waveform (2000 min/max pairs per audio asset) no longer travels in the session JSON: the
webview gets `has_waveform` and fetches the peaks as bytes from `oca://wave/<id>/coarse`.

Project file (`.ocproj`) is now LZ4-compressed `postcard` (`to_project_bytes`/`from_project_bytes`,
format version 2) instead of gzip + struct-map MessagePack: save 12.2 -> 1.8 ms, load 11.9 -> 3.0 ms
at 3000 clips (the old time was MessagePack writing about 70 field names per clip, not gzip).
The encoding is positional, so changing the fields of a type inside `Project` requires bumping
`PROJECT_FORMAT_VERSION`; other versions are refused with an error. Prefs, queue and transcript
files keep the self-describing MessagePack framing (prefs preserve unknown keys).

## Resource profile of the running app (2026-10-04)

`e2e/profile_resources.py` launches the built app through tauri-driver, samples RAM and CPU of
the whole process tree (app plus WebView2 children) every 0.5 s and GPU (dedicated memory and
engine utilization, Windows performance counters) in parallel. Same scenario run on the build
before PR 179 (`411fc4ca`) and after. Machine: Ryzen 9 5950X, RTX 4070, release build without LTO.
Media: a 1080p60 and a 2160p30 H.264 clip with audio (about 290 MB each).

Peak RSS of the process tree, MB:

| phase | before | after |
|---|---|---|
| idle, home screen | 426 | 424 |
| 2 clips imported | 471 | 459 |
| scrub, 120 seeks | 692 | 676 |
| play 30 s across four cuts (3x 1080p60, 1x 4K) | 701 | 682 |
| 300 edits, 4 clips | 708 | 681 |
| grow to 604 clips | 1425 | 742 |
| 300 edits, 604 clips | 1502 | 769 |
| idle after | 1415 | 760 |

| | before | after |
|---|---|---|
| round trip of one `update_clip`, 4 clips | 10.6 ms | 7.7 ms |
| round trip of one `update_clip`, 604 clips | 41.9 ms | 6.5 ms |
| webview JS heap after the 604-clip session | 663 MB | 16.7 MB |

- RAM: with a large timeline the webview was the leak-shaped part: every full-session JSON
  replaced the whole project object graph (663 MB of JS heap). With patches the heap stays at
  16.7 MB and the process tree at about half. The edit latency no longer grows with the project.
- CPU: playback of 1080p60 and 4K averages 5-6% of the machine (peak 17-19%, whole tree); an
  edit burst 7-17%. No difference before and after; the playback path was not changed.
- GPU: 26-29 MB dedicated and 0% engine utilization in every phase, before and after. The
  preview composites on the CPU and decodes in software by default (`OCA_BUSH_RENDERER=gpu` and
  `OCA_BUSH_DECODE=hardware` are developer switches), so the GPU is idle apart from the webview's
  own compositor. Playback never used it, so there is nothing to optimize on the GPU side
  without first changing that default.
- Baseline floor: the 424 MB at the home screen is a counting artifact. The tree is the app
  (36 MB unique) plus six WebView2 processes; summing RSS counts their shared pages once per
  process. Unique memory (USS) of the whole tree is about 121 MB.

## Further scenarios (`e2e/profile_scenarios.py`)

Soak, 10 minutes (58 cycles of play 8 s, 40 edits, 20 undos, 30 seeks): mean RSS 816 MB in the
first third and 843 MB in the last (+3%), private memory 608 -> 633 MB, webview JS heap warming up
from 7 to 19 MB and then flat. A slow native drift of a few MB per minute is visible and was not
explained; 10 minutes cannot tell a leak from caches filling, so a longer soak is the next check.

Webview in a 604-clip session with the clip selected and the Inspector open: 300 edits and 10 s
of playback both ran at 16.7 ms per frame (p95 16.8, max 17 ms) with no long task at all.

Two hour 540p recording: import 15 s; the detailed waveform takes 10.1 s to decode and is 2.9 MB;
RSS goes 479 -> 552 MB across it. Filmstrip tiles cost 200-300 ms each (400 tiles in 119 s, 2000
in 390 s) because every distinct time seeks and decodes from a keyframe; RSS grew only 19 MB over
2000 tiles (the 1500-entry tile cache holds). Tile latency, not memory, is the limit on long
recordings.

Compositor and decoder switches (`OCA_BUSH_RENDERER`, `OCA_BUSH_DECODE`), 15 s of playback across a
1080p60 and a 2160p30 clip. Delivered distinct frames were 707-724 in every combination (about
47 per second), so the choice does not change what the webview sees:

| | CPU mean / peak | RSS | VRAM | GPU engines |
|---|---|---|---|---|
| CPU compose, software decode (default) | 7.4% / 26% | 677 MB | 29 MB | 3D 0.2% |
| GPU compose, software decode | 5.1% / 8% | 954 MB | 203 MB | 3D 2.6% |
| CPU compose, hardware decode | 6.5% / 19% | 733 MB | 55 MB | VideoDecode 1.7% |
| GPU compose, hardware decode | 4.5% / 7% | 891 MB | 210 MB | 3D 2.7%, VideoDecode 1.5% |

The GPU compositor trades about 280 MB of RAM and 175 MB of VRAM for 2-3 points of CPU and a lower
CPU peak; not worth making the default at this load.

Export, 60 s of 1080p60 (40 Mbps source), hardware encoder, output matched to the source bitrate:
52-57 s before, 25.5 s after (1.1x -> 2.4x real time), same 280 MB output. The process tree
averaged 3% CPU (one core of 32) before: the ffmpeg command line with the same filters and encoder
does the file in 9.4 s. A stage timing split of the export loop (single track path,
`timeline_export.c`; bridge test harness, 20 s of source) showed 10.5 of 13.6 s inside the video
decoder, against 0.5 s for filters and 2.0 s for encoder submission: the export decoders were
opened by hand without a thread count, so they decoded on one thread (about 115 fps). They now use
frame plus slice threading (`bridge_enable_decoder_threads`), which took the harness run from
14.1 s to 4.0 s. Frame threading holds frames back until the decoder is flushed, so each segment
now drains its decoder at the end of the input (the multi-track path sends an empty packet at EOF);
without that a segment ending with its file lost 16 frames, and a test covers it. Proxy generation
uses the same decoder setting. Overlay (secondary track) decoders and the privacy-blur pass keep
their single-threaded decoders; the remaining gap to the command line (6.4x) is encoder
submission and the loudness and mux passes.

## Corrections and later passes (2026-10-05)

**The GPU poller distorted the export numbers above.** `profile_scenarios.py` polled GPU counters
through PowerShell on every export poll (and `profile_resources.py` on a second thread), and that
cost the app it measured: the same 60 s export took 26.8 s with the poller and 12.3 s without
(`OCA_PROFILE_NO_GPU=1` turns it off). Re-measured without it: 37.2 s before the decoder change,
12.3 s after, so the decoder change was a 3x gain and not the 2.1x quoted above. The other tables
in this document sample GPU counters on their own thread and are not affected the same way for
CPU-bound work, but treat the GPU columns as approximate.

**Export audio on its own thread.** The same 60 s export split as video filter and encode 5.0 s,
audio (decode, afftdn, loudnorm, alimiter, AAC) 4.4 s, everything else under 0.6 s, all in one
loop, so the video encoder idled while the audio graph ran. `audio_worker.c` now owns the audio
graph and encoder behind a bounded frame queue (per-segment gain and tempo commands travel through
the same queue, so they stay ordered with the frames); both threads write the muxer behind a mutex
found through `out_ctx->opaque`. Bridge harness 10.2 s -> 7.2 s (the ffmpeg command line on the
same encoder: 7.5 s); in the app 12.3 s -> 8.8 s, 6.8x real time, applied to the single-track and
multi-track paths. Multi-track alone (no overlay) takes 9.2 s against 7.2 s for one track; the
overlay decoders stay single-threaded.

**Frames the export was losing** (found while measuring, both silent):
- every segment of a single-track export lost its last frame (the `fps` stage holds its newest
  frame until told its input ended), so the video fell one frame behind the audio per cut;
- a multi-track export stamped overlay layers with a frame counter in the source's time base, so
  `fps` saw a second of video in a millisecond: a clip fully covered by an overlay exported with 0
  video frames, one covered for 0.5 s lost 15 frames. Layers are now stamped with real time, and
  graphs freed on a mode change or at the end of a segment are drained first.
Tests assert exact frame counts for both.

**Proxies and overlay passes.** Proxy generation scaled on one thread and encoded with one:
`threads=0` on the scaler and eight encoder threads take 60 s of 1080p60 from 13.7 s to 8.5 s and
60 s of 2160p30 from 8.3 s to 5.5 s (the ffmpeg command line with libx264 ultrafast: 8.1 s). The
text, shape and privacy blur passes re-encode the whole exported file, and ran a single-threaded
decoder into the CPU encoder: 61.7 s for 60 s of 1080p60, several times the export they follow.
They now decode with frame threading and open the hardware encoder when its input format matches
the graphs (CPU otherwise): 11.8 s for the same text pass, all 3600 frames and the audio intact.
Software encoders everywhere get eight threads (the CPU export fallback was single-threaded).

**Filmstrip tiles.** A screenful of tile requests spawned a thread and opened a decoder each, so 24
simultaneous requests on a 2 hour file each waited 1.3 s (every decoder starts a thread per core).
Opening costs 45 ms and the frame 30 ms. `tile_workers.rs` drains requests with four workers that
keep the decoder of the file they just served and drop it after 300 ms idle. Bench burst: first
tile at 190 ms (was 1049 ms), all 24 in 0.98 s (was 1.26 s). In the app, 400 scattered tiles of the
2 hour file: 118 s -> 45 s. A 2000 tile run returned only 717 images in the app (0 failures in an
isolated run); probably tiles requested while the proxy was still being written, not confirmed.

**Memory drift: not a leak in the app.** A 40 minute soak showed RSS +131 MB (about 3 MB/min),
not flattening. Isolated: the native app process stayed at 374 -> 378 MB over 20 minutes; the growth
is in the WebView2 renderer. Through the DevTools protocol (`profile_cdp.py`), 20000 edits make the
renderer reach 320 MB and a forced collection returns it to 44 MB with the JS heap at 3 MB and the
DOM node count constant, so that part is garbage waiting for V8 (each edit allocates about 13 KB in
the renderer, mostly Tauri's event script compilation). The residual retention in the long soak
(about 5 KB per call) comes from the WebDriver harness: 6000 `execute_async_script` calls that
invoke a command retain 34 MB after collection, where the same invocations in an in-page loop
retain nothing.

**Models, playback timing, idle cost** (2026-10-05, later pass):
- Face detection and person segmentation built an ONNX session per sampled frame. Sessions are
  cached per model path and dropped after 20 s idle: MODNet at 1080p 410 -> 270 ms per frame,
  UltraFace 19 -> 4 ms. Eight and 32 ORT threads take the same time, so the default stays.
- On Windows `recv_timeout` is as coarse as the system timer (a 1 ms wait takes 15.5 ms, measured;
  `thread::sleep` takes 1.5 ms). The playback loop therefore ran every 15.5 ms and the 16 ms
  preview tick went out every second pass: 1080p60 showed 34 pictures a second, now 58 (counted
  from the monitor image's `src` changes, `e2e/profile_fps.py`). The idle loop now waits for a
  command instead of polling: thread wakeups of an idle app 77 -> 9 per second
  (`e2e/profile_idle.py`). Seek to picture about 45 ms, edit to picture 35 ms
  (`e2e/profile_latency.py`); first paint after launch 640-690 ms, of which our JS is 10 ms.
- The CPU encoder (libopenh264) takes about 23.5 s per 60 s of 1080p60 whatever thread count it is
  given; only the hardware encoder is fast.
- Not measured: Whisper (no model on the machine; its default is 4 threads).

## Not yet measured

Multi-track export with several overlays, project open time on a large project, and Whisper
transcription speed (no model on the measuring machine).
