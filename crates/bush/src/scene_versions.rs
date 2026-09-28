//! Version-based invalidation for editor preview state.
//!
//! Mose maps its mutable timeline state into these three versions when it eventually submits a
//! scene to Bush. The mapping keeps a playhead move from reopening decoders, while still making a
//! source replacement take precedence over every cheaper update.

/// A monotonically increasing opaque version supplied by the scene producer.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub struct Version(pub u64);

/// Versions for the independent causes of preview recomputation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SceneVersions {
    /// Source identity, trim, decode format, or proxy/original selection changed.
    pub source: Version,
    /// Contributors, effects, overlays, audio routing, or layer ordering changed.
    pub scene: Version,
    /// Playhead, transport, or viewport changed without changing decoded scene content.
    pub presentation: Version,
}

/// The minimum engine work required to move from one scene revision to another.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidationScope {
    /// All decoded and composed state is still valid.
    Reuse,
    /// Preserve sources and scene resources; only schedule a new presentation frame.
    UpdatePresentation,
    /// Preserve unaffected source sessions; rebuild contributor/composition state.
    RebuildScene,
    /// Reopen or reselect source sessions before rebuilding the scene.
    ReopenSources,
}

/// Returns the cheapest valid invalidation path for `next`.
///
/// Version values are opaque equality tokens, not counters interpreted by Bush. If several
/// versions differ, the most expensive necessary scope wins.
pub fn invalidation_scope(current: SceneVersions, next: SceneVersions) -> InvalidationScope {
    if current.source != next.source {
        InvalidationScope::ReopenSources
    } else if current.scene != next.scene {
        InvalidationScope::RebuildScene
    } else if current.presentation != next.presentation {
        InvalidationScope::UpdatePresentation
    } else {
        InvalidationScope::Reuse
    }
}
