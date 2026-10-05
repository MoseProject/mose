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

//! Local music/SFX catalog for `request.md`'s Fase 4 "Biblioteca de música e efeitos sonoros".
//!
//! Nothing here is bundled with the app — per this codebase's no-mock-data rule, and since
//! licensing real royalty-free tracks is out of scope for this pass, [`scan_library_dir`]
//! simply reads whatever audio files the user has dropped into a folder they configure
//! themselves (see `ui`'s Preferences screen). The library starts empty until that folder has
//! a `music/` or `sfx/` subfolder with files in it.

use std::path::{Path, PathBuf};

use crate::probe::probe_media;

/// Which subfolder of the library root a track was found in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SoundCategory {
    Music,
    Sfx,
}

/// One audio file found under the configured library folder, probed for its duration.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryTrack {
    pub name: String,
    pub path: PathBuf,
    pub duration_secs: f64,
    pub category: SoundCategory,
}

const AUDIO_EXTENSIONS: [&str; 5] = ["mp3", "wav", "ogg", "flac", "m4a"];

/// Scans `<dir>/music` and `<dir>/sfx` for audio files (filtered by [`AUDIO_EXTENSIONS`]),
/// probing each for its duration via [`probe_media`]. A missing subfolder is simply skipped
/// (not an error) — the library starts empty until the user creates it and drops files in.
/// A file that fails to probe (corrupt/unsupported codec) is silently skipped rather than
/// failing the whole scan. Returned sorted by name for a stable on-screen order.
pub fn scan_library_dir(dir: &Path) -> Vec<LibraryTrack> {
    let mut tracks = Vec::new();
    for (subdir, category) in [("music", SoundCategory::Music), ("sfx", SoundCategory::Sfx)] {
        let Ok(entries) = std::fs::read_dir(dir.join(subdir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let is_audio = path
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()))
                .unwrap_or(false);
            if !is_audio {
                continue;
            }
            let Ok(probed) = probe_media(&path) else {
                continue;
            };
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            tracks.push(LibraryTrack {
                name,
                path,
                duration_secs: probed.duration_secs,
                category,
            });
        }
    }
    tracks.sort_by(|a, b| a.name.cmp(&b.name));
    tracks
}
