# Undo/Redo (`ROADMAP.md` P0 item 1)

## Done

`crates/core/src/model/undo.rs` — `UndoStack`: two bounded `Vec<Sequence>` stacks (undo/redo),
snapshot-based (not command-pattern — see the module's own doc for why `Sequence: Clone` makes
this the correct reuse-before-build choice per `RULES.md`). `push`/`undo`/`redo`/`can_undo`/
`can_redo`/`clear`. Unit tested (`crates/core/tests/undo_test.rs`, 7 tests): fresh-stack state,
undo restores the pushed snapshot, redo restores what undo moved away from, a new push after
undo clears the redo branch (real editor semantics), undo-beyond-history returns `None` without
touching redo, capacity bounds the stack by dropping the oldest entry, `clear` drops both.

## Wired

Items 1-2 and 4-7 below are done. `Project::active_sequence()`/`active_sequence_mut()`
accessors added (`crates/core/src/model/project.rs`, next to `timeline()`/`timeline_mut()`).
`App::undo_stack: avcore::undo::UndoStack` (`crates/ui/src/app/mod.rs`, next to
`selected_clip_id`). `App::push_undo_snapshot()`/`undo()`/`redo()`/`can_undo()`/`can_redo()`
added next to `reset_sequence_context`. `Undo`/`Redo` added to `BindableAction`/`KeyBindings`
(default `Ctrl+Z`/`Ctrl+Y`), configurable in Preferences same as the other five. Toolbar's
pre-existing dead `↺`/`↻` buttons (`screens/editor/mod.rs`) now call `undo()`/`redo()`,
enabled state from `can_undo()`/`can_redo()`. `undo_stack.clear()` called from
`reset_sequence_context` (covers `select_sequence`/`add_sequence`/`duplicate_sequence`/
`delete_sequence`) and from `App::open_project`. Unit tests in `app_test.rs`
(`undo_after_split_at_playhead_restores_the_unsplit_clip`, `undo_is_a_no_op_with_empty_history`,
`selecting_a_sequence_clears_undo_history_from_the_previous_one`) plus the 7 `core` tests —
**all verified passing** on a machine with GStreamer configured (`cargo test -p core --test
undo_test`, `cargo test -p ui undo`), closing the "not verified" gap from the primitive-only
pass.

## Call sites (item 3) — done

`push_undo_snapshot()` is wired into `timeline_ops.rs`'s single-shot mutations
(`add_asset_to_timeline[_at]`, `split_at_playhead`, `delete_selected_clip`,
`paste_clip_at_playhead`, `merge_into_composite`, `add_video_track`, `add_text_track`/
`add_text_clip`, `add_shape_track`/`add_shape_clip`/`finish_drawing_custom_shape`) and into
`App::confirm_text_color_edit` (`crates/ui/src/app/color.rs`, the text-color modal's confirm
button). The timeline strip's trim/move drags (`screens/editor/timeline_panel.rs`) push once on
`drag_started()` — not inside `trim_clip_start`/`trim_clip_end`/`move_clip`/
`move_clip_with_group`/`move_clip_to_track` themselves, since those run every frame of a drag.

**Effect-property setters** (`crates/ui/src/app/clip_props.rs` — gain, crop, mask, color/
vignette/blur/etc., keyframes) and their sliders in `screens/editor/properties_panel.rs`
(~30 call sites, all funneling through `App::with_selected_clip_mut`, plus `text_clip_properties`/
`shape_clip_properties`'s own single write-back each) are covered too, via a different mechanism
than `drag_started()`: those setters are reached through several layers of `bool`-returning
helpers (`components::property_section` etc.) that don't thread an `egui::Response` back to the
caller, so there's no `Response` to call `drag_started()` on at the one place that could use it.
Instead, `App::push_undo_snapshot_for_drag()` (next to `push_undo_snapshot`) tracks a stateful
`undo_drag_active: bool` flag: the first setter call since the flag was last cleared pushes a
snapshot and sets it; every subsequent call this same continuous drag re-fires (once per frame,
same as `drag_started()` would see) is a no-op. `screens::editor::show` clears the flag once per
frame whenever `ui.input(|i| i.pointer.any_down())` is `false` — i.e. once the drag actually
ends — so the next drag (or an instant click, which clears the flag again the very next frame
regardless) starts a fresh snapshot. Wired into the three shared mutation points:
`App::with_selected_clip_mut` (`timeline_ops.rs` — covers every `set_selected_clip_*` setter,
including all keyframe editors), and the `if changed { ... }` write-back in both
`text_clip_properties` and `shape_clip_properties` (`properties_panel.rs`).

Unit-tested in `app_test.rs`: `a_continuous_effect_property_drag_pushes_only_one_undo_step`
(three simulated same-drag frames of `set_selected_clip_gain` collapse into one undo step that
restores the pre-drag value, not just the last frame's increment) and
`releasing_the_pointer_starts_a_fresh_undo_step_for_the_next_drag` (calling
`end_undo_drag_tracking_if_pointer_released(false)` between two setter calls splits them into
two separate undo steps).

`toggle_track_visibility` stays unwired — a display toggle, not timeline content, same category
`panel_layout` writes are excluded for.

## Design note: why per-sequence, not per-project

Snapshotting the whole `Project` (media library, all sequences, display metadata) on every
timeline edit would work but is heavier than necessary and conflates two different kinds of
"undo" a user would expect to be separate — undoing a clip move on sequence tab A shouldn't be
affected by having imported a new asset while working on tab B in between. Scoping to the
active `Sequence` keeps undo history meaningful per timeline, matches how every surveyed editor
(`matrix/competitor-parity.md`) scopes undo to the active edit context, not the whole project.

---

[← back to spec/INDEX.md](../INDEX.md)
