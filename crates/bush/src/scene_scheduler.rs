//! Editor-first scheduling of immutable scene snapshots.

use crate::{
    invalidation_scope, Generation, InvalidationScope, LatestIntent, SceneSnapshot, SceneVersions,
};

/// In-memory scheduler for Bush's future worker thread.
///
/// It owns no decoder or renderer. Its contract is deliberately useful on its own: a rapid
/// sequence of Mose edits retains only the latest unstarted scene, and its invalidation scope is
/// calculated relative to the last scene that was actually applied.
#[derive(Debug, Default)]
pub struct SceneScheduler {
    next_generation: u64,
    applied_versions: Option<SceneVersions>,
    pending: LatestIntent<SceneSnapshot>,
}

/// The result of accepting a new scene.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Submission {
    pub generation: Generation,
    pub superseded: Option<Generation>,
}

/// Work ready for a Bush worker to apply.
#[derive(Debug, PartialEq)]
pub struct ScheduledScene {
    pub generation: Generation,
    pub invalidation: InvalidationScope,
    pub scene: SceneSnapshot,
}

/// Failure to assign a unique scene generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ScheduleError {
    GenerationExhausted,
}

impl SceneScheduler {
    /// Creates an empty scheduler whose first accepted scene has generation one.
    pub const fn new() -> Self {
        Self {
            next_generation: 0,
            applied_versions: None,
            pending: LatestIntent::new(),
        }
    }

    /// Queues `scene`, replacing pending scene work that has not started.
    pub fn submit(&mut self, scene: SceneSnapshot) -> Result<Submission, ScheduleError> {
        let next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(ScheduleError::GenerationExhausted)?;
        self.next_generation = next_generation;
        let generation = Generation(next_generation);
        let superseded = self
            .pending
            .replace(generation, scene)
            .map(|old| old.generation);

        Ok(Submission {
            generation,
            superseded,
        })
    }

    /// Takes the newest scene and computes its work against the last applied scene.
    pub fn take_next(&mut self) -> Option<ScheduledScene> {
        let pending = self.pending.take()?;
        let invalidation = self
            .applied_versions
            .map(|current| invalidation_scope(current, pending.value.versions))
            .unwrap_or(InvalidationScope::ReopenSources);

        Some(ScheduledScene {
            generation: pending.generation,
            invalidation,
            scene: pending.value,
        })
    }

    /// Records a scene only after its worker-side resources have been applied successfully.
    pub fn mark_applied(&mut self, scene: &SceneSnapshot) {
        self.applied_versions = Some(scene.versions);
    }

    /// Returns whether a scene is waiting to start.
    pub const fn is_idle(&self) -> bool {
        self.pending.is_empty()
    }
}
