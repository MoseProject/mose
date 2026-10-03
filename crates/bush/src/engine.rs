//! Bush's engine facade: `submit` the newest scene + playhead, `poll` the newest frame.
//!
//! One engine thread owns every native decode session, the decoded-frame cache and the
//! compositor. Submitting replaces any unstarted request and cancels the in-flight one at the
//! next decode boundary, so a burst of scrubs costs one frame, not one frame per scrub. Every
//! queue is a single latest-value slot and every cache is byte-bounded.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::{
    ByteCache, ContributorContent, ContributorId, CpuRenderer, DecodePolicy, DecodeSource,
    Generation, LatestIntent, LayerFrame, Placement, Renderer, SceneSnapshot, VideoFrame,
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
    /// Software decode, or hardware with software fallback. Hardware is opt-in: measure it on
    /// the target media before enabling it.
    pub decode_policy: DecodePolicy,
    /// Which compositor to use. A GPU request without the `gpu` feature, or without a usable
    /// adapter, falls back to the CPU reference; see [`Engine::renderer_name`].
    pub renderer: RendererChoice,
}

/// Compositor selection for [`EngineConfig`].
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RendererChoice {
    /// The deterministic, threaded CPU compositor.
    #[default]
    Cpu,
    /// The wgpu compute compositor when available (feature `gpu` and an adapter), else CPU.
    GpuWithCpuFallback,
}

/// Largest canvas the engine accepts, in pixels (8K UHD is 33.2 M). Bounds the compositor's
/// per-frame allocation regardless of what a caller asks for.
pub const MAX_CANVAS_PIXELS: usize = 8192 * 4608;

/// Number of most recent request latencies retained for percentile reporting.
const LATENCY_WINDOW: usize = 1024;

/// Percentile summary of submit-to-published-frame latency over the most recent requests.
/// Cancelled and superseded requests are not samples: they never produced a frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LatencySummary {
    pub samples: usize,
    pub p50_micros: u64,
    pub p95_micros: u64,
    pub p99_micros: u64,
    pub max_micros: u64,
}

impl LatencySummary {
    fn from_samples(window: &VecDeque<u64>) -> Self {
        if window.is_empty() {
            return Self::default();
        }
        let mut sorted: Vec<u64> = window.iter().copied().collect();
        sorted.sort_unstable();
        let at = |percentile: usize| {
            let index = (sorted.len() * percentile).div_ceil(100).max(1) - 1;
            sorted[index.min(sorted.len() - 1)]
        };
        Self {
            samples: sorted.len(),
            p50_micros: at(50),
            p95_micros: at(95),
            p99_micros: at(99),
            max_micros: *sorted.last().unwrap_or(&0),
        }
    }
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
    submitted_at: Instant,
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
    latencies: Mutex<VecDeque<u64>>,
    event_wake: Condvar,
    stopped: AtomicBool,
    counters: Counters,
}

