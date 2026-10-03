//! Playback driver: turns clock ticks into engine requests.
//!
//! `Player` owns an [`Engine`] and a [`Transport`] but no thread and no wall clock: the caller
//! supplies a monotonic `now` to every call (the UI's frame timer in production, plain
//! `Duration`s in tests). Each tick advances the playhead by the elapsed time times the rate and
//! submits one request; a request still unfinished when the next tick arrives is superseded by
//! the engine, so a slow decode drops frames instead of building latency.

use std::sync::Arc;
use std::time::Duration;

use crate::{
    ContributorId, Engine, EngineError, EngineEvent, EngineRequest, Generation, PlaybackState,
    SceneSnapshot, Transport, TransportError, VideoFrame,
};

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerEvent {
    Frame {
        generation: Generation,
        position_secs: f64,
        frame: VideoFrame,
    },
    /// Playback reached the end of the timeline and paused there. Emitted once, after the
    /// final frame.
    Ended { position_secs: f64 },
    Failed {
        generation: Generation,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum PlayerError {
    Engine(EngineError),
    Transport(TransportError),
    InvalidDuration(f64),
}

impl From<EngineError> for PlayerError {
    fn from(error: EngineError) -> Self {
        Self::Engine(error)
    }
}

impl From<TransportError> for PlayerError {
    fn from(error: TransportError) -> Self {
        Self::Transport(error)
    }
}

type Overlays = Vec<(ContributorId, Arc<VideoFrame>)>;

/// Generations whose position is remembered; older ones were necessarily superseded.
const IN_FLIGHT_LIMIT: usize = 8;

#[derive(Debug)]
pub struct Player {
    engine: Engine,
    transport: Transport,
    scene: SceneSnapshot,
    overlays: Overlays,
    duration_secs: f64,
    last_tick: Option<Duration>,
    /// Set when a tick reached the end; cleared by the next `play`/`seek`.
    ended: bool,
    /// Generation of the request rendered at the end position, once the end was reached.
    end_generation: Option<Generation>,
    pending_end_event: Option<f64>,
    /// Position each submitted generation was rendered for, oldest first (bounded).
    in_flight: Vec<(Generation, f64)>,
}

impl Player {
    pub fn new(
        engine: Engine,
        scene: SceneSnapshot,
        duration_secs: f64,
    ) -> Result<Self, PlayerError> {
        validate_duration(duration_secs)?;
        Ok(Self {
            engine,
            transport: Transport::new(0.0, 1.0)?,
            scene,
            overlays: Vec::new(),
            duration_secs,
            last_tick: None,
            ended: false,
            end_generation: None,
            pending_end_event: None,
            in_flight: Vec::new(),
        })
    }

    pub const fn position_secs(&self) -> f64 {
        self.transport.position_secs()
    }

    pub const fn state(&self) -> PlaybackState {
        self.transport.state()
    }

    pub const fn rate(&self) -> f64 {
        self.transport.rate()
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Whether playback has reached the end since the last [`Self::play`] or [`Self::seek`].
    pub const fn has_ended(&self) -> bool {
        self.ended
    }

    /// Starts playing from the current position; at the end of the timeline, from the start.
    pub fn play(&mut self, now: Duration) -> Result<(), PlayerError> {
        if self.transport.position_secs() >= self.duration_secs {
            self.transport.seek(0.0)?;
        }
        self.reset_end();
        self.last_tick = Some(now);
        self.transport.play();
        self.submit_current().map(|_| ())
    }

    pub fn pause(&mut self) {
        self.transport.pause();
        self.last_tick = None;
    }

    /// Jumps to `position_secs` (clamped to the timeline) and renders that frame, playing or not.
    pub fn seek(&mut self, position_secs: f64) -> Result<Generation, PlayerError> {
        self.transport
            .seek(position_secs.clamp(0.0, self.duration_secs))?;
        self.reset_end();
        self.submit_current()
    }

    pub fn set_rate(&mut self, rate: f64) -> Result<(), PlayerError> {
        self.transport.set_rate(rate)?;
        Ok(())
    }

    /// Live edit: replaces the scene and re-renders the current position immediately.
    pub fn set_scene(
        &mut self,
        scene: SceneSnapshot,
        overlays: Overlays,
        duration_secs: f64,
    ) -> Result<Generation, PlayerError> {
        validate_duration(duration_secs)?;
        self.scene = scene;
        self.overlays = overlays;
        self.duration_secs = duration_secs;
        if self.transport.position_secs() > duration_secs {
            self.transport.seek(duration_secs)?;
        }
        self.submit_current()
    }

    /// Advances playback to `now` and submits the frame for the new position. Returns `None`
    /// when paused or when no time has elapsed.
    pub fn tick(&mut self, now: Duration) -> Result<Option<Generation>, PlayerError> {
        if self.transport.state() != PlaybackState::Playing {
            return Ok(None);
        }
        let elapsed = self
            .last_tick
            .map(|last| now.saturating_sub(last))
            .unwrap_or_default();
        self.last_tick = Some(now);
        if elapsed.is_zero() {
            return Ok(None);
        }

        self.transport.advance(elapsed.as_secs_f64())?;
        let reached_end = self.transport.position_secs() >= self.duration_secs;
        if reached_end {
            self.transport.seek(self.duration_secs)?;
            self.transport.pause();
            self.last_tick = None;
            self.ended = true;
        }
        let generation = self.submit_current()?;
        if reached_end {
            self.end_generation = Some(generation);
        }
        Ok(Some(generation))
    }

    /// Like [`Self::tick`], but audio is the master clock: the playhead follows
    /// `audio_position_secs` (frames the audio device has actually played divided by the sample
    /// rate) instead of integrating wall-clock time, so video drops or repeats frames to stay in
    /// sync and never runs ahead of sound. The playhead never moves backwards: a late or glitching
    /// audio clock holds the picture, it does not rewind it. Returns `None` when paused or when
    /// the audio clock has not advanced.
    pub fn tick_synced(
        &mut self,
        now: Duration,
        audio_position_secs: f64,
    ) -> Result<Option<Generation>, PlayerError> {
        if self.transport.state() != PlaybackState::Playing {
            return Ok(None);
        }
        self.last_tick = Some(now);
        if !audio_position_secs.is_finite() {
            return Ok(None);
        }
        let target = audio_position_secs.max(self.transport.position_secs());
        if target == self.transport.position_secs() {
            return Ok(None);
        }
        self.transport.seek(target)?;
        let reached_end = self.transport.position_secs() >= self.duration_secs;
        if reached_end {
            self.transport.seek(self.duration_secs)?;
            self.transport.pause();
            self.last_tick = None;
            self.ended = true;
        }
        let generation = self.submit_current()?;
        if reached_end {
            self.end_generation = Some(generation);
        }
        Ok(Some(generation))
    }

    /// Signed drift of the integrated video clock against the audio clock, in seconds: positive
    /// means the playhead is ahead of the audio device. Use it to report A/V sync health.
    pub fn drift_against(&self, audio_position_secs: f64) -> f64 {
        self.transport.position_secs() - audio_position_secs
    }

    /// The newest finished event. Frames are tagged with the position they were rendered for.
    pub fn poll(&mut self) -> Option<PlayerEvent> {
        if let Some(position_secs) = self.pending_end_event.take() {
            return Some(PlayerEvent::Ended { position_secs });
        }
        let event = self.engine.poll()?;
        Some(self.map_event(event))
    }

    /// Like [`Self::poll`], but blocks up to `timeout` for an event.
    pub fn wait(&mut self, timeout: Duration) -> Option<PlayerEvent> {
        if let Some(position_secs) = self.pending_end_event.take() {
            return Some(PlayerEvent::Ended { position_secs });
        }
        let event = self.engine.wait(timeout)?;
        Some(self.map_event(event))
    }

    fn map_event(&mut self, event: EngineEvent) -> PlayerEvent {
        match event {
            EngineEvent::Frame { generation, frame } => {
                let position_secs = self
                    .position_for(generation)
                    .unwrap_or_else(|| frame.timestamp_secs());
                self.in_flight
                    .retain(|(candidate, _)| *candidate > generation);
                if self.end_generation == Some(generation) {
                    self.end_generation = None;
                    self.pending_end_event = Some(position_secs);
                }
                PlayerEvent::Frame {
                    generation,
                    position_secs,
                    frame,
                }
            }
            EngineEvent::Failed {
                generation,
                message,
            } => PlayerEvent::Failed {
                generation,
                message,
            },
        }
    }

    fn reset_end(&mut self) {
        self.ended = false;
        self.end_generation = None;
        self.pending_end_event = None;
    }

    fn submit_current(&mut self) -> Result<Generation, PlayerError> {
        let position = self.transport.position_secs();
        let generation = self.engine.submit(EngineRequest {
            scene: self.scene.clone(),
            playhead_secs: position,
            overlays: self.overlays.clone(),
        })?;
        self.in_flight.push((generation, position));
        if self.in_flight.len() > IN_FLIGHT_LIMIT {
            self.in_flight.remove(0);
        }
        Ok(generation)
    }

    fn position_for(&self, generation: Generation) -> Option<f64> {
        self.in_flight
            .iter()
            .find(|(candidate, _)| *candidate == generation)
            .map(|(_, position)| *position)
    }
}

fn validate_duration(duration_secs: f64) -> Result<(), PlayerError> {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err(PlayerError::InvalidDuration(duration_secs));
    }
    Ok(())
}
