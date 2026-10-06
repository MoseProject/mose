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

use crate::project::Sequence;

const DEFAULT_CAPACITY: usize = 100;
const DEFAULT_BYTE_BUDGET: usize = 100 * 1024 * 1024;

/// Count the serialized dynamic payload without allocating a JSON tree or output buffer.
/// A conservative multiplier also accounts for struct/container overhead. This is an estimate,
/// not allocator telemetry; one oversized newest step is retained to preserve undo availability.
fn weight(sequence: &Sequence) -> usize {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut counter = Counter(0);
    if serde_json::to_writer(&mut counter, sequence).is_err() {
        return usize::MAX;
    }
    counter
        .0
        .saturating_mul(4)
        .saturating_add(std::mem::size_of::<Sequence>())
}

struct Snapshot {
    sequence: Sequence,
    bytes: usize,
}

/// Two bounded snapshot stacks. Dynamic payloads count towards a shared estimated byte budget.
/// The newest step alone may exceed the budget, so even an oversized edit remains undoable.
pub struct UndoStack {
    capacity: usize,
    byte_budget: usize,
    retained_bytes: u128,
    undo: std::collections::VecDeque<Snapshot>,
    redo: std::collections::VecDeque<Snapshot>,
}

impl UndoStack {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self::with_limits(capacity, DEFAULT_BYTE_BUDGET)
    }

    /// Selects history depth and a shared estimated byte budget for undo and redo.
    pub fn with_limits(capacity: usize, byte_budget: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            byte_budget,
            retained_bytes: 0,
            undo: Default::default(),
            redo: Default::default(),
        }
    }

    fn trim(&mut self, newest_is_undo: bool) {
        while self.undo.len() + self.redo.len() > 1
            && (self.undo.len() + self.redo.len() > self.capacity
                || self.retained_bytes > self.byte_budget as u128)
        {
            let oldest = if newest_is_undo {
                self.redo.pop_front().or_else(|| self.undo.pop_front())
            } else {
                self.undo.pop_front().or_else(|| self.redo.pop_front())
            };
            if let Some(oldest) = oldest {
                self.retained_bytes = self.retained_bytes.saturating_sub(oldest.bytes as u128);
            }
        }
    }

    fn snapshot(&mut self, sequence: Sequence) -> Snapshot {
        let bytes = weight(&sequence);
        self.retained_bytes = self.retained_bytes.saturating_add(bytes as u128);
        Snapshot { sequence, bytes }
    }

    /// Records the pre-edit state and discards the previous redo branch.
    pub fn push(&mut self, before: Sequence) {
        for snapshot in self.redo.drain(..) {
            self.retained_bytes = self.retained_bytes.saturating_sub(snapshot.bytes as u128);
        }
        let snapshot = self.snapshot(before);
        self.undo.push_back(snapshot);
        self.trim(true);
    }

    pub fn undo(&mut self, current: Sequence) -> Option<Sequence> {
        let previous = self.undo.pop_back()?;
        self.retained_bytes = self.retained_bytes.saturating_sub(previous.bytes as u128);
        let snapshot = self.snapshot(current);
        self.redo.push_back(snapshot);
        self.trim(false);
        Some(previous.sequence)
    }

    pub fn redo(&mut self, current: Sequence) -> Option<Sequence> {
        let next = self.redo.pop_back()?;
        self.retained_bytes = self.retained_bytes.saturating_sub(next.bytes as u128);
        let snapshot = self.snapshot(current);
        self.undo.push_back(snapshot);
        self.trim(true);
        Some(next.sequence)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.retained_bytes = 0;
    }
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new()
    }
}
