# Async preview worker

> **Historical:** describes the removed GStreamer `PreviewWorker`. The preview now runs on Bush (`crates/bush`), see [gstreamer-removal-plan.md](gstreamer-removal-plan.md).

## Goal

Keep every GStreamer operation off eframe's UI thread while preserving immediate timeline
feedback. A single worker owns `avcore::preview::Preview`; the UI owns timeline state, egui
textures, and the last displayed CPU frame.

## Current boundary

`ui::app::preview` currently opens pipelines, seeks, reads `VideoFrame`s, and toggles playback
on the UI thread. `preview_live_updates` also writes live GStreamer properties directly. The
worker replaces all of those direct accesses. `VideoFrame` is safe to transport because it owns
only dimensions and packed RGBA bytes. `egui::TextureHandle` never leaves the UI thread.

## Protocol

The UI creates a monotonically increasing preview generation for every session/seek state. It
updates the timeline playhead synchronously, then places the desired seek in a single-slot
latest-value mailbox. New seeks overwrite pending ones. Session replacement and playback state
also retain only the latest desired state; shutdown uses an atomic flag and a capacity-one wakeup.
Live property updates coalesce per property in a fixed-size mailbox, preserving unrelated
properties without allowing an unbounded queue.

The worker drains structural commands first, then takes one latest seek without holding mailbox
locks during GStreamer calls. It owns the pipeline, performs open/seek/decode/live effect updates,
and publishes at most one latest frame. Every result includes the generation that produced it.
The UI drops mismatched generations before texture upload. The worker also publishes a bounded
`Ready`/`Failed` status, so availability is tied to the matching generation rather than merely
to the existence of a thread.

## Invariants

- UI code never calls `Preview::{open, seek, current_frame, play, pause, set_live_*}`.
- A mailbox contains at most one pending continuous request and a frame slot at most one decoded
  frame; obsolete work cannot grow without bound.
- A lock is never held over GStreamer, decoding, texture upload, or CPU image processing.
- Session replacement and shutdown cannot be overwritten by a scrub request.
- The worker reports open/seek failures as structured UI events and never panics the UI.

## Migration order

1. Add worker lifecycle, generation, bounded latest-value mailboxes, and observable counters.
2. Move session opening, play/pause, seeks, and frame polling to the worker.
3. Route all `preview_live_updates` calls through that worker.
4. Retain UI-side LUT/vignette/glitch/deflicker/scopes initially for output parity, then profile
   them independently before moving any CPU work.
5. Add real-pipeline tests for rapid seeks, clip switches, stale-frame rejection, errors, and
   shutdown. Record release-build mouse-to-frame median and P95 latency.

## Frame correctness

Generation prevents an old worker result from replacing newer UI intent, but it does not prove
that a post-seek sample has the requested media timestamp. The worker must use GStreamer's flush
seek semantics and expose/validate sample PTS or pipeline position before publishing a frame when
precise final-seek correctness is required.
