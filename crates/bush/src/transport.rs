//! Presentation-only transport state for interactive preview.

/// Whether Bush should advance the presentation clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackState {
    Paused,
    Playing,
}

/// Validated editor transport state in timeline seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transport {
    position_secs: f64,
    rate: f64,
    state: PlaybackState,
}

/// Invalid transport input rejected before scheduling or decoder work.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransportError {
    InvalidPosition(f64),
    InvalidRate(f64),
    InvalidElapsed(f64),
}

impl Transport {
    /// Creates a paused transport at `position_secs` with a positive playback `rate`.
    pub fn new(position_secs: f64, rate: f64) -> Result<Self, TransportError> {
        validate_position(position_secs)?;
        validate_rate(rate)?;

        Ok(Self {
            position_secs,
            rate,
            state: PlaybackState::Paused,
        })
    }

    /// Returns the current playhead position in timeline seconds.
    pub const fn position_secs(&self) -> f64 {
        self.position_secs
    }

    /// Returns the positive playback speed factor.
    pub const fn rate(&self) -> f64 {
        self.rate
    }

    /// Returns the current playback state.
    pub const fn state(&self) -> PlaybackState {
        self.state
    }

    /// Starts advancing the presentation clock.
    pub fn play(&mut self) {
        self.state = PlaybackState::Playing;
    }

    /// Stops advancing the presentation clock without changing its position.
    pub fn pause(&mut self) {
        self.state = PlaybackState::Paused;
    }

    /// Moves the playhead without changing the scene or source versions.
    pub fn seek(&mut self, position_secs: f64) -> Result<(), TransportError> {
        validate_position(position_secs)?;
        self.position_secs = position_secs;
        Ok(())
    }

    /// Changes speed without conflating pause with a zero or invalid rate.
    pub fn set_rate(&mut self, rate: f64) -> Result<(), TransportError> {
        validate_rate(rate)?;
        self.rate = rate;
        Ok(())
    }

    /// Advances the playing transport by a monotonic elapsed duration.
    pub fn advance(&mut self, elapsed_secs: f64) -> Result<(), TransportError> {
        if !elapsed_secs.is_finite() || elapsed_secs < 0.0 {
            return Err(TransportError::InvalidElapsed(elapsed_secs));
        }
        if self.state == PlaybackState::Paused {
            return Ok(());
        }

        let next = self.position_secs + elapsed_secs * self.rate;
        validate_position(next)?;
        self.position_secs = next;
        Ok(())
    }
}

fn validate_position(position_secs: f64) -> Result<(), TransportError> {
    if !position_secs.is_finite() || position_secs < 0.0 {
        return Err(TransportError::InvalidPosition(position_secs));
    }
    Ok(())
}

fn validate_rate(rate: f64) -> Result<(), TransportError> {
    if !rate.is_finite() || rate <= 0.0 {
        return Err(TransportError::InvalidRate(rate));
    }
    Ok(())
}