/// The preview engine. Dropping it shuts the engine thread down and releases native sessions.
pub struct Engine {
    shared: Arc<Shared>,
    join: Mutex<Option<JoinHandle<()>>>,
    renderer_name: String,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    pub fn new(config: EngineConfig) -> Result<Self, EngineError> {
        let max_bytes = canvas_budget(&config)
            .filter(|bytes| *bytes <= MAX_CANVAS_PIXELS * 4)
            .ok_or(EngineError::InvalidConfig)?;
        let renderer = create_renderer(config.renderer, max_bytes)?;
        let renderer_name = renderer.name();
        let cache =
            ByteCache::new(config.frame_cache_bytes).map_err(|_| EngineError::InvalidConfig)?;
        if config.max_open_sources == 0 {
            return Err(EngineError::InvalidConfig);
        }

        let shared = Arc::new(Shared {
            inbox: Mutex::new(Inbox::default()),
            wake: Condvar::new(),
            events: Mutex::new(VecDeque::new()),
            latencies: Mutex::new(VecDeque::new()),
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
                deflickers: HashMap::new(),
            }
            .run();
        });
        Ok(Self {
            shared,
            join: Mutex::new(Some(join)),
            renderer_name,
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
                submitted_at: Instant::now(),
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

    /// Submit-to-frame latency percentiles over the most recent published frames.
    pub fn latency(&self) -> LatencySummary {
        LatencySummary::from_samples(
            &self
                .shared
                .latencies
                .lock()
                .expect("engine latencies poisoned"),
        )
    }

    /// The compositor in use: `cpu`, or `gpu:<adapter name>`.
    pub fn renderer_name(&self) -> &str {
        &self.renderer_name
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

fn create_renderer(
    choice: RendererChoice,
    max_bytes: usize,
) -> Result<Box<dyn Renderer>, EngineError> {
    #[cfg(feature = "gpu")]
    if choice == RendererChoice::GpuWithCpuFallback {
        if let Ok(gpu) = crate::GpuRenderer::new(max_bytes) {
            return Ok(Box::new(gpu));
        }
    }
    #[cfg(not(feature = "gpu"))]
    let _ = choice;
    CpuRenderer::new(max_bytes)
        .map(|cpu| Box::new(cpu) as Box<dyn Renderer>)
        .map_err(|_| EngineError::InvalidConfig)
}

fn canvas_budget(config: &EngineConfig) -> Option<usize> {
    if config.canvas_width == 0 || config.canvas_height == 0 {
        return None;
    }
    (config.canvas_width as usize)
        .checked_mul(config.canvas_height as usize)?
        .checked_mul(4)
}

/// Blur radius of the unsharp mask behind Placement::sharpen, in source pixels.
const SHARPEN_SIGMA: f32 = 1.5;

type CacheKey = (PathBuf, u64, i64);

type DecodeResult = Result<Option<Arc<VideoFrame>>, crate::DecodeSourceError>;

/// One source frame a render needs: which media, and the source time to present.
struct MediaNeed<'a> {
    path: &'a Path,
    version: u64,
    time: f64,
}

impl MediaNeed<'_> {
    fn cache_key(&self) -> CacheKey {
        (
            self.path.to_path_buf(),
            self.version,
            (self.time * 1e6).round() as i64,
        )
    }
}

struct EngineThread {
    shared: Arc<Shared>,
    config: EngineConfig,
    renderer: Box<dyn Renderer>,
    cache: ByteCache<CacheKey, Arc<VideoFrame>>,
    /// Open decoders, least recently used first.
    sources: Vec<((PathBuf, u64), DecodeSource)>,
    /// Temporal deflicker state per contributor; only present while the effect is enabled.
    deflickers: HashMap<ContributorId, crate::Deflicker>,
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
            let submitted_at = pending.submitted_at;
            self.publish(generation, outcome, submitted_at);
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

    fn publish(&self, generation: Generation, outcome: RenderOutcome, submitted_at: Instant) {
        let counters = &self.shared.counters;
        let event = match outcome {
            RenderOutcome::Cancelled => {
                counters.cancelled.fetch_add(1, Ordering::Relaxed);
                return;
            }
            RenderOutcome::Frame(frame) => {
                counters.frames_rendered.fetch_add(1, Ordering::Relaxed);
                let micros = u64::try_from(submitted_at.elapsed().as_micros()).unwrap_or(u64::MAX);
                let mut latencies = self
                    .shared
                    .latencies
                    .lock()
                    .expect("engine latencies poisoned");
                if latencies.len() == LATENCY_WINDOW {
                    latencies.pop_front();
                }
                latencies.push_back(micros);
                drop(latencies);
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
        // Decode every needed source frame up front so independent sources run in parallel.
        let needs: Vec<MediaNeed<'_>> = request
            .scene
            .visible_contributors()
            .filter(|contributor| contributor.is_active_at(playhead))
            .filter_map(|contributor| match &contributor.content {
                ContributorContent::Media(source) => {
                    contributor.source_time_at(playhead).map(|time| MediaNeed {
                        path: &source.path,
                        version: source.version.0,
                        time,
                    })
                }
                ContributorContent::RasterOverlay(_) => None,
            })
            .collect();
        let mut fetched = match self.fetch_media(&needs, cancel) {
            Ok(Some(frames)) => frames.into_iter(),
            Ok(None) => return RenderOutcome::Cancelled,
            Err(message) => return RenderOutcome::Failed(message),
        };
        let mut layers: Vec<(Arc<VideoFrame>, Placement)> = Vec::new();
        for contributor in request.scene.visible_contributors() {
            if !contributor.is_active_at(playhead) {
                continue;
            }
            let frame = match &contributor.content {
                ContributorContent::Media(_) => {
                    if contributor.source_time_at(playhead).is_none() {
                        continue;
                    }
                    match fetched.next() {
                        Some(frame) => frame,
                        None => return RenderOutcome::Failed("media plan out of sync".into()),
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
            let placement = contributor.placement_at(playhead);
            let frame = match (placement.deflicker, &contributor.content) {
                (Some(smoothing), ContributorContent::Media(_)) => {
                    match self.deflicker_for(contributor.id, smoothing) {
                        Some(deflicker) => Arc::new(deflicker.process(&frame)),
                        None => frame,
                    }
                }
                _ => {
                    self.deflickers.remove(&contributor.id);
                    frame
                }
            };
            // Neighbourhood effects work on the frame before compositing, so the CPU and GPU
            // renderers share them.
            let frame = if matches!(contributor.content, ContributorContent::Media(_))
                && (placement.blur_sigma > 0.0 || placement.sharpen > 0.0 || placement.glitch > 0.0)
            {
                let mut processed = frame;
                if placement.blur_sigma > 0.0 {
                    processed = Arc::new(crate::gaussian_blur(&processed, placement.blur_sigma));
                }
                if placement.sharpen > 0.0 {
                    processed =
                        Arc::new(crate::sharpen(&processed, placement.sharpen, SHARPEN_SIGMA));
                }
                if placement.glitch > 0.0 {
                    processed = Arc::new(crate::glitch(&processed, placement.glitch, playhead));
                }
                processed
            } else {
                frame
            };
            layers.push((frame, placement));
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
                effects: placement.effects,
                lut: placement.lut.as_deref(),
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

    fn deflicker_for(
        &mut self,
        id: ContributorId,
        smoothing: f32,
    ) -> Option<&mut crate::Deflicker> {
        if let std::collections::hash_map::Entry::Vacant(entry) = self.deflickers.entry(id) {
            entry.insert(crate::Deflicker::new(smoothing)?);
        }
        self.deflickers.get_mut(&id)
    }

    /// Source frames for every need, in order: from the cache where possible, otherwise decoded
    /// now. Misses on different sources decode on parallel threads (multicam, layered clips);
    /// misses on one source decode in sequence because a session is single-threaded.
    /// `Ok(None)` means the work was cancelled.
    fn fetch_media(
        &mut self,
        needs: &[MediaNeed<'_>],
        cancel: &AtomicBool,
    ) -> Result<Option<Vec<Arc<VideoFrame>>>, String> {
        let mut frames: Vec<Option<Arc<VideoFrame>>> = vec![None; needs.len()];
        let mut misses: Vec<usize> = Vec::new();
        for (index, need) in needs.iter().enumerate() {
            if let Some(frame) = self.cache.get(&need.cache_key()) {
                self.shared
                    .counters
                    .cache_hits
                    .fetch_add(1, Ordering::Relaxed);
                frames[index] = Some(Arc::clone(frame));
            } else {
                self.shared
                    .counters
                    .cache_misses
                    .fetch_add(1, Ordering::Relaxed);
                misses.push(index);
            }
        }

        if !misses.is_empty() {
            let decoded = self.decode_misses(needs, &misses, cancel)?;
            for (index, result) in decoded {
                let need = &needs[index];
                let frame = match result {
                    Ok(Some(frame)) => frame,
                    Ok(None) if cancel.load(Ordering::Acquire) => return Ok(None),
                    Ok(None) => {
                        return Err(format!("{} produced no frame", display_name(need.path)))
                    }
                    Err(error) => return Err(format!("{}: {error:?}", display_name(need.path))),
                };
                let bytes = frame.retained_bytes();
                let _ = self
                    .cache
                    .insert(need.cache_key(), Arc::clone(&frame), bytes);
                frames[index] = Some(frame);
            }
        }
        Ok(Some(
            frames
                .into_iter()
                .map(|frame| frame.expect("every need was resolved"))
                .collect(),
        ))
    }

    /// Decodes the frames for `misses`, grouped per source session, in parallel across sessions.
    fn decode_misses(
        &mut self,
        needs: &[MediaNeed<'_>],
        misses: &[usize],
        cancel: &AtomicBool,
    ) -> Result<Vec<(usize, DecodeResult)>, String> {
        let mut distinct: Vec<(&Path, u64)> = Vec::new();
        for &index in misses {
            let key = (needs[index].path, needs[index].version);
            if !distinct.contains(&key) {
                distinct.push(key);
            }
        }
        // More distinct sources than session slots: decode one at a time, reopening as needed.
        if distinct.len() > self.config.max_open_sources {
            let mut results = Vec::with_capacity(misses.len());
            for &index in misses {
                let need = &needs[index];
                let slot = self.open_source(need.path, need.version)?;
                results.push((index, self.sources[slot].1.frame_at(need.time, cancel)));
            }
            return Ok(results);
        }
        for (path, version) in &distinct {
            self.open_source(path, *version)?;
        }

        let mut jobs: Vec<(usize, Vec<usize>)> = Vec::new();
        for &index in misses {
            let need = &needs[index];
            let position = self
                .sources
                .iter()
                .position(|((path, version), _)| path == need.path && *version == need.version)
                .ok_or_else(|| "decode session was evicted mid-frame".to_string())?;
            match jobs
                .iter_mut()
                .find(|(candidate, _)| *candidate == position)
            {
                Some((_, indices)) => indices.push(index),
                None => jobs.push((position, vec![index])),
            }
        }

        if jobs.len() == 1 {
            let (position, indices) = &jobs[0];
            let source = &mut self.sources[*position].1;
            return Ok(indices
                .iter()
                .map(|&index| (index, source.frame_at(needs[index].time, cancel)))
                .collect());
        }

        let mut slots: Vec<Option<&mut DecodeSource>> = self
            .sources
            .iter_mut()
            .map(|(_, source)| Some(source))
            .collect();
        let mut results = Vec::with_capacity(misses.len());
        std::thread::scope(|scope| {
            let handles: Vec<_> = jobs
                .iter()
                .map(|(position, indices)| {
                    let source = slots[*position].take().expect("one job per session");
                    scope.spawn(move || {
                        indices
                            .iter()
                            .map(|&index| (index, source.frame_at(needs[index].time, cancel)))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            for handle in handles {
                results.extend(handle.join().expect("decode thread panicked"));
            }
        });
        Ok(results)
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
        let source = DecodeSource::open_with_policy(path, self.config.decode_policy)
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
