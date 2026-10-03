# Preview Pipeline (GStreamer, removed)

> **Historical:** the items below were proven against the GStreamer pipeline, which was removed. The live preview is Bush; its checklist is [bush.md](bush.md).

`core/src/preview.rs` — the real-time playback/scrub pipeline, separate from the avbridge
export path. Detail: `matrix/changelog.md` Fase 4's compositing/hardware-decode entries.

- [x] Single-clip playback (`Preview::open`) — real audio sink (`autoaudiosink`), retries with
      `fakesink` once if no usable audio device.
- [x] Multi-track compositing (`Preview::open_composited`) — raw `gst::Pipeline` +
      `compositor`, one branch per overlay clip, dynamic pad linking.
- [x] Per-branch independent seeking + playback rate (`seek_composited`, not one pipeline-wide
      seek — each branch is a different source file with its own trim/time base).
- [x] Audio mixing across every visible contributor (`audiomixer`) — embedded audio from
      background/overlay branches + audio-only tracks, each own
      `queue`→`audioconvert`→`audioresample`→`volume` chain.
- [x] Matte compositing preview (`alphacombine`, gst-plugins-bad `codecalpha`) — required
      explicit identical `colorimetry=bt601` on both capsfilters (discovered empirically, not
      documented anywhere obvious upstream).
- [x] Static overlay branches (text/shape) via `appsrc ! imagefreeze` — no pad-probe/seek entry
      needed, `imagefreeze` repeats the latest buffer.
- [x] Hardware-accelerated decode (VideoToolbox/NVDEC/Quick Sync/VAAPI) — process-global rank
      boost for hardware decoder factories, falls back to software once on a failed preroll.
      `PrefsState::preview_hardware_decode`, default on.
- [x] Speed (`speed_factor`) honored via GStreamer full rate-seek (`seek_with_rate`), not just
      export.

## Verification posture (real, not assumed)

Every pipeline shape above is proven against a *live* GStreamer pipeline on the dev machine
(`crates/core/tests/preview_test.rs`) — links, prerolls, decodes multiple frames without error.
**Not verified anywhere in this group:** actual rendered-pixel correctness (position/scale/
chroma-key placement lands where intended) — tests prove the pipeline doesn't crash, not that
pixels are geometrically correct. Treat pixel-level correctness as unverified until eyeballed
on a real display.

---

[← back to spec/INDEX.md](../INDEX.md)
