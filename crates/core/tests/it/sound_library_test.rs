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

use std::path::{Path, PathBuf};

use avcore::sound_library::{scan_library_dir, SoundCategory};

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// A fresh temp dir with no `music`/`sfx` subfolders at all — the common case before a user has
/// configured anything.
#[test]
fn scan_library_dir_is_empty_for_a_folder_with_no_subdirs() {
    let dir = std::env::temp_dir().join(format!("oca_sound_lib_empty_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);

    let tracks = scan_library_dir(&dir);

    assert!(tracks.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_library_dir_finds_and_probes_files_in_music_and_sfx() {
    let dir = std::env::temp_dir().join(format!("oca_sound_lib_full_{}", std::process::id()));
    let music_dir = dir.join("music");
    let sfx_dir = dir.join("sfx");
    std::fs::create_dir_all(&music_dir).unwrap();
    std::fs::create_dir_all(&sfx_dir).unwrap();

    std::fs::copy(fixture("audio.m4a"), music_dir.join("background_theme.m4a")).unwrap();
    std::fs::copy(fixture("audio.m4a"), sfx_dir.join("click.m4a")).unwrap();
    // A non-audio file dropped in by accident must be ignored, not crash the scan.
    std::fs::write(music_dir.join("notes.txt"), b"not audio").unwrap();

    let tracks = scan_library_dir(&dir);

    assert_eq!(tracks.len(), 2);
    let music = tracks
        .iter()
        .find(|t| t.category == SoundCategory::Music)
        .expect("music track present");
    assert_eq!(music.name, "background_theme");
    assert!(music.duration_secs > 0.0);

    let sfx = tracks
        .iter()
        .find(|t| t.category == SoundCategory::Sfx)
        .expect("sfx track present");
    assert_eq!(sfx.name, "click");
    assert!(sfx.duration_secs > 0.0);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn scan_library_dir_skips_a_missing_root_folder() {
    let dir = std::env::temp_dir().join("oca_sound_lib_does_not_exist_at_all");
    let _ = std::fs::remove_dir_all(&dir);

    let tracks = scan_library_dir(&dir);

    assert!(tracks.is_empty());
}
