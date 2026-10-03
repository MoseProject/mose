//! Real-time audio path: scene -> mixed blocks -> an [`AudioSink`].
//!
//! Everything here is headless. [`AudioRenderer`] turns a scene and a frame range into one mixed
//! block (decode, per-clip gain, timeline alignment, limiter); [`AudioPlayer`] keeps an
//! [`AudioSink`] topped up to a bounded buffer. A device sink implements [`AudioSink`]; tests use
//! [`FakeSink`], whose underflow and overrun accounting is deterministic.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::{
    AudioBlock, AudioBlockError, AudioMixer, AudioSource, AudioSourceError, ContributorContent,
    ContributorId, MixError, MixInput, SceneSnapshot, TimeStretcher, MAX_SPEED, MIN_SPEED,
};

/// Destination for mixed audio. Implemented by a device callback bridge in production.
pub trait AudioSink {
    /// Queues one block. Must not block; a full sink reports [`SinkError::Overrun`].
    fn write(&mut self, block: &AudioBlock) -> Result<(), SinkError>;
    /// Frames queued but not yet played.
    fn buffered_frames(&self) -> usize;
    /// Drops everything queued (seek, scene replacement).
    fn clear(&mut self);
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SinkError {
    /// The device was lost or the sink shut down; further writes will not succeed.
    Closed,
    Overrun {
        capacity_frames: usize,
        requested_frames: usize,
    },
}

/// Deterministic in-memory sink with a bounded queue and explicit underflow accounting.
#[derive(Debug)]
pub struct FakeSink {
    capacity_frames: usize,
    channels: u16,
    queued: VecDeque<f32>,
    played: Vec<f32>,
    underflows: u64,
    closed: bool,
}

impl FakeSink {
    pub fn new(capacity_frames: usize, channels: u16) -> Self {
        Self {
            capacity_frames,
            channels,
            queued: VecDeque::new(),
            played: Vec::new(),
            underflows: 0,
            closed: false,
        }
    }

    /// Simulates the device pulling `frames`. Missing frames are played as silence and counted
    /// as one underflow. Returns the frames that were real audio.
    pub fn consume(&mut self, frames: usize) -> usize {
        let wanted = frames * usize::from(self.channels);
        let available = self.queued.len().min(wanted);
        self.played.extend(self.queued.drain(..available));
        if available < wanted {
            self.underflows += 1;
            self.played
                .resize(self.played.len() + (wanted - available), 0.0);
        }
        available / usize::from(self.channels)
    }

    /// Simulates device loss: every later write fails with [`SinkError::Closed`].
    pub fn lose_device(&mut self) {
        self.closed = true;
    }

    pub fn underflows(&self) -> u64 {
        self.underflows
    }

    /// Every interleaved sample the simulated device has played, silence included.
    pub fn played(&self) -> &[f32] {
        &self.played
    }
}

impl AudioSink for FakeSink {
    fn write(&mut self, block: &AudioBlock) -> Result<(), SinkError> {
        if self.closed {
            return Err(SinkError::Closed);
        }
        if self.buffered_frames() + block.frame_count() > self.capacity_frames {
            return Err(SinkError::Overrun {
                capacity_frames: self.capacity_frames,
                requested_frames: block.frame_count(),
            });
        }
        self.queued.extend(block.samples().iter().copied());
        Ok(())
    }

    fn buffered_frames(&self) -> usize {
        self.queued.len() / usize::from(self.channels)
    }

