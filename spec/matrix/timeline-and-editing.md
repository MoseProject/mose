# Timeline & Editing

Project/timeline data model (`core/src/timeline.rs`, `project.rs`) plus the editing
interactions in `ui`. Detail: `features/request.md` Fase 3, `matrix/changelog.md` Fase 5/6's
sequence-management/copy-paste entries.

## Done

- [x] Custom timeline widget: per-frame filmstrip (zoom-adaptive, bounded — see
      `matrix/performance.md`) + synced waveform track.
- [x] Select / split / trim / drag between tracks.
- [x] Preview scrubbing synced to playhead.
- [x] Zoom (`Ctrl`+scroll).
- [x] Multiple sequence tabs per project, each own timeline + export defaults — full lifecycle:
      add/select/rename/duplicate/delete (confirmation modal)/reorder (drag or context menu).
      `Project::duplicate_sequence`/`remove_sequence`/`move_sequence` own the invariants (always
      ≥1 sequence, deletion selects nearest neighbor, reorder tracks by stable id).
- [x] Resizable UI panels (library/preview/properties/timeline), layout savable per-user or
      per-project (`PanelLayout`, `LayoutScope`).
- [x] Composite blocks (merge selected clips into one movable/cuttable unit) — toolbar action
      + context-menu action (`"Mesclar em bloco composto"`).
- [x] Copy/paste within and across sequence tabs — preserves composite-block membership
      (`App::copy_selected_clip` captures the whole `composite_id` group, not just the clicked
      clip).
- [x] Context menu mirrors toolbar shortcut actions.
- [x] Copy/paste of just formatting (`Ctrl+Shift+C`/`V`) — effects/config without duplicating
      the clip.
- [x] Multiple overlapping video/audio layers (not just sequential tracks) — base for overlays,
      PiP, layer templates.
- [x] Layer transform (position/size) — drag + resize handles on preview, or numeric entry;
      both stay in sync.
- [x] Layer templates — save a configured camera/webcam/background layer group, reapply to a
      new short asking only for source clips per layer.
- [x] **Undo/redo.** `core::undo::UndoStack` (snapshot-based) wired into `ui` — `Ctrl+Z`/`Ctrl+Y`,
      toolbar buttons, covers clip add/split/delete/cut/copy/paste, trim/move drags, track/text/
      shape-track add, composite merge, paste-formatting, text-color-modal confirm, and every
      effect-property slider/keyframe editor (drag-coalesced into one undo step per drag, not
      one per frame). See `architecture/undo-redo.md`, `ROADMAP.md` P0.
