# QA sweeps

Property-style suites that hunt for bugs with random but reproducible input (fixed seeds, so a
failure prints the seed and step that reproduce it). Add a case here when a bug is found by hand:
it is cheaper to extend a sweep than to write a one-off test.

| suite | what it checks |
|---|---|
| `crates/core/tests/it/qa_track_invariants_test.rs` | random split, ripple, lift, roll, slide, trim and move on tracks: lengths, gaps, unique ids, finite times, a refused edit changes nothing, an edit that did nothing reports `false` |
| `crates/core/tests/it/qa_files_and_text_test.rs` | project files cut short or with a byte changed never panic, SRT entries stay well-formed, silence gaps are ordered, disjoint and long enough |
| `apps/desktop/src-tauri/src/commands/qa_edit_sequences_test.rs` | random sequences of the real timeline commands: the timeline stays well-formed, undo and redo walk through exactly the states the edits made, an edit that changed nothing leaves no undo step |
| `apps/desktop/tests/qaTimelineGeometry.test.ts` | ruler labels and ticks for any zoom and scroll, coordinate round trips, zoom anchoring |

Run them: `cargo test -p core --test it qa_`, `cargo test -p desktop qa_`,
`cd apps/desktop && npx vitest run tests/qaTimelineGeometry.test.ts`.

## Bugs these found

- A clip edited with a NaN or infinite time was corrupted (every comparison with NaN is false, so
  the guards let it through). Edits now refuse non-finite input.
- `slip_clip`, `ripple_delete_range` and `lift_range` recorded an empty undo step when they changed
  nothing (slip by zero, a range past the end of the content). They now report `false`.
- An SRT caption with a blank line in its text became two entries; a negative duration produced a
  caption ending before it started; sorting captions with a NaN start could panic.
- The ruler label of a time just under a second read `0:01.100` instead of `0:02`.
- `rulerTicks` with a zero zoom tried to build an array of unbounded length.

Seen and left as it is: splitting a clip copies its incoming transition to the second half
(`split_clip_at_keeps_transition_on_both_halves` pins this on purpose), so a Fade appears again at
the cut. Worth a product decision.
