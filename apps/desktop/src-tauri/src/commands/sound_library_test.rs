use super::*;
use crate::testing::{fixture, harness, path_in};

#[test]
fn an_unset_library_path_lists_nothing() {
    let h = harness();
    h.prefs().view.sound_library_path = "   ".into();
    assert!(list_sound_library(h.state()).is_empty());
}

#[test]
fn audio_files_in_the_music_and_sfx_folders_are_listed_with_their_category() {
    let h = harness();
    let root = path_in(&h.dir, "sounds");
    std::fs::create_dir_all(root.join("music")).unwrap();
    std::fs::create_dir_all(root.join("sfx")).unwrap();
    std::fs::copy(fixture("audio.m4a"), root.join("music").join("theme.m4a")).unwrap();
    std::fs::copy(fixture("audio.m4a"), root.join("sfx").join("hit.m4a")).unwrap();
    std::fs::write(root.join("sfx").join("notes.txt"), b"ignored").unwrap();
    h.prefs().view.sound_library_path = root.display().to_string();

    let tracks = list_sound_library(h.state());
    assert_eq!(tracks.len(), 2);
    let theme = tracks.iter().find(|t| t.name == "theme").unwrap();
    assert!(matches!(theme.category, SoundCategoryView::Music));
    assert!(theme.duration_secs > 0.0);
    assert!(theme.path.ends_with("theme.m4a"));
    let hit = tracks.iter().find(|t| t.name == "hit").unwrap();
    assert!(matches!(hit.category, SoundCategoryView::Sfx));
}

#[test]
fn a_missing_library_folder_lists_nothing() {
    let h = harness();
    h.prefs().view.sound_library_path = path_in(&h.dir, "nowhere").display().to_string();
    assert!(list_sound_library(h.state()).is_empty());
}