- [x] **Magnetic snap** while dragging — playhead, clip trim (either edge), clip body move
      (single or composite group) all snap to the nearest other clip's edge or the playhead
      (`Alt` to disable). Timeline markers aren't a target — no markers feature exists yet
      (P2 item 9). See `ROADMAP.md` P0.
      **Extended (D5, `ROADMAP.md` P3 item 16):** clip trim specifically (not body move) also
      snaps to waveform low-energy points — `waveform_snap_points_for_clip` reuses
      `avcore::clip_silence_gaps` (D1) as a "quiet moment finder" with a much shorter minimum
      gap (0.05s vs. D1's own 0.5s cuttable-gap threshold), so a dragged cut can land mid-pause
      instead of mid-word/mid-sound-effect.
- [x] **Review/comment markers (P2 item 9).** `avcore::timeline::Marker`/`MarkerKind`
      (Standard/ToDo/Chapter — Final Cut Pro's typed-marker model, not just a plain note; `ToDo`
      tracks a `completed` flag) on `Timeline::markers`, `#[serde(default)]` so an older-saved
      project still loads. `Timeline::add_marker`/`remove_marker`/`marker_mut`/`markers_sorted`
      own the id-assignment/mutation invariants. `ui`: a searchable Timeline Index panel
      (`App::show_timeline_index_panel`, toolbar's "🏷 Marcadores" toggle) — text search over
      labels, per-marker kind picker, ToDo-complete checkbox, click-timestamp-to-seek, inline
      label editing, add/remove. **Not done**: magnetic snap (P0 item 2) doesn't treat markers
      as a snap target yet — that doc's own note said "revisit when it lands," this is the
      revisit-later follow-up, not silently included here. No ruler tick-mark rendering on the
      timeline strip itself either — the Timeline Index panel is the only way to see/navigate
      markers today.
- [x] **Named trim modes: Ripple / Roll / Slip / Slide (P2 item 11).** Confirmed (2026-08-27):
      the existing trim (`ClipInstance::trim_start`/`trim_end`) and move (`Track::move_clip`)
      matched none of the four named tools — trimming an edge never shifted neighboring clips
      (no Ripple, no Roll), and there was no way to change which part of the source media shows
      without also moving the clip or changing its duration (no Slip). All four now real,
      distinct `EditorTool` toolbar modes:
      - *Ripple* — `Track::ripple_trim_start`/`ripple_trim_end` trim one edge, then shift every
        clip past the edited clip's own (pre-edit) `start_secs` by the same delta.
      - *Roll* — `Track::roll_edit` moves the shared boundary between a clip and its
        `next_clip_id` neighbor; both sides' trims are snapshotted first and rolled back
        atomically if either refuses, so a rejected roll never leaves a half-edited pair.
        Dragging either clip's edge at the seam produces the same edit
        (`App::roll_edit_from_start_edge` resolves the *previous* neighbor when the later
        clip's start edge was the one grabbed).
      - *Slip* — `ClipInstance::slip` shifts `source_in_secs`/`source_out_secs` together,
        leaving `start_secs`/duration untouched. Wired to a clip-body drag (not an edge drag)
        while the Slip tool is active.
      - *Slide* — `Track::slide_clip` moves a clip's `start_secs`, then asks its previous/next
        neighbors (by pre-move position) to absorb the move via their own `trim_end`/
        `trim_start`; also snapshotted and rolled back atomically on a refusal.
      All four are pure, unit-tested `core` methods (`timeline_test.rs` — id assignment/
      neighbor-walking, the happy path, and the atomic-rollback-on-refusal path for Roll and
      Slide) plus a thin `ui` wiring layer (`App::ripple_trim_clip_start`/`_end`,
      `roll_edit_clip`/`roll_edit_from_start_edge`, `slip_clip`, `slide_clip` in
      `timeline_ops.rs`) dispatched from `timeline_panel.rs`'s existing edge-drag/body-drag
      request handling based on `app.tool`. Verified via `cargo check -p core --tests`/
      `-p ui --tests` (types/borrows, no link — see `CLAUDE.md`'s build-environment notes);
      **not driven through a live/e2e build** — no visual confirmation that the drag
      interactions feel right in the actual running app, same caveat magnetic snap's own entry
      above already carries for UI-only interaction logic.
- [x] **Multicam editing (P2 item 10).** `avcore::timeline::MulticamGroup` — a `Timeline`
      sidecar (member `Video` track ids, program/active track id, per-track sync offset
      seconds), same non-invasive shape `Marker` uses; angles are ordinary `Video` tracks, no new
      `TrackKind`. `Timeline::add_multicam_group` hides every non-program member
      (`Track::visible = false`) so only the active angle renders/exports.
      `Timeline::switch_multicam_angle` splits the program track's clip at the given time
      (`Track::split_clip_at`) and retargets the new piece's `asset_id`/`source_in_secs`/
      `source_out_secs` to the target angle's own footage at the sync-offset-adjusted equivalent
      time — reuses existing split/retarget primitives, no new editing-model concept beyond the
      group record itself. Sync offsets come from `avcore::multicam_sync` (RMS-envelope cross-
      correlation over `avbridge::extract_pcm_16k_mono` PCM — audio-waveform sync only, not
      timecode). `App::create_multicam_group_from_video_tracks` (Editor toolbar's "Sync
      Multicam" button) builds the group from every `Video` track in the sequence;
      `App::switch_multicam_angle_at_playhead`, wired to number keys 1-9, does the switch.
      **Not done**: live multi-feed preview during scrub/playback — switching still only takes
      effect on the materialized timeline clips, not a real-time multicam monitor; the preview
      pipeline plays whichever one decoded source is active, same "no live preview effect yet"
      gap most other per-clip effects have. Verified via a real-execution scratch crate (`core`'s
      own test binary can't link in this sandbox — pre-existing ONNX Runtime gap — but
      `timeline.rs`/`multicam_sync.rs` have zero heavy deps) rather than `cargo check`-only.
- [x] **D1 — automatic silence/dead-air cut** (`architecture/differentiators.md`, `ROADMAP.md`
      P3 item 13). `Track::ripple_delete_range` is the general-purpose "ripple delete a timeline
      range" primitive this feature applies (split any clip straddling either boundary via the
      existing `split_clip_at`, drop what's fully inside, ripple the rest left) — reusable by
      any future feature needing the same shape, not D1-specific despite living on `Track`.
      Detection itself (`avcore::silence_detection`, built on `avcore::waveform` rather than
      `avcore::loudness` — see `ROADMAP.md` item 13 for why) is a `core`-only concern with no
      timeline-editing-model implications beyond the ripple-delete it feeds, so it isn't
      repeated here in full.

---

[← back to spec/INDEX.md](../INDEX.md)
- [x] Frame-accurate edits: `Project.frame_rate` (an exact fraction, 23.976 to 60) is picked in the
      timeline toolbar. While set, moves, trims, rolls, slides, range delete/lift and the
      playhead used by split all land on whole frame boundaries (`Project::snap_secs`). Unset
      keeps the old free behaviour. Time is still `f64` seconds on purpose: converting the
      ~2,000 `*_secs` uses to integer ticks was judged not worth breaking the file format.
      The project format is now version 4; version 3 files still open (the new field is the
      last one, so the upgrade appends `None`).
