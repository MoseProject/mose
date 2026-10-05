// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! `oca-avbridge` — thin C bridge over libavformat/libavcodec/libavutil (FFmpeg), called via
//! FFI. Replaces spawning `ffprobe`/`ffmpeg` as external processes: `core` links this
//! crate directly, so probing and encoding never shell out or parse subprocess stdout.
//!
//! The C surface (`csrc/bridge.c`) is written for this project — not a full auto-generated
//! FFmpeg binding — so it stays small and auditable. See [`build.rs`](../build.rs) for how the
//! FFmpeg dev libs (`FFMPEG_DIR`) are located and linked.

mod audio_decode;
mod audio_mix;
mod decode;
mod export;
mod ffi;
mod loudness;
mod matte;
mod overlay;
mod pcm;
mod probe;
mod proxy;
mod remux;
#[cfg(test)]
mod status_tests;
mod timeline_export;
mod waveform;

pub use audio_decode::{AudioDecodeError, AudioDecodeInfo, AudioDecodeSession, DecodedAudioBlock};
pub use audio_mix::*;
pub use decode::*;
pub use export::*;
pub(crate) use ffi::*;
pub use loudness::*;
pub use matte::*;
pub use overlay::*;
pub use pcm::*;
pub use probe::*;
pub use proxy::*;
pub use remux::*;
pub use timeline_export::*;
pub use waveform::*;

/// libavformat's packed version number (same encoding as `LIBAVFORMAT_VERSION_INT` /
/// `avformat_version()`): `(major << 16) | (minor << 8) | micro`.
pub fn version() -> u32 {
    // SAFETY: avbridge_version() takes no arguments, returns a plain u32, and has no
    // documented failure mode in libavformat — it's a pure accessor.
    unsafe { avbridge_version() }
}
