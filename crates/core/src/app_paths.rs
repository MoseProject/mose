// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! On-disk locations every oca frontend must agree on, so the egui and Tauri shells read the
//! same preferences and recover each other's autosaves.

use std::path::{Path, PathBuf};

/// The preferences file -- `.oc`, the same gzip-compressed MessagePack framing as a `.ocproj`
/// project (see [`crate::persistence::to_ocproj_bytes`]), not JSON.
///
/// - macOS:   `~/Library/Application Support/oca/prefs.oc`
/// - Windows: `%APPDATA%\oca\prefs.oc`
/// - Linux:   `~/.config/oca/prefs.oc`
///
/// Falls back to `prefs.oc` in the working directory if the platform variable is unset.
pub fn prefs_path() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join("Library/Application Support/oca/prefs.oc");
        }
    }
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join("oca\\prefs.oc");
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(".config/oca/prefs.oc");
        }
    }
    PathBuf::from("prefs.oc")
}

/// The crash-recovery snapshot written next to a saved project:
/// `<dir>/<name>.autosave.ocproj` for `<dir>/<name>.ocproj`.
pub fn autosave_path(project_path: &Path) -> PathBuf {
    project_path.with_extension("autosave.ocproj")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autosave_sits_next_to_the_project() {
        assert_eq!(
            autosave_path(Path::new("C:/projects/demo.ocproj")),
            PathBuf::from("C:/projects/demo.autosave.ocproj")
        );
    }

    #[test]
    fn prefs_file_is_named_prefs_oc() {
        assert_eq!(prefs_path().file_name().unwrap(), "prefs.oc");
    }
}
