//! Time-varying layer properties: keyframe tracks and fades.
//!
//! Mose resolves its own keyframe model into these plain tracks; Bush evaluates them per frame
//! from the playhead, so animating a property never rebuilds decoders or invalidates caches.

/// Interpolation applied over the segment that *starts* at a keyframe.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Easing {
    #[default]
    Linear,
    /// Holds the starting value until the next keyframe, then jumps.
    Hold,
    EaseIn,
    EaseOut,
    EaseInOut,
}

impl Easing {
    /// Maps linear progress `0.0..=1.0` to eased progress.
    fn apply(self, progress: f64) -> f64 {
        match self {
            Self::Linear => progress,
            Self::Hold => 0.0,
            Self::EaseIn => progress * progress,
            Self::EaseOut => 1.0 - (1.0 - progress) * (1.0 - progress),
            Self::EaseInOut => progress * progress * (3.0 - 2.0 * progress),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Keyframe {
    /// Seconds from the contributor's timeline start.
    pub time_secs: f64,
    pub value: f32,
    pub easing: Easing,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeyframeError {
    Empty,
    NonFinite,
    NegativeTime,
    /// Times must be strictly increasing.
    UnorderedTimes,
}

/// A validated, strictly time-ordered keyframe list.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeTrack {
    keyframes: Vec<Keyframe>,
}

impl KeyframeTrack {
    pub fn new(keyframes: Vec<Keyframe>) -> Result<Self, KeyframeError> {
        if keyframes.is_empty() {
            return Err(KeyframeError::Empty);
        }
        let mut previous: Option<f64> = None;
        for keyframe in &keyframes {
            if !keyframe.time_secs.is_finite() || !keyframe.value.is_finite() {
                return Err(KeyframeError::NonFinite);
            }
            if keyframe.time_secs < 0.0 {
                return Err(KeyframeError::NegativeTime);
            }
            if previous.is_some_and(|time| keyframe.time_secs <= time) {
                return Err(KeyframeError::UnorderedTimes);
            }
            previous = Some(keyframe.time_secs);
        }
        Ok(Self { keyframes })
    }

    pub fn keyframes(&self) -> &[Keyframe] {
        &self.keyframes
    }

    /// Whether every keyframe value lies in `range`.
    pub fn values_within(&self, range: std::ops::RangeInclusive<f32>) -> bool {
        self.keyframes
            .iter()
            .all(|keyframe| range.contains(&keyframe.value))
    }

    /// The value at `local_secs`: clamped to the first/last keyframe outside the track, eased
    /// between neighbours.
    pub fn value_at(&self, local_secs: f64) -> f32 {
        let first = self.keyframes[0];
        if local_secs <= first.time_secs {
            return first.value;
        }
        let last = self.keyframes[self.keyframes.len() - 1];
        if local_secs >= last.time_secs {
            return last.value;
        }
        let next_index = self
            .keyframes
            .partition_point(|keyframe| keyframe.time_secs <= local_secs);
        let (from, to) = (self.keyframes[next_index - 1], self.keyframes[next_index]);
        let progress = (local_secs - from.time_secs) / (to.time_secs - from.time_secs);
        let eased = from.easing.apply(progress);
        (f64::from(from.value) + (f64::from(to.value) - f64::from(from.value)) * eased) as f32
    }
}

/// Per-property animation overrides. A property with no track keeps its static value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Animation {
    pub opacity: Option<KeyframeTrack>,
    pub x: Option<KeyframeTrack>,
    pub y: Option<KeyframeTrack>,
    pub scale_x: Option<KeyframeTrack>,
    pub scale_y: Option<KeyframeTrack>,
    pub rotation_deg: Option<KeyframeTrack>,
}

impl Animation {
    pub fn is_empty(&self) -> bool {
        self.opacity.is_none()
            && self.x.is_none()
            && self.y.is_none()
            && self.scale_x.is_none()
            && self.scale_y.is_none()
            && self.rotation_deg.is_none()
    }

    /// Whether every track's values are legal for the property it animates.
    pub(crate) fn is_valid(&self) -> bool {
        let opacity_ok = self
            .opacity
            .as_ref()
            .is_none_or(|track| track.values_within(0.0..=1.0));
        let scale_ok = [&self.scale_x, &self.scale_y].into_iter().all(|track| {
            track.as_ref().is_none_or(|track| {
                track
                    .keyframes()
                    .iter()
                    .all(|keyframe| keyframe.value > 0.0)
            })
        });
        opacity_ok && scale_ok
    }
}

/// Linear fade-in from the contributor's start and fade-out to its end.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fade {
    pub in_secs: f64,
    pub out_secs: f64,
}

impl Fade {
    pub(crate) fn is_valid(&self) -> bool {
        self.in_secs.is_finite()
            && self.out_secs.is_finite()
            && self.in_secs >= 0.0
            && self.out_secs >= 0.0
    }

    /// Opacity multiplier at `local_secs` into a contributor lasting `duration_secs` (`None`
    /// means no end, so only the fade-in applies).
    pub fn factor_at(&self, local_secs: f64, duration_secs: Option<f64>) -> f32 {
        let mut factor = 1.0_f64;
        if self.in_secs > 0.0 && local_secs < self.in_secs {
            factor = factor.min((local_secs / self.in_secs).max(0.0));
        }
        if let Some(duration) = duration_secs {
            if self.out_secs > 0.0 && local_secs > duration - self.out_secs {
                factor = factor.min(((duration - local_secs) / self.out_secs).max(0.0));
            }
        }
        factor as f32
    }
}
