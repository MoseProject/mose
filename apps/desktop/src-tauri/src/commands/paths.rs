// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Path helpers for the settings dialog: a native file/folder picker and a cheap existence
//! check, so a model or folder path can be browsed for and validated instead of typed blind.

use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PathKind {
    File,
    Folder,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum PathStatus {
    /// Nothing entered yet.
    Empty,
    File,
    Folder,
    Missing,
}

/// Opens the native picker; `extensions` (without dots) filter file picks. `None` when the
/// dialog is cancelled.
#[tauri::command]
pub fn pick_path(kind: PathKind, extensions: Vec<String>) -> Option<String> {
    let picked = match kind {
        PathKind::Folder => crate::dialogs::pick_folder(),
        PathKind::File => {
            let extensions: Vec<&str> = extensions.iter().map(String::as_str).collect();
            crate::dialogs::pick_file("Files", &extensions)
        }
    };
    picked.map(|path| path.display().to_string())
}

/// What `path` currently is on disk.
#[tauri::command]
pub fn check_path(path: String) -> PathStatus {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return PathStatus::Empty;
    }
    let path = Path::new(trimmed);
    if path.is_dir() {
        PathStatus::Folder
    } else if path.is_file() {
        PathStatus::File
    } else {
        PathStatus::Missing
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn an_empty_or_blank_path_is_empty() {
        assert_eq!(check_path(String::new()), PathStatus::Empty);
        assert_eq!(check_path("   ".into()), PathStatus::Empty);
    }

    #[test]
    fn files_folders_and_missing_paths_are_told_apart() {
        let dir = std::env::temp_dir().join(format!("oca_paths_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("model.bin");
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(check_path(dir.display().to_string()), PathStatus::Folder);
        assert_eq!(check_path(file.display().to_string()), PathStatus::File);
        assert_eq!(
            check_path(dir.join("nope.bin").display().to_string()),
            PathStatus::Missing
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_picker_returns_what_the_dialog_answers() {
        crate::dialogs::scripted::answer(Some(vec![PathBuf::from("C:/models/w.bin")]));
        assert_eq!(
            pick_path(PathKind::File, vec!["bin".into()]),
            Some(PathBuf::from("C:/models/w.bin").display().to_string())
        );
        crate::dialogs::scripted::answer(Some(vec![PathBuf::from("C:/out")]));
        assert_eq!(
            pick_path(PathKind::Folder, Vec::new()),
            Some(PathBuf::from("C:/out").display().to_string())
        );
        crate::dialogs::scripted::answer(None);
        assert_eq!(pick_path(PathKind::Folder, Vec::new()), None);
    }
}
