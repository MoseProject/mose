//! Shared configuration updates for effects that retain frame history.

pub(crate) trait TemporalEffect {
    fn smoothing_mut(&mut self) -> &mut f32;
    fn reset_history(&mut self);

    /// Applies a validated smoothing weight, resetting history only when it changes.
    fn set_smoothing(&mut self, smoothing: f32) {
        let current = self.smoothing_mut();
        if *current != smoothing {
            *current = smoothing;
            self.reset_history();
        }
    }
}

#[cfg(test)]
#[path = "temporal_effect_test.rs"]
mod tests;
