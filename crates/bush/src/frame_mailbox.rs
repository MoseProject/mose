//! One-slot handoff from decode/composition work to the preview renderer.

use crate::{Generation, VideoFrame};

/// A decoded frame paired with the editor intent that produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct PublishedFrame {
    pub generation: Generation,
    pub frame: VideoFrame,
}

/// A bounded handoff that never presents a frame from an obsolete editor intent.
///
/// Synchronization belongs at the worker boundary. This deterministic primitive owns only the
/// latest completed frame, dropping stale work before any GPU upload or texture replacement.
#[derive(Debug, Default)]
pub struct FrameMailbox {
    pending: Option<PublishedFrame>,
}

impl FrameMailbox {
    pub const fn new() -> Self {
        Self { pending: None }
    }

    /// Publishes a completed frame, replacing an unconsumed older result.
    pub fn publish(&mut self, frame: PublishedFrame) -> Option<PublishedFrame> {
        self.pending.replace(frame)
    }

    /// Takes a frame only when it belongs to the currently requested generation.
    ///
    /// A stale pending result is discarded. A newer result is retained because the renderer may
    /// have observed an older generation immediately before its scene update arrives.
    pub fn take_for(&mut self, generation: Generation) -> Option<VideoFrame> {
        match self.pending.as_ref().map(|frame| frame.generation) {
            Some(frame_generation) if frame_generation == generation => {
                self.pending.take().map(|frame| frame.frame)
            }
            Some(frame_generation) if frame_generation < generation => {
                self.pending = None;
                None
            }
            _ => None,
        }
    }

    /// Drops a pending result older than `generation` and reports whether work was discarded.
    pub fn discard_before(&mut self, generation: Generation) -> bool {
        if matches!(self.pending.as_ref(), Some(frame) if frame.generation < generation) {
            self.pending = None;
            true
        } else {
            false
        }
    }

    pub const fn is_empty(&self) -> bool {
        self.pending.is_none()
    }
}
