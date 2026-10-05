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

## Not yet measured

Memory over hours (the 10 minute soak shows a small unexplained drift), proxy generation time
before and after the decoder change, thumbnail latency on long recordings, multi-track export
speed.
