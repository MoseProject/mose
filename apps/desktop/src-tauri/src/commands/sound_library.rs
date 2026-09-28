//! Bundled sound library browsing -- lists the configured `prefs.sound_library_path`'s
//! `music`/`sfx` subfolders via `avcore::scan_library_dir`; importing a track reuses the
//! existing `commands::media::import_media` pipeline, same as any other file. No core changes
//! needed.

use avcore::{LibraryTrack, SoundCategory};
use serde::Serialize;
use tauri::State;

use crate::state::AppState;

#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SoundCategoryView {
    Music,
    Sfx,
}

impl From<SoundCategory> for SoundCategoryView {
    fn from(category: SoundCategory) -> Self {
        match category {
            SoundCategory::Music => Self::Music,
            SoundCategory::Sfx => Self::Sfx,
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryTrackView {
    pub name: String,
    pub path: String,
    pub duration_secs: f64,
    pub category: SoundCategoryView,
}

impl From<LibraryTrack> for LibraryTrackView {
    fn from(track: LibraryTrack) -> Self {
        Self {
            name: track.name,
            path: track.path.display().to_string(),
            duration_secs: track.duration_secs,
            category: track.category.into(),
        }
    }
}

/// Scans `prefs.sound_library_path`'s `music`/`sfx` subfolders for audio files -- what the
/// media library's "Biblioteca de som" panel lists. Empty if the pref isn't configured or the
/// folder has neither subfolder.
#[tauri::command]
pub fn list_sound_library(state: State<'_, AppState>) -> Vec<LibraryTrackView> {
    let path = state.prefs().view.sound_library_path.clone();
    if path.trim().is_empty() {
        return Vec::new();
    }
    avcore::scan_library_dir(std::path::Path::new(&path))
        .into_iter()
        .map(LibraryTrackView::from)
        .collect()
}
