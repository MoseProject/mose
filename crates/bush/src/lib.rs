//! Bush is Mose's standalone, editor-first preview engine.
//!
//! This crate intentionally has no dependency on `core`, UI crates, GStreamer, or FFmpeg. Its
//! first primitive models the scheduling contract shared by every interactive engine command:
//! pending work is replaceable, and the worker receives only the latest editor intent.

mod audio_block;
mod audio_mixer;
mod byte_cache;
mod cpu_renderer;
mod decode_source;
mod frame_mailbox;
mod latest_intent;
mod preview_worker;
mod scene;
mod scene_scheduler;
mod scene_versions;
mod source_variants;
mod transport;
mod video_frame;

pub use audio_block::{AudioBlock, AudioBlockError};
pub use audio_mixer::{AudioMixer, MixError, MixInput};
pub use byte_cache::{ByteCache, CacheError, CacheInsert};
pub use cpu_renderer::{BlendMode, CpuRenderer, CropRect, LayerFrame, LayerTransform, RenderError};
pub use decode_source::{DecodeSource, DecodeSourceError, SourceInfo, MAX_SEEK_SEARCH_FRAMES};
pub use frame_mailbox::{FrameMailbox, PublishedFrame};
pub use latest_intent::{Generation, LatestIntent, PendingIntent};
pub use preview_worker::{
    PreviewCommand, PreviewWorker, WorkerFailure, WorkerOperation, WorkerSubmitError,
};
pub use scene::{
    Contributor, ContributorContent, ContributorId, MediaSource, Placement, RasterOverlay,
    SceneError, SceneSnapshot, SourceRange,
};
pub use scene_scheduler::{SceneScheduler, ScheduleError, ScheduledScene, Submission};
pub use scene_versions::{invalidation_scope, InvalidationScope, SceneVersions, Version};
pub use source_variants::{
    PreviewQuality, SourceVariant, SourceVariantError, SourceVariantKind, SourceVariants,
};
pub use transport::{PlaybackState, Transport, TransportError};
pub use video_frame::{PixelFormat, VideoFrame, VideoFrameError};
