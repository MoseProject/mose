//! Snapshot-based undo/redo for one [`crate::project::Sequence`]'s timeline edits
//! (`spec/ROADMAP.md` P0 item 1 — no undo/redo existed anywhere in the codebase before this).
//!
//! Snapshot-based, not command-pattern: `Sequence` (and everything it owns — `Timeline`,
//! `Track`, `ClipInstance`) already derives `Clone`/`Serialize`/`Deserialize` for `.ocproj`
//! persistence, so a full clone of the sequence *before* a mutation is a real, correct, and
//! immediately-reusable snapshot — no per-operation `Command` type needed for the dozens of
//! distinct mutation kinds (move/trim/split/effect-change/keyframe-edit/track-add/...). Matches
//! `spec/RULES.md`'s "reuse before building" rule: this reuses the exact same `Clone` derive
//! `.ocproj` save/load already depends on, rather than inventing a new diff/patch
//! representation.
//!
//! Scoped to one sequence, not the whole `Project` — undo/redo covers timeline *edits*, not
//! project-level changes (media import, project rename). [`UndoStack::clear`] should be called
//! on sequence switch: history from one sequence tab isn't meaningful applied to another.
//!
//! Deliberately not yet wired to any UI call site — this is the primitive only. Wiring
//! `UndoStack::push` into every `Timeline`/`Track`/`ClipInstance` mutation call site in
//! `ui/src/app/timeline_ops.rs`, plus `Ctrl+Z`/`Ctrl+Y` key bindings (same
//! `BindableAction`/`KeyBindings` pattern `AddOpacityMarker` already established), is the next
//! step — see `spec/architecture/undo-redo.md`.

use crate::project::Sequence;

const DEFAULT_CAPACITY: usize = 100;

/// Clips (of any kind) all the undo snapshots together may hold. A snapshot is a full copy of
/// the sequence, so a long history of a big timeline would otherwise grow without a bound that
/// means anything (a clip is several hundred bytes: this is on the order of 100 MB). Past it the
/// oldest steps are dropped, so small projects keep every one of their [`DEFAULT_CAPACITY`] steps
/// and very large ones keep fewer.
const CLIP_BUDGET: usize = 120_000;

/// How much memory a snapshot holds, in clips (one extra for the sequence itself).
fn weight(sequence: &Sequence) -> usize {
    1 + sequence
        .timeline
        .tracks
        .iter()
        .map(|track| track.clips.len() + track.text_clips.len() + track.shape_clips.len())
        .sum::<usize>()
}

/// Two bounded stacks of `Sequence` snapshots. `push` is called with the sequence's state
/// *before* a mutation is applied (the caller applies the mutation itself, immediately after);
/// `undo`/`redo` take the *current* state as their argument (so it can be pushed onto the other
/// stack) and return the state to restore.
pub struct UndoStack {
    capacity: usize,
    undo: Vec<Sequence>,
    redo: Vec<Sequence>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// Records `before` as a real undo point. Any pending redo history is discarded — a fresh
    /// edit after an undo invalidates the branch that redo would have replayed, same as every
    /// real editor's undo/redo semantics.
    pub fn push(&mut self, before: Sequence) {
        self.undo.push(before);
        self.redo.clear();
        let mut held: usize = self.undo.iter().map(weight).sum();
        // The newest step always stays, however big the sequence is.
        while self.undo.len() > 1 && (self.undo.len() > self.capacity || held > CLIP_BUDGET) {
            held -= weight(&self.undo.remove(0));
        }
    }

    /// Pops the most recent undo point, pushes `current` onto the redo stack, and returns the
    /// state the caller should now restore. `None` when there's nothing to undo.
    pub fn undo(&mut self, current: Sequence) -> Option<Sequence> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    /// The inverse of [`Self::undo`].
    pub fn redo(&mut self, current: Sequence) -> Option<Sequence> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Drops all history — call when switching to a different sequence/project. History from
    /// one sequence applied to another would restore the wrong timeline entirely.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}
