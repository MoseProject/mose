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

## Not yet measured

GPU/VRAM and process RAM under real editing, thumbnail/waveform cache pressure, export
throughput, webview render cost of the Inspector for large sessions. Needs a run of the built
app with the telemetry sampler on and a trace from the webview.