    fn clear(&mut self) {
        self.queued.clear();
    }
}

#[derive(Debug)]
pub enum AudioRenderError {
    Source(AudioSourceError),
    Mix(MixError),
    Block(AudioBlockError),
    /// Audio is stretched pitch-preserving only for speeds in
    /// [`MIN_SPEED`](crate::MIN_SPEED)..=[`MAX_SPEED`](crate::MAX_SPEED); anything else is an
    /// explicit error rather than silently playing at the wrong pitch.
    UnsupportedSpeed(ContributorId),
    InvalidConfig,
}

impl From<AudioSourceError> for AudioRenderError {
    fn from(error: AudioSourceError) -> Self {
        Self::Source(error)
    }
}

impl From<MixError> for AudioRenderError {
    fn from(error: MixError) -> Self {
        Self::Mix(error)
    }
}

impl From<AudioBlockError> for AudioRenderError {
    fn from(error: AudioBlockError) -> Self {
        Self::Block(error)
    }
}

type SourceKey = (PathBuf, u64);

/// Mixes a scene's audible contributors into fixed-format blocks.
#[derive(Debug)]
pub struct AudioRenderer {
    sample_rate: u32,
    channels: u16,
    max_block_frames: usize,
    max_open_sources: usize,
    mixer: AudioMixer,
    /// Open sources, least recently used first.
    sources: Vec<(SourceKey, AudioSource)>,
    /// Time-stretch state for contributors playing at a speed other than 1.
    stretchers: HashMap<ContributorId, StretchState>,
}

#[derive(Debug)]
struct StretchState {
    stretcher: TimeStretcher,
    /// Timeline frame the next block must start at for the state to be continuous.
    next_frame: u64,
}

impl AudioRenderer {
    pub fn new(
        sample_rate: u32,
        channels: u16,
        max_block_frames: usize,
        max_open_sources: usize,
    ) -> Result<Self, AudioRenderError> {
        if max_open_sources == 0 || max_block_frames == 0 {
            return Err(AudioRenderError::InvalidConfig);
        }
        Ok(Self {
            sample_rate,
            channels,
            max_block_frames,
            max_open_sources,
            mixer: AudioMixer::new(sample_rate, channels, max_block_frames)?,
            sources: Vec::new(),
            stretchers: HashMap::new(),
        })
    }

    /// The mixed block for timeline frames `start_frame..start_frame + frames`. `Ok(None)` means
    /// cancelled.
    pub fn render(
        &mut self,
        scene: &SceneSnapshot,
        start_frame: u64,
        frames: usize,
        cancel: &AtomicBool,
    ) -> Result<Option<AudioBlock>, AudioRenderError> {
        if frames == 0 || frames > self.max_block_frames {
            return Err(AudioRenderError::InvalidConfig);
        }
        let rate = f64::from(self.sample_rate);
        let window_end = start_frame + frames as u64;
        // A silent base block guarantees the output covers the whole requested range.
        let base = AudioBlock::new(
            self.sample_rate,
            self.channels,
            start_frame as f64 / rate,
            vec![0.0; frames * usize::from(self.channels)],
        )?;
        let mut parts: Vec<(AudioBlock, f32)> = Vec::new();

        for contributor in scene.audible_contributors() {
            let ContributorContent::Media(source) = &contributor.content else {
                continue;
            };
            let placement = &contributor.placement;
            let ramp_end = placement.speed_ramp_end;
            let unit_speed = (placement.speed - 1.0).abs() <= f64::EPSILON && ramp_end.is_none();
            let supported = |speed: f64| (MIN_SPEED..=MAX_SPEED).contains(&speed);
            if !unit_speed && !(supported(placement.speed) && ramp_end.is_none_or(supported)) {
                return Err(AudioRenderError::UnsupportedSpeed(contributor.id));
            }
            let clip_start = (placement.timeline_start_secs * rate).round() as u64;
            let clip_frames =
                (contributor.timeline_duration_secs().unwrap_or(0.0) * rate).round() as u64;
            let overlap_start = start_frame.max(clip_start);
            let overlap_end = window_end.min(clip_start + clip_frames);
            if overlap_start >= overlap_end {
                self.stretchers.remove(&contributor.id);
                continue;
            }
            let length = (overlap_end - overlap_start) as usize;
            let source_start = (source.range.in_secs * rate).round() as u64;

            let slot = self.open_source(&source.path, source.version.0)?;
            let samples = if unit_speed {
                self.stretchers.remove(&contributor.id);
                let source_frame = source_start + (overlap_start - clip_start);
                let Some(block) = self.sources[slot].1.read(source_frame, length, cancel)? else {
                    return Ok(None);
                };
                block.samples().to_vec()
            } else {
                // Continue the existing stretch only if this block follows the previous one.
                let mut state = match self.stretchers.remove(&contributor.id) {
                    Some(state) if state.next_frame == overlap_start => state,
                    _ => {
                        let mut stretcher =
                            TimeStretcher::new(placement.speed, usize::from(self.channels))
                                .ok_or(AudioRenderError::UnsupportedSpeed(contributor.id))?;
                        let local = (overlap_start - clip_start) as f64 / rate;
                        stretcher.reset(
                            source_start as f64 + contributor.source_offset_at(local) * rate,
                        );
                        StretchState {
                            stretcher,
                            next_frame: overlap_start,
                        }
                    }
                };
                // A speed ramp changes the stretch speed block by block.
                let block_local = (overlap_start - clip_start) as f64 / rate;
                state.stretcher.set_speed(contributor.speed_at(block_local));
                let channels = usize::from(self.channels);
                let source_audio = &mut self.sources[slot].1;
                let mut failure: Option<AudioSourceError> = None;
                let rendered = state.stretcher.render(length, &mut |start, frames| {
                    // Frames before the stream start are silence.
                    let padding = (-start).clamp(0, frames as i64) as usize;
                    let mut out = vec![0.0_f32; padding * channels];
                    if padding < frames {
                        match source_audio.read(start.max(0) as u64, frames - padding, cancel) {
                            Ok(Some(block)) => out.extend_from_slice(block.samples()),
                            Ok(None) => return None,
                            Err(error) => {
                                failure = Some(error);
                                return None;
                            }
                        }
                    }
                    Some(out)
                });
                if let Some(error) = failure {
                    return Err(error.into());
                }
                let Some(samples) = rendered else {
                    return Ok(None);
                };
                state.next_frame = overlap_end;
                self.stretchers.insert(contributor.id, state);
                samples
            };
            // Re-time from source time to timeline time so the mixer aligns it.
            let aligned = AudioBlock::new(
                self.sample_rate,
                self.channels,
                overlap_start as f64 / rate,
                samples,
            )?;
            parts.push((aligned, placement.gain));
        }

        let mut inputs = vec![MixInput {
            block: &base,
            gain: 1.0,
            muted: false,
        }];
        inputs.extend(parts.iter().map(|(block, gain)| MixInput {
            block,
            gain: *gain,
            muted: false,
        }));
        Ok(Some(self.mixer.mix(&inputs)?))
    }

