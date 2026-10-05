# Timeline — Domain and Interaction Rules

Ground truth against `crates/ui/src/screens/editor/timeline_panel/{mod.rs,draw.rs,snap.rs}` and
`crates/ui/src/app/{timeline_ops,timeline_track_ops,timeline_trim_ops,timeline_clipboard_ops}.rs`.
The timeline is genuinely a specialized interactive canvas here, not a generic widget list —
treat it that way.

## How it's actually built

`timeline_panel/mod.rs` (~1050 lines) owns the whole panel: ruler, per-track header row,
per-clip layout/interaction/hit-testing, and dispatch of edit requests into `App` methods.
`draw.rs` holds pure-drawing helpers (filmstrip tiling, waveform painting, marker ticks,
playhead line). `snap.rs` holds `ClipDrag` and the snap-target math. Rendering is
**custom-painted**: `ui.allocate_exact_size`/`ui.interact` are used only for hit-testing regions;
actual visuals go through `painter.rect_filled`/`rect_stroke`/`text`/`image`/`line_segment`/
`convex_polygon` calls, not nested egui widgets or a custom `egui::Widget` impl. When adding a
new timeline visual, follow the established shape: a pure `draw_x(painter, rect, data,
px_per_sec) -> Option<InteractionResult>` function in `draw.rs`, called from `mod.rs`'s per-frame
loop — this is exactly how markers were added.

## Coordinate systems

Keep domain time separate from screen coordinates — real and enforced:
- **Time**: `f64` seconds, stable domain unit (`ClipInstance::start_secs`, `Timeline::
  playhead_secs`, `Marker::position_secs`).
- **Zoom**: `App::timeline_px_per_sec: f32`, clamped `[0.5, 60.0]`
  (`MIN_PX_PER_SEC`/`MAX_PX_PER_SEC` in `app/mod.rs`), updated by ctrl+scroll `zoom_delta()`.
- **Screen position**: forward conversion is `x = track_rect.left() + clip.start_secs as f32 *
  px_per_sec` (repeated per clip kind — video/text/shape — at their respective draw sites);
  reverse is `secs = ((pos.x - rect.left()) / px_per_sec) as f64` (used at the ruler, trim edges,
  asset-drop handling). There is no separate horizontal-scroll-offset field — x=0 is always
  track-left; only vertical scrolling exists (`egui::ScrollArea::vertical()` around the track
  stack).

Never persist pixel positions as editing data — confirmed, only `f64` seconds are ever stored on
`ClipInstance`/`Marker`/`Timeline`.

## Core interactions — real implementation shape

- **Select / multi-select**: ctrl-click toggles into `multi_select_requests`, applied
  post-loop via `app.toggle_multi_select`. **No marquee/rubber-band select exists** — do not
  assume it when designing a feature that references it; it would be new work.
- **Drag/move**: per-clip body uses `Sense::click_and_drag()`. While dragging, a `ClipDrag`
  (transient, `snap.rs`) is pushed to a local `Vec<ClipDrag>` — **not** applied to the project
  immediately. After the whole per-frame immutable-borrow loop over tracks/clips ends, the
  buffered drags are applied via `app.move_clip_with_group`/`move_clip_to_track`/`slip_clip`/
  `slide_clip`. This immutable-borrow-then-apply shape is why timeline interaction code is
  structured as "collect requests during the draw loop, apply after it" — mutating `Project`
  mid-loop isn't possible given the borrow the loop holds.
- **Trim**: separate `Sense::drag()` regions for left/right edge strips; positions collect into
  `trim_requests: Vec<(u64, TrimEdge)>`, applied post-loop, tool-aware (Ripple vs. Roll dispatch)
  into `timeline_trim_ops.rs` functions (`trim_clip_start`/`trim_clip_end`/
  `ripple_trim_clip_start`/`roll_edit_clip`/etc).
- **Split**: right-click context menu sets a `split_at_playhead_requested` flag, applied
  post-loop via `app.split_at_playhead()` (`timeline_ops.rs`).
- **Snap**: `snap.rs::snap_to_nearest`/`snap_move_start`, threshold `SNAP_THRESHOLD_PX = 8.0`
  converted to seconds at the current zoom. Alt suppresses it. Snap targets = every clip edge +
  every marker position, collected once per frame before the draw loop — markers use **no
  special-cased snap logic**, they're folded into the same target vector clips use. Trim drags
  additionally snap to waveform low-energy points (`waveform_snap_points_for_clip`).
- **Commit boundary**: `drag_started_this_frame` is set on drag start; after the per-frame loop,
  `app.push_undo_snapshot()` is called **once**, strictly after all buffered requests
  (`ClipDrag`, `trim_requests`, etc.) are drained and applied. The screen-space live preview
  (`body_response.drag_delta()`) is genuinely separate from the committed mutation — the "do not
  mutate persistent project state continuously" rule from the generic guidance is real and
  already followed; extend it, don't work around it.

## Zoom

Preserve context — real behavior: ctrl+scroll adjusts `timeline_px_per_sec` via `zoom_delta()`,
clamped to `[0.5, 60.0]`. At low zoom, clips still render at full detail (no LOD simplification
implemented — see `engineering/TIMELINE_PERFORMANCE.md`'s "known gap" section).

## Markers

`Marker { id, position_secs, label, kind: MarkerKind, completed }`,
`MarkerKind { Standard, ToDo, Chapter, Highlight }`. `marker_kind_color` maps kind to color;
`draw_marker_ticks` draws a small ruler triangle per marker, culled if outside the visible rect,
returns an optional seek position on click. This is the most recent addition to the timeline and
the clearest template to copy for a similar ruler-level feature.

## Thumbnails and waveforms

- **Waveform**: `crates/core/src/av/waveform.rs::generate_waveform` produces peaks stored directly
  on `MediaAsset.waveform_peaks` (persisted) — no separate runtime cache struct; `draw_waveform`
  reads it straight off the asset.
- **Thumbnails**: generated in `crates/ui/src/app/import.rs` (no dedicated `core` module), cached
  in `App.thumbnail_state.thumbnail_textures: HashMap<(asset_id, frame_index),
  egui::TextureHandle>`, bounded by `THUMBNAIL_CACHE_CAPACITY` with LRU-style eviction
  (`evict_thumbnail_textures`). The draw loop only reads/touches cache keys; actual
  request/eviction calls happen post-loop (`app.touch_thumbnails`/`app.request_thumbnail`),
  matching the same "collect during draw, mutate after" shape as clip drag/trim.

## Strengths (preserve these)

- Custom painting keeps clip rendering cheap and flexible — don't replace it with nested widgets.
- The immutable-borrow-loop-then-apply pattern cleanly separates read (draw) from write (commit)
  every frame, without a bespoke command queue.
- Thumbnail tile virtualization already exists *within* a clip's filmstrip
  (`visible_tile_range` intersecting `painter.clip_rect()`) — a real, working LOD-adjacent
  pattern to extend rather than reinvent.

## Weaknesses / risks — see `engineering/TIMELINE_PERFORMANCE.md`

Track/clip-level viewport culling across the whole timeline does not exist yet (every track,
every clip, every text/shape clip is iterated and painted every frame regardless of visibility).
This is a real, confirmed gap, not a hypothetical risk — factor it into any change that adds
per-clip work to the draw loop, and don't assume culling exists elsewhere just because tile-level
culling exists inside one clip's filmstrip.
