//! Bounded, latest-value scheduling for interactive editor work.
//!
//! A worker owns this type and takes the newest pending command before it starts an expensive
//! decode, seek, or composition operation. It deliberately has no synchronization primitive:
//! cross-thread transport belongs at Bush's worker boundary, while this type keeps replacement
//! semantics deterministic and cheap to test.

/// Identifies one editor intent generation.
///
/// Results emitted for another generation must be ignored by the caller; [`LatestIntent`] only
/// governs pending work and cannot cancel work already taken by a worker.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Generation(pub u64);

/// A pending value paired with the scene generation that requested it.
#[derive(Debug, Eq, PartialEq)]
pub struct PendingIntent<T> {
    pub generation: Generation,
    pub value: T,
}

/// A one-item queue where newer editor intent replaces unstarted work.
///
/// This is the core backpressure rule for Bush preview scheduling. It stays bounded at one item:
/// stale scrubs or live edits are returned to the caller on replacement instead of accumulating.
#[derive(Debug, Default)]
pub struct LatestIntent<T> {
    pending: Option<PendingIntent<T>>,
}

impl<T> LatestIntent<T> {
    /// Creates an empty queue.
    pub const fn new() -> Self {
        Self { pending: None }
    }

    /// Replaces unstarted work and returns the obsolete intent, if any.
    pub fn replace(&mut self, generation: Generation, value: T) -> Option<PendingIntent<T>> {
        self.pending.replace(PendingIntent { generation, value })
    }

    /// Takes the newest pending intent for execution.
    pub fn take(&mut self) -> Option<PendingIntent<T>> {
        self.pending.take()
    }

    /// Returns whether no intent is waiting to start.
    pub const fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
}