    fn open_source(&mut self, path: &Path, version: u64) -> Result<usize, AudioRenderError> {
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
        let source =
            AudioSource::open(path, self.sample_rate, self.channels, self.max_block_frames)?;
        while self.sources.len() >= self.max_open_sources {
            self.sources.remove(0);
        }
        self.sources.push(((path.to_path_buf(), version), source));
        Ok(self.sources.len() - 1)
    }
}

#[derive(Debug)]
pub enum AudioPlayerError {
    Render(AudioRenderError),
    Sink(SinkError),
}

impl From<AudioRenderError> for AudioPlayerError {
    fn from(error: AudioRenderError) -> Self {
        Self::Render(error)
    }
}

impl From<SinkError> for AudioPlayerError {
    fn from(error: SinkError) -> Self {
        Self::Sink(error)
    }
}

/// Keeps a sink filled to `target_buffer_frames` ahead of the device by rendering consecutive
/// blocks. The caller invokes [`Self::pump`] from its audio-feed thread or timer.
#[derive(Debug)]
pub struct AudioPlayer {
    renderer: AudioRenderer,
    scene: SceneSnapshot,
    position_frame: u64,
    block_frames: usize,
    target_buffer_frames: usize,
}

impl AudioPlayer {
    pub fn new(
        renderer: AudioRenderer,
        scene: SceneSnapshot,
        block_frames: usize,
        target_buffer_frames: usize,
    ) -> Self {
        Self {
            renderer,
            scene,
            position_frame: 0,
            block_frames,
            target_buffer_frames,
        }
    }

    /// Timeline frame of the next block to be rendered.
    pub const fn position_frame(&self) -> u64 {
        self.position_frame
    }

    /// Jumps to `frame`, discarding queued audio so the next sound is from the new position.
    pub fn seek(&mut self, frame: u64, sink: &mut dyn AudioSink) {
        self.position_frame = frame;
        sink.clear();
    }

    /// Replaces the scene (a live edit). Queued-but-unplayed audio was rendered from the old
    /// scene, so it is dropped and rendering rewinds to where the device had reached.
    pub fn set_scene(&mut self, scene: SceneSnapshot, sink: &mut dyn AudioSink) {
        let queued = sink.buffered_frames() as u64;
        self.position_frame = self.position_frame.saturating_sub(queued);
        self.scene = scene;
        sink.clear();
    }

    /// Renders and queues blocks until the sink holds `target_buffer_frames`; returns the
    /// number of frames written. Stops without error when cancelled.
    pub fn pump(
        &mut self,
        sink: &mut dyn AudioSink,
        cancel: &AtomicBool,
    ) -> Result<usize, AudioPlayerError> {
        let mut written = 0;
        while sink.buffered_frames() < self.target_buffer_frames {
            let Some(block) = self.renderer.render(
                &self.scene,
                self.position_frame,
                self.block_frames,
                cancel,
            )?
            else {
                break;
            };
            sink.write(&block)?;
            self.position_frame += self.block_frames as u64;
            written += self.block_frames;
        }
        Ok(written)
    }
}
