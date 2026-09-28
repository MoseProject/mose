//! Bush is Mose's standalone, editor-first preview engine.
//!
//! This crate intentionally has no dependency on `core`, UI crates, GStreamer, or FFmpeg. Its
//! first primitive models the scheduling contract shared by every interactive engine command:
//! pending work is replaceable, and the worker receives only the latest editor intent.

mod byte_cache;
mod latest_intent;
mod scene;
mod scene_scheduler;
mod scene_versions;
mod transport;
mod video_frame;

pub use byte_cache::{ByteCache, CacheError, CacheInsert};
pub use latest_intent::{Generation, LatestIntent, PendingIntent};
pub use scene::{
    Contributor, ContributorContent, ContributorId, MediaSource, RasterOverlay, SceneError,
    SceneSnapshot, SourceRange,
};
pub use scene_scheduler::{SceneScheduler, ScheduleError, ScheduledScene, Submission};
pub use scene_versions::{invalidation_scope, InvalidationScope, SceneVersions, Version};
pub use transport::{PlaybackState, Transport, TransportError};
pub use video_frame::{PixelFormat, VideoFrame, VideoFrameError};
