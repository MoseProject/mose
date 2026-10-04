//! Bush is Mose's standalone, editor-first preview engine.
//!
//! This crate intentionally has no dependency on `core`, UI crates, GStreamer, or FFmpeg. Its
//! first primitive models the scheduling contract shared by every interactive engine command:
//! pending work is replaceable, and the worker receives only the latest editor intent.

mod audio_block;
mod audio_mixer;
mod audio_playback;
mod audio_sink;
mod audio_source;
mod blend8;
mod blur;
mod byte_cache;
mod cpu_renderer;
mod decode_source;
mod deflicker;
#[cfg(feature = "cpal")]
mod device_sink;
mod engine;
mod frame_mailbox;
mod glitch;
#[cfg(feature = "gpu")]
mod gpu_blur;
#[cfg(feature = "gpu")]
mod gpu_renderer;
mod k_weighting;
mod keyframes;
mod latest_intent;
mod layer_mods;
mod matte;
mod player;
mod preview_worker;
mod scene;
mod scene_scheduler;
mod scene_versions;
mod source_variants;
mod stabilizer;
mod time_stretch;
mod transport;
mod video_frame;

pub use audio_block::{AudioBlock, AudioBlockError};
pub use audio_mixer::{AudioMixer, MixError, MixInput};
pub use audio_playback::{AudioPlayer, AudioPlayerError, AudioRenderError, AudioRenderer};
pub use audio_sink::{AudioSink, FakeSink, SinkError};
pub use audio_source::{AudioSource, AudioSourceError};
pub use avbridge::DecodePolicy;
pub use blur::{gaussian_blur, sharpen, MAX_BLUR_SIGMA};
pub use byte_cache::{ByteCache, CacheError, CacheInsert};
pub use cpu_renderer::{
    BlendMode, CpuRenderer, CropRect, LayerEffects, LayerFrame, LayerTransform, RenderError,
    Renderer,
};
pub use decode_source::{DecodeSource, DecodeSourceError, SourceInfo, MAX_SEEK_SEARCH_FRAMES};
pub use deflicker::Deflicker;
#[cfg(feature = "cpal")]
pub use device_sink::{DeviceSink, DeviceSinkError};
pub use engine::{
    Engine, EngineConfig, EngineError, EngineEvent, EngineRequest, EngineStats, LatencySummary,
    RendererChoice, MAX_CANVAS_PIXELS,
};
pub use frame_mailbox::{FrameMailbox, PublishedFrame};
pub use glitch::{glitch, shake_offset};
#[cfg(feature = "gpu")]
pub use gpu_renderer::{GpuError, GpuRenderer};
pub use k_weighting::{lufs, KWeighting};
pub use keyframes::{
    Animation, Easing, Fade, Keyframe, KeyframeError, KeyframeTrack, MAX_GAIN_DB, MIN_GAIN_DB,
};
pub use latest_intent::{Generation, LatestIntent, PendingIntent};
pub use layer_mods::{ChromaKey, LayerMask, Lut3D, LutError, MaskShape};
pub use player::{Player, PlayerError, PlayerEvent};
pub use preview_worker::{
    PreviewCommand, PreviewWorker, WorkerFailure, WorkerOperation, WorkerSubmitError,
};
pub use scene::{
    Contributor, ContributorContent, ContributorId, MatteSource, MediaSource, Placement,
    PrivacyBlur, RasterOverlay, SceneError, SceneSnapshot, SourceRange,
};
pub use scene_scheduler::{SceneScheduler, ScheduleError, ScheduledScene, Submission};
pub use scene_versions::{invalidation_scope, InvalidationScope, SceneVersions, Version};
pub use source_variants::{
    PreviewQuality, SourceVariant, SourceVariantError, SourceVariantKind, SourceVariants,
};
pub use stabilizer::{Stabilizer, STABILIZE_ZOOM};
pub use time_stretch::{TimeStretcher, MAX_SPEED, MIN_SPEED};
pub use transport::{PlaybackState, Transport, TransportError};
pub use video_frame::{PixelFormat, VideoFrame, VideoFrameError};
