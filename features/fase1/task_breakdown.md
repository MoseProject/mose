# Task Breakdown — Fase 1 (Motor central)

Scope: request.md Fase 1 only. Excludes Fases 2-8.

Already done (verified against current code, not re-tasked):
- In-memory project/timeline/media model (`core::project`, `::timeline`, `::media`)
- JSON save/load, wired end-to-end from UI (`home.rs` open dialog, `editor.rs` save) via
  `avcore::{load_project_from_file, save_project_to_file}`

Not done (tasked below):
- `avbridge` C bridge (probe + encode FFI) — `probe.rs`/`render.rs` currently spawn
  `ffprobe`/`ffmpeg` as subprocesses; request.md requires FFI, zero subprocess
- GStreamer preview pipeline — does not exist yet

Toolchain: FFmpeg dev libs (BtbN LGPL shared 8.1) installed, GNU-toolchain link verified
(`avformat_version()` smoke test compiled+linked+ran). GStreamer mingw devel installing in
background — B-series tasks below are blocked until that completes and is verified the same way.

---

### task_id: chore-001
title: Scaffold avbridge C bridge crate
specialist_type: impl
files_to_touch: Cargo.toml, crates/avbridge/Cargo.toml, crates/avbridge/build.rs, crates/avbridge/src/lib.rs, crates/avbridge/csrc/bridge.c, crates/avbridge/csrc/bridge.h
estimated_diff_lines: 120
acceptance_criteria:
  - New workspace member `crates/avbridge` builds with `cargo build -p avbridge`
  - `build.rs` locates FFmpeg via `FFMPEG_DIR` env var (`include/`, `lib/`), fails with a clear message if unset/missing
  - `csrc/bridge.c` exposes one smoke-test `extern "C"` function (`avbridge_version() -> u32`, wraps `avformat_version()`)
  - Rust `lib.rs` has a safe wrapper `pub fn avbridge_version() -> u32` calling the FFI fn
  - No other crate depends on this yet (leaf crate)
execution_order: sequential
context_snapshot: |
  FFMPEG_DIR for this machine: C:\Users\lucas\AppData\Local\Microsoft\WinGet\Packages\BtbN.FFmpeg.LGPL.Shared.8.1_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-n8.1.2-34-g9b6c8969e0-win64-lgpl-shared-8.1
  Verified smoke test: gcc -Iinclude -Llib smoke.c -lavformat -lavcodec -lavutil links and runs
  clean on the GNU (mingw) toolchain already in use by this workspace.

---

### task_id: impl-002a
title: Implement probe FFI, C side (bridge.c/bridge.h)
specialist_type: impl
files_to_touch: crates/avbridge/csrc/bridge.c, crates/avbridge/csrc/bridge.h
estimated_diff_lines: 99
acceptance_criteria:
  - `extern "C"` fn opens a file via `avformat_open_input` + `avformat_find_stream_info`, returns a plain C struct (codec name, width, height, fps num/den, bit_rate, duration_secs) or an error code — no stdout/text parsing
  - Struct has no owned pointers requiring the Rust side to free anything beyond the struct itself
  - Handles missing/corrupt file without panicking or leaking the AVFormatContext (call `avformat_close_input` on every exit path)
execution_order: sequential (dep: chore-001)
observations: |
  Originally scoped as one impl-002 task covering C + Rust wrapper together
  (estimated 150 lines); actual came to 228 lines (99 C + 129 Rust), over both
  the impl-specialist self-limit and the reviewer's 200-line gate. Split along
  the C/Rust file boundary into impl-002a/impl-002b per Manager granularity
  rules — no code rewritten, just the commit split.

### task_id: impl-002b
title: Implement probe FFI, Rust wrapper (safe API + error type)
specialist_type: impl
files_to_touch: crates/avbridge/src/lib.rs
estimated_diff_lines: 129
acceptance_criteria:
  - Rust wrapper returns `Result<ProbeInfo, ProbeError>`, mirrors fields `avcore::probe::ProbedMedia` currently derives from ffprobe JSON
  - `unsafe` FFI call has a `// SAFETY:` comment
  - Manually verified against a real encoded file (ffmpeg-generated mpeg4/aac mp4) and two error paths (missing file, non-media garbage file) — all three matched expected behavior with no panic
