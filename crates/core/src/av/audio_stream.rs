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

//! The tracks of a multi-track recording (OBS can write game, microphone and chat as separate
//! audio streams of one file). Every audio path of the editor (waveform, loudness, preview mix,
//! export mix) reads a file's first audio stream, so each extra track is copied out, without
//! decoding, into a file of its own in the project cache and imported as an ordinary audio asset
//! ([`crate::MediaAsset::audio_stream`] remembers where it came from, so a cleared cache or a
//! project opened on another machine can make the file again).

use std::fs;
use std::path::{Path, PathBuf};

/// Where an audio-only asset made from one track of a multi-track file came from.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AudioStreamSource {
    /// The recording the track was copied from.
    pub origin: PathBuf,
    /// The track's stream index within `origin`.
    pub stream_index: u32,
}

/// What [`ensure_audio_stream_file`] failed on.
#[derive(Debug)]
pub enum AudioStreamError {
    Io(std::io::Error),
    Bridge(avbridge::RemuxError),
}

impl std::fmt::Display for AudioStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AudioStreamError::Io(error) => write!(f, "could not write the audio track: {error}"),
            AudioStreamError::Bridge(error) => write!(f, "could not copy the audio track: {error}"),
        }
    }
}

impl std::error::Error for AudioStreamError {}

/// The file track `stream_index` of `origin` is copied to inside `dir`. Named from the recording's
/// stem, the track and a hash of the full path, so two recordings called the same never share a
/// file.
pub fn audio_stream_file_path(origin: &Path, stream_index: u32, dir: &Path) -> PathBuf {
    // FNV-1a over the path: stable across runs and Rust versions, unlike `DefaultHasher`.
    let mut hash: u32 = 0x811c_9dc5;
    for byte in origin.to_string_lossy().bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    let stem = origin
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("recording");
    dir.join(format!("{stem}_track{stream_index}_{hash:08x}.mka"))
}

/// Copies track `stream_index` of `origin` into `dir` and returns the file, unless a copy newer
/// than `origin` is already there. Written under a temporary name and moved into place, so a
/// failed or interrupted copy never leaves a file that passes for a finished one.
pub fn ensure_audio_stream_file(
    origin: &Path,
    stream_index: u32,
    dir: &Path,
) -> Result<PathBuf, AudioStreamError> {
    let path = audio_stream_file_path(origin, stream_index, dir);
    if is_up_to_date(origin, &path) {
        return Ok(path);
    }
    fs::create_dir_all(dir).map_err(AudioStreamError::Io)?;
    let mut partial_name = path.file_name().unwrap_or_default().to_os_string();
    partial_name.push(".partial.mka");
    let partial = path.with_file_name(partial_name);
    if let Err(error) = avbridge::extract_audio_stream(origin, &partial, stream_index) {
        let _ = fs::remove_file(&partial);
        return Err(AudioStreamError::Bridge(error));
    }
    if let Err(error) = fs::rename(&partial, &path) {
        let _ = fs::remove_file(&partial);
        return Err(AudioStreamError::Io(error));
    }
    Ok(path)
}

fn is_up_to_date(origin: &Path, copy: &Path) -> bool {
    let (Ok(origin_meta), Ok(copy_meta)) = (fs::metadata(origin), fs::metadata(copy)) else {
        return false;
    };
    match (origin_meta.modified(), copy_meta.modified()) {
        (Ok(origin_time), Ok(copy_time)) => copy_time >= origin_time,
        _ => false,
    }
}

#[cfg(test)]
#[path = "audio_stream/audio_stream_test.rs"]
mod tests;
