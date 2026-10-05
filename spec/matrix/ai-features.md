# AI Features

Background removal, auto-reframe, motion tracking, text-to-speech, subtitles/Whisper. All
models ship under `resources/models/` — no runtime downloader anywhere in this group. Detail:
`matrix/changelog.md` Fase 4 (background removal / auto-reframe / motion tracking / TTS
sub-sections).

- [x] **AI background removal** — MODNet (ONNX, Apache-2.0) via `ort`, per-frame alpha matte
      (`avcore::background_removal::segment_person`). Matte generation samples across a clip's
      trim range (`SAMPLES_PER_SEC = 2.0`, coarser than clip fps — export compositor tolerates
      by holding last known frame), encodes to H.264 (`matte_encode.c`), export- and
      preview-wired (`alphacombine`/`alphamerge`). Confirmed working end-to-end (real inference).
- [x] **Auto-reframe** (static crop only, not animated) — UltraFace RFB-320 (ONNX, MIT) face
      detection, falls back to centered crop with a toast if no face found.
      `compute_reframe_crop()` pure geometry, fully unit tested. Confirmed working end-to-end.
- [x] **Motion tracking** — plain fixed-template block matching (SAD against first frame,
      no ML), converts tracked path into `position_keyframes` delta. Region now user-editable:
      numeric entry (center/width/height/search-radius) *and* drag-on-preview picker. Fully
      unit tested including off-center starting region.
- [x] **Text-to-speech** — own pt-BR phonemizer (`text_to_speech/pt_br.rs`) + Piper VITS ONNX (pt-BR `faber` voice,
      MIT). Confirmed real synthesis, non-silent output.
- [x] **Whisper subtitles** — Base model bundled, live word-highlight timing shared between
      preview/export layout.
- [x] Standalone `.srt` export (`avcore::export_srt`) — separate from the embedded-overlay
      subtitle path, which happens on every export regardless.
- [x] **Scene-cut detection for chapter markers** (D4, `architecture/differentiators.md`,
      `ROADMAP.md` P3 item 15) — no ML, same "no ML" family as motion tracking: mean absolute
      luma difference between consecutive sampled frames (`avcore::detect_scene_cuts`), reusing
      `motion_tracking::rgba_to_gray` for the grayscale conversion rather than a second copy.
      Sampling itself reuses `avcore::FrameSampler`, same primitive motion tracking's own
      background thread already uses (`spec/architecture/performance-and-caching.md` §5).
      Detected cuts become non-destructive `MarkerKind::Chapter` markers (P2 item 9) rather than
      a separate review modal — the existing Timeline Index panel's rename/delete already is the
      review step. A plain-text `H:MM:SS Label` chapter-list export (YouTube's own format) is a
      thin filter over the marker list.
- [x] **Highlight detection from audio spikes** (D2, `architecture/differentiators.md`,
      `ROADMAP.md` P3 item 17) — no ML, same "correlate a signal against a threshold" family as
      D1/D5. Needed a real "which track is which" answer first (`avcore::timeline::AudioRole` —
      Unspecified/GameAudio/Mic/Music, user-set via the timeline track header) since
      `Project`/`Timeline` had no such distinction; see `ROADMAP.md` item 17's preserved
      investigation note. `avcore::highlight_detection::clip_amplitude_samples` maps a clip's
      waveform into timeline-relative amplitude samples (D1's `clip_silence_gaps` mapping, every
      bucket instead of only quiet runs); `detect_highlight_candidates` correlates a
      `AudioRole::GameAudio` track's samples against a `AudioRole::Mic` track's onto a common
      coarse time grid and flags simultaneous spikes. Non-destructive `MarkerKind::Highlight`
      markers, same review-via-Timeline-Index shape as D4's chapters. Unblocks D6 (`ROADMAP.md`
      item 18), which was waiting on this.
- [x] **One-click shorts pack** (D6, `architecture/differentiators.md`, `ROADMAP.md` P3 item
      18) — ties D2's Highlight markers, `avcore::extract_timeline_window` (new `core` primitive:
      turns one `[start, end)` slice of a `Timeline` into its own standalone, rebased-to-zero
      `Timeline`, splitting straddling Video/Audio clips precisely via the existing
      `Track::split_clip_at` while only keeping Text/Shape overlays entirely inside the window),
      and the existing multi-job export queue into one batch action. Confirmed three real scope
      decisions with the user first (fixed window size around each highlight; reuse existing
      auto-reframe/transcription rather than running either pipeline fresh per short) — see
      `ROADMAP.md` item 18 for the full reasoning. `App::spawn_shorts_pack` resolves and queues
      one `ExportAspectRatio::Portrait` job per highlight, skipping (not erroring on) a window
      that resolves to zero clips.

## Known gaps

- [ ] Voice-clone TTS beyond the single bundled voice — explicitly out of scope for now, see
      `architecture/differentiators.md`.

---

[← back to spec/INDEX.md](../INDEX.md)
