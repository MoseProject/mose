//! Bush is Mose's standalone, editor-first preview engine.
//!
//! This crate intentionally has no dependency on `core`, UI crates, GStreamer, or FFmpeg. Its
//! first primitive models the scheduling contract shared by every interactive engine command:
//! pending work is replaceable, and the worker receives only the latest editor intent.

mod latest_intent;

pub use latest_intent::{Generation, LatestIntent, PendingIntent};
