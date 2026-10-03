//! Bush's engine facade: `submit` the newest scene + playhead, `poll` the newest frame.
//!
//! One engine thread owns every native decode session, the decoded-frame cache and the
//! compositor. Submitting replaces any unstarted request and cancels the in-flight one at the
//! next decode boundary, so a burst of scrubs costs one frame, not one frame per scrub. Every
//! queue is a single latest-value slot and every cache is byte-bounded.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::{
    ByteCache, ContributorContent, ContributorId, CpuRenderer, DecodeSource, Generation,
    LatestIntent, LayerFrame, Placement, SceneSnapshot, VideoFrame,
};

/// Fixed engine limits. Every field is validated by [`Engine::new`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineConfig {
    pub canvas_width: u32,
    pub canvas_height: u32,
    /// Maximum simultaneously open decoder sessions; the least recently used one is closed.
    pub max_open_sources: usize,
    /// Byte budget for decoded source frames retained between requests.
    pub frame_cache_bytes: usize,
}

/// Everything needed to produce one frame. Overlay pixels are supplied by the caller, already
/// shaped/rasterized (Bush never lays out text).
#[derive(Clone, Debug)]
pub struct EngineRequest {
    pub scene: SceneSnapshot,
    pub playhead_secs: f64,
    pub overlays: Vec<(ContributorId, Arc<VideoFrame>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum EngineEvent {
    Frame {
        generation: Generation,
        frame: VideoFrame,
    },
    Failed {
        generation: Generation,
        message: String,
    },
}

impl EngineEvent {
    pub const fn generation(&self) -> Generation {
        match self {
            Self::Frame { generation, .. } | Self::Failed { generation, .. } => *generation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EngineError {
    InvalidConfig,
    InvalidPlayhead,
    Stopped,
    GenerationExhausted,
}

/// Monotonic counters for diagnostics. Contains no paths or media metadata.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EngineStats {
    pub submitted: u64,
    pub superseded: u64,
    pub frames_rendered: u64,
    pub failures: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cancelled: u64,
}

#[derive(Debug, Default)]
struct Counters {
    submitted: AtomicU64,
    superseded: AtomicU64,
    frames_rendered: AtomicU64,
    failures: AtomicU64,
    cache_hits: AtomicU64,
    cache_misses: AtomicU64,
    cancelled: AtomicU64,
}

struct Pending {
    request: EngineRequest,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inbox {
    pending: LatestIntent<Pending>,
    inflight: Option<Arc<AtomicBool>>,
    next_generation: u64,
}

struct Shared {
    inbox: Mutex<Inbox>,
    wake: Condvar,
    events: Mutex<VecDeque<EngineEvent>>,
    event_wake: Condvar,
    stopped: AtomicBool,
    counters: Counters,
}

/// The preview engine. Dropping it shuts the engine thread down and releases native sessions.
pub struct Engine {
    shared: Arc<Shared>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let renderer = canvas_budget(&config)
            .and_then(|bytes| CpuRenderer::new(bytes).ok())
            .ok_or(EngineError::InvalidConfig)?;
        let cache =
            ByteCache::new(config.frame_cache_bytes).map_err(|_| EngineError::InvalidConfig)?;
        if config.max_open_sources == 0 {
            return Err(EngineError::InvalidConfig);
        }

        let shared = Arc::new(Shared {
            inbox: Mutex::new(Inbox::default()),
            wake: Condvar::new(),
            events: Mutex::new(VecDeque::new()),
            event_wake: Condvar::new(),
            stopped: AtomicBool::new(false),
            counters: Counters::default(),
        });
        let worker_shared = Arc::clone(&shared);
        let join = thread::spawn(move || {
            EngineThread {
                shared: worker_shared,
                config,
                renderer,
                cache,
                sources: Vec::new(),
            }
            .run();
        });
        Ok(Self {
            shared,
            join: Mutex::new(Some(join)),
        })
    }

    /// Replaces any unstarted request, cancels the in-flight one, and returns the new
    /// generation. Older generations' events are discarded.
    pub fn submit(&self, request: EngineRequest) -> Result<Generation, EngineError> {
        if !request.playhead_secs.is_finite() || request.playhead_secs < 0.0 {
            return Err(EngineError::InvalidPlayhead);
        }
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(EngineError::Stopped);
        }
        let mut inbox = self.shared.inbox.lock().expect("engine inbox poisoned");
        let next = inbox
            .next_generation
            .checked_add(1)
            .ok_or(EngineError::GenerationExhausted)?;
        inbox.next_generation = next;
        let generation = Generation(next);
        if let Some(inflight) = &inbox.inflight {
            inflight.store(true, Ordering::Release);
        }
        let replaced = inbox.pending.replace(
            generation,
            Pending {
                request,
                cancel: Arc::new(AtomicBool::new(false)),
            },
        );
        drop(inbox);
        self.shared
            .counters
            .submitted
            .fetch_add(1, Ordering::Relaxed);
        if replaced.is_some() {
            self.shared
                .counters
                .superseded
                .fetch_add(1, Ordering::Relaxed);
        }
        self.shared.wake.notify_one();
        Ok(generation)
    }

    /// The newest event, if any; superseded generations were already discarded.
    pub fn poll(&self) -> Option<EngineEvent> {
        self.shared
            .events
            .lock()
            .expect("engine events poisoned")
            .pop_front()
    }

    /// Blocks up to `timeout` for the next event.
    pub fn wait(&self, timeout: Duration) -> Option<EngineEvent> {
        let events = self.shared.events.lock().expect("engine events poisoned");
        let (mut events, _) = self
            .shared
            .event_wake
            .wait_timeout_while(events, timeout, |events| {
                events.is_empty() && !self.shared.stopped.load(Ordering::Acquire)
            })
            .expect("engine events poisoned");
        events.pop_front()
    }

    pub fn stats(&self) -> EngineStats {
        let counters = &self.shared.counters;
        EngineStats {
            submitted: counters.submitted.load(Ordering::Relaxed),
            superseded: counters.superseded.load(Ordering::Relaxed),
            frames_rendered: counters.frames_rendered.load(Ordering::Relaxed),
            failures: counters.failures.load(Ordering::Relaxed),
            cache_hits: counters.cache_hits.load(Ordering::Relaxed),
            cache_misses: counters.cache_misses.load(Ordering::Relaxed),
            cancelled: counters.cancelled.load(Ordering::Relaxed),
        }
    }

    /// Cancels in-flight work, stops the engine thread and joins it, releasing every native
    /// decode session. Idempotent.
    pub fn shutdown(&self) {
        if self.shared.stopped.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(inflight) = &self
            .shared
            .inbox
            .lock()
            .expect("engine inbox poisoned")
            .inflight
        {
            inflight.store(true, Ordering::Release);
        }
        self.shared.wake.notify_all();
        self.shared.event_wake.notify_all();
        if let Some(join) = self.join.lock().expect("engine join poisoned").take() {
            let _ = join.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn canvas_budget(config: &EngineConfig) -> Option<usize> {
    if config.canvas_width == 0 || config.canvas_height == 0 {
        return None;
    }
    (config.canvas_width as usize)
        .checked_mul(config.canvas_height as usize)?
        .checked_mul(4)
}

type CacheKey = (PathBuf, u64, i64);

struct EngineThread {
    shared: Arc<Shared>,
    config: EngineConfig,
    renderer: CpuRenderer,
    cache: ByteCache<CacheKey, Arc<VideoFrame>>,
    /// Open decoders, least recently used first.
    sources: Vec<((PathBuf, u64), DecodeSource)>,
}

enum RenderOutcome {
    Frame(VideoFrame),
    Cancelled,
    Failed(String),
}

impl EngineThread {
    fn run(mut self) {
        while let Some((generation, pending)) = self.take_next() {
            let outcome = self.render(&pending.request, &pending.cancel);
            self.publish(generation, outcome);
        }
    }

    fn take_next(&self) -> Option<(Generation, Pending)> {
        let mut inbox = self.shared.inbox.lock().expect("engine inbox poisoned");
        loop {
            if self.shared.stopped.load(Ordering::Acquire) {
                return None;
            }
            if let Some(next) = inbox.pending.take() {
                inbox.inflight = Some(Arc::clone(&next.value.cancel));
                return Some((next.generation, next.value));
            }
            inbox = self.shared.wake.wait(inbox).expect("engine inbox poisoned");
        }
    }

    fn publish(&self, generation: Generation, outcome: RenderOutcome) {
        let counters = &self.shared.counters;
        let event = match outcome {
            RenderOutcome::Cancelled => {
                counters.cancelled.fetch_add(1, Ordering::Relaxed);
                return;
            }
            RenderOutcome::Frame(frame) => {
                counters.frames_rendered.fetch_add(1, Ordering::Relaxed);
                EngineEvent::Frame { generation, frame }
            }
            RenderOutcome::Failed(message) => {
                counters.failures.fetch_add(1, Ordering::Relaxed);
                EngineEvent::Failed {
                    generation,
                    message,
                }
            }
        };
        // Drop the event if a newer request already superseded this generation.
        let newest = self
            .shared
            .inbox
            .lock()
            .expect("engine inbox poisoned")
            .next_generation;
        if generation.0 != newest {
            return;
        }
        let mut events = self.shared.events.lock().expect("engine events poisoned");
        events.clear();
        events.push_back(event);
        drop(events);
        self.shared.event_wake.notify_all();
    }

    fn render(&mut self, request: &EngineRequest, cancel: &AtomicBool) -> RenderOutcome {
        let playhead = request.playhead_secs;
        let mut layers: Vec<(Arc<VideoFrame>, Placement)> = Vec::new();
        for contributor in request.scene.visible_contributors() {
            if !contributor.is_active_at(playhead) {
                continue;
            }
            let frame = match &contributor.content {
                ContributorContent::Media(source) => {
                    let Some(source_time) = contributor.source_time_at(playhead) else {
                        continue;
                    };
                    match self.source_frame(&source.path, source.version.0, source_time, cancel) {
                        Ok(Some(frame)) => frame,
                        Ok(None) => return RenderOutcome::Cancelled,
                        Err(message) => return RenderOutcome::Failed(message),
                    }
                }
                ContributorContent::RasterOverlay(_) => {
                    match request
                        .overlays
                        .iter()
                        .find(|(id, _)| *id == contributor.id)
                    {
                        Some((_, frame)) => Arc::clone(frame),
                        None => {
                            return RenderOutcome::Failed(format!(
                                "overlay {} has no raster data",
                                contributor.id.0
                            ))
                        }
                    }
                }
            };
            layers.push((frame, contributor.placement));
        }
        if cancel.load(Ordering::Acquire) {
            return RenderOutcome::Cancelled;
        }

        let layer_frames: Vec<LayerFrame<'_>> = layers
            .iter()
            .map(|(frame, placement)| LayerFrame {
                frame,
                x: placement.x,
                y: placement.y,
                opacity: placement.opacity,
                transform: placement.transform,
                blend: placement.blend,
            })
            .collect();
        match self.renderer.render(
            self.config.canvas_width,
            self.config.canvas_height,
            playhead,
            &layer_frames,
        ) {
            Ok(frame) => RenderOutcome::Frame(frame),
            Err(error) => RenderOutcome::Failed(format!("render failed: {error:?}")),
        }
    }

    /// The decoded frame for `source_time`, from the cache or a (reused) decode session.
    /// `Ok(None)` means the decode was cancelled.
    fn source_frame(
        &mut self,
        path: &Path,
        version: u64,
        source_time: f64,
        cancel: &AtomicBool,
    ) -> Result<Option<Arc<VideoFrame>>, String> {
        let key: CacheKey = (
            path.to_path_buf(),
            version,
            (source_time * 1e6).round() as i64,
        );
        if let Some(frame) = self.cache.get(&key) {
            self.shared
                .counters
                .cache_hits
                .fetch_add(1, Ordering::Relaxed);
            return Ok(Some(Arc::clone(frame)));
        }
        self.shared
            .counters
            .cache_misses
            .fetch_add(1, Ordering::Relaxed);

        let slot = self.open_source(path, version)?;
        let frame = match self.sources[slot].1.frame_at(source_time, cancel) {
            Ok(Some(frame)) => frame,
            Ok(None) if cancel.load(Ordering::Acquire) => return Ok(None),
            Ok(None) => return Err(format!("{} produced no frame", display_name(path))),
            Err(error) => return Err(format!("{}: {error:?}", display_name(path))),
        };
        let bytes = frame.retained_bytes();
        let _ = self.cache.insert(key, Arc::clone(&frame), bytes);
        Ok(Some(frame))
    }

    /// Index of the open session for `(path, version)`, opening and evicting as needed. The
    /// returned session is moved to the most-recently-used end.
    fn open_source(&mut self, path: &Path, version: u64) -> Result<usize, String> {
        if let Some(index) = self
            .sources
            .iter()
            .position(|((candidate, candidate_version), _)| {
                candidate == path && *candidate_version == version
            })
        {
            let entry = self.sources.remove(index);
            self.sources.push(entry);
            return Ok(self.sources.len() - 1);
        }
        let source = DecodeSource::open(path)
            .map_err(|error| format!("{}: {error:?}", display_name(path)))?;
        while self.sources.len() >= self.config.max_open_sources {
            self.sources.remove(0);
        }
        self.sources.push(((path.to_path_buf(), version), source));
        Ok(self.sources.len() - 1)
    }
}

/// File name only: diagnostics must not leak full paths.
fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "<media>".to_string())
}