execution_order: sequential (dep: impl-002a)

---

### task_id: impl-003
title: Swap core probe.rs from ffprobe subprocess to avbridge FFI
specialist_type: impl
files_to_touch: crates/core/Cargo.toml, crates/core/src/av/probe.rs
estimated_diff_lines: 140
acceptance_criteria:
  - `core` depends on `avbridge`
  - `probe_media()` calls the FFI wrapper instead of `std::process::Command::new("ffprobe")`
  - `ProbeError` variants updated to reflect FFI failure modes (no more `Spawn`/`ExitStatus`/stdout `Json` variants tied to a subprocess)
  - `into_media_asset()` behavior unchanged (same `MediaAsset` shape out)
  - No `ffprobe` binary needed on PATH anymore for probing
execution_order: sequential (dep: impl-002)

---

### task_id: test-001
title: Tests for probe FFI + swapped probe.rs
specialist_type: test
files_to_touch: crates/avbridge/tests/probe.rs, crates/core/tests/probe.rs
estimated_diff_lines: 100
acceptance_criteria:
  - Every impl-002/impl-003 acceptance_criterion has a matching test
  - Covers: valid media file, missing file, non-media file (corrupt/garbage input)
  - No leak-detectable failure path (documented, since no valgrind on Windows — note in test comments which paths were manually verified against `avformat_close_input`)
execution_order: sequential (dep: impl-003)

---

### task_id: docs-001
title: Document avbridge probe path + update stale status notes
specialist_type: docs
files_to_touch: crates/avbridge/src/lib.rs (doc comments), CLAUDE.md, README.md
estimated_diff_lines: 70
acceptance_criteria:
  - Public FFI wrapper has full rustdoc (safety notes on the `unsafe` boundary)
  - CLAUDE.md "What this is" / Architecture sections updated: probe no longer subprocess-based; JSON save/load end-to-end note corrected (it's already wired, current text is stale)
  - CHANGELOG entry if CHANGELOG.md exists (none currently — skip, note as observation)
execution_order: sequential (dep: test-001)

---

### task_id: impl-004a
title: Encode FFI — remux passthrough (no audio filter yet)
specialist_type: impl
files_to_touch: crates/avbridge/csrc/bridge.c, crates/avbridge/csrc/bridge.h, crates/avbridge/src/lib.rs
estimated_diff_lines: 140
acceptance_criteria:
  - `extern "C"` fn demuxes `in_path` and remuxes every stream (video + audio) to `out_path` unchanged — no decode, no encode, no filter (`av_read_frame` + rescale PTS/DTS/duration via `av_packet_rescale_ts` + `av_interleaved_write_frame`, matching plain `ffmpeg -c copy` behavior)
  - Output file opens successfully via `probe()` afterward with the same duration/codec/resolution as the input
  - `avformat_close_input`/`avio_closep` called on every exit path, including error paths mid-loop
  - This is the smallest provable slice: proves the write/mux path works before the harder audio-filter-graph piece
execution_order: sequential (dep: docs-001)

### task_id: impl-004b
title: Encode FFI — audio filter graph (loudnorm + true-peak limiter)
specialist_type: impl
files_to_touch: crates/avbridge/csrc/bridge.c, crates/avbridge/csrc/bridge.h, crates/avbridge/src/lib.rs
estimated_diff_lines: unknown, likely 150-200, may need its own split once attempted
acceptance_criteria:
  - Audio stream only: decode -> `libavfilter` graph (`loudnorm=I=<target>:TP=-1.0:LRA=11,alimiter=limit=0.95:attack=5:release=50`, matching render.rs's current filter string) -> encode AAC -> mux, video stream still passthrough-copied per impl-004a
  - Filter graph and codec contexts freed on every exit path (no leaked `AVFilterGraph`/`AVCodecContext`)
  - Output loudness re-measured (reuse a loudness-measurement path) lands within tolerance of `target_lufs`
execution_order: sequential (dep: impl-004a)
context_snapshot: |
  Two-pass loudnorm (measure then apply, per Fase 2 of request.md) vs. the single-pass
  render.rs currently does (`loudnorm=I=<target>:TP=-1.0:LRA=11` applied directly, no prior
  measurement) — matching current single-pass behavior for this task, true two-pass is a
  Fase 2 concern, not re-scoping it in here.

### task_id: impl-004c
title: Encode FFI — progress callback + cancellation
specialist_type: impl
files_to_touch: crates/avbridge/csrc/bridge.c, crates/avbridge/csrc/bridge.h, crates/avbridge/src/lib.rs
estimated_diff_lines: 80
acceptance_criteria:
  - C fn accepts a progress callback (`void (*)(void *user_data, double seconds_processed)`) invoked periodically during the mux loop, and a `const uint8_t *cancel` pointer checked between packets/frames
  - When `*cancel` becomes nonzero mid-render, the C fn stops, closes contexts cleanly, and returns a distinct "cancelled" status (not an error) — matches `render_export`'s `RenderOutcome::Cancelled` semantics, no `Child::kill()` needed since there's no child process
  - Rust wrapper turns the callback into a safe `impl FnMut(u8)` (percent, matching render.rs's existing `on_progress` signature) using `duration_secs` known from probe
execution_order: sequential (dep: impl-004b)

### task_id: impl-004d
title: Swap core render.rs from ffmpeg subprocess to encode FFI
specialist_type: impl
files_to_touch: crates/core/src/render.rs
estimated_diff_lines: 100
acceptance_criteria:
  - `render_export()` calls the FFI wrapper instead of spawning `ffmpeg`
  - `RenderError`/`RenderOutcome` shape preserved for callers (`ui`'s export queue doesn't need to change)
  - `parse_progress_line` and its tests are removed (no more `-progress pipe:1` text to parse)
execution_order: sequential (dep: impl-004c)

### task_id: test-002
title: Tests for encode FFI + swapped render.rs
specialist_type: test
files_to_touch: crates/avbridge/tests/encode.rs, crates/core/tests/render.rs
estimated_diff_lines: 100
acceptance_criteria:
  - Covers: successful render (probe the output, check duration/codec match), cancellation mid-render, loudness of output within tolerance of target
execution_order: sequential (dep: impl-004d)

### task_id: docs-002
title: Document encode FFI path
specialist_type: docs
files_to_touch: crates/avbridge/src/lib.rs (doc comments), CLAUDE.md, README.md
estimated_diff_lines: 50
acceptance_criteria:
  - CLAUDE.md/README.md updated: render no longer subprocess-based (mirrors docs-001's probe update)
execution_order: sequential (dep: test-002)

---

### task_id: chore-002
title: Add gstreamer-rs dependency + verify pkg-config link
specialist_type: impl
files_to_touch: Cargo.toml, crates/core/Cargo.toml (or new crates/oca-preview), .cargo/config.toml (env for PKG_CONFIG_PATH if needed)
estimated_diff_lines: 60
acceptance_criteria:
  - `cargo build` succeeds with `gstreamer = "0.2x"` (gstreamer-rs) added
  - `gst::init()` callable from a throwaway test without panicking
execution_order: sequential (dep: GStreamer mingw devel install finishing + link smoke-test, tracked outside this file)

---

### task_id: impl-005
title: Minimal GStreamer preview pipeline (independent of UI thread)
specialist_type: impl
files_to_touch: TBD once chore-002 lands
estimated_diff_lines: unknown
acceptance_criteria:
  - NOT YET BROKEN DOWN — blocked on chore-002 landing first (need to confirm playbin vs
    manual pipeline, sink choice for egui texture upload, before sizing this task)
execution_order: sequential (dep: chore-002)
