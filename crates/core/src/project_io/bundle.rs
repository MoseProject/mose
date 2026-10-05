// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Runtime-resource discovery for self-contained release bundles.
//!
//! Release assembly places every non-system resource under `resources/` next to the oca
//! executable. The application only reads from that tree; network access is deliberately kept
//! in the build-time packaging scripts. `OCA_RESOURCE_DIR` is an explicit override for tests,
//! development and unusual package layouts.

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundledResource {
    WhisperBase,
    Reframe,
    BackgroundRemoval,
    TtsVoice,
    TtsVoiceConfig,
}

impl BundledResource {
    pub const REQUIRED: [Self; 5] = [
        Self::WhisperBase,
        Self::Reframe,
        Self::BackgroundRemoval,
        Self::TtsVoice,
        Self::TtsVoiceConfig,
    ];

    pub const fn relative_path(self) -> &'static str {
        match self {
            Self::WhisperBase => "models/ggml-base.bin",
            Self::Reframe => "models/version-RFB-320_simplified.onnx",
            Self::BackgroundRemoval => "models/modnet_photographic.onnx",
            Self::TtsVoice => "models/pt_BR-faber-medium.onnx",
            Self::TtsVoiceConfig => "models/pt_BR-faber-medium.onnx.json",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingBundleResources {
    pub paths: Vec<PathBuf>,
}

impl std::fmt::Display for MissingBundleResources {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bundle is missing {} required resource(s)",
            self.paths.len()
        )
    }
}

impl std::error::Error for MissingBundleResources {}

pub fn resource_path_in(root: &Path, resource: BundledResource) -> PathBuf {
    root.join(resource.relative_path())
}

/// Finds the release bundle's `resources/` directory.
pub fn bundled_resources_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OCA_RESOURCE_DIR") {
        return Some(PathBuf::from(path));
    }
    resources_dir_for_executable(&std::env::current_exe().ok()?)
}

fn resources_dir_for_executable(executable: &Path) -> Option<PathBuf> {
    let executable_dir = executable.parent()?;
    if executable_dir
        .file_name()
        .is_some_and(|name| name == "MacOS")
    {
        let contents = executable_dir.parent()?;
        if contents.file_name().is_some_and(|name| name == "Contents") {
            return Some(contents.join("Resources"));
        }
    }
    Some(executable_dir.join("resources"))
}

/// Returns a required resource only when it is already present locally.
pub fn bundled_resource_path(resource: BundledResource) -> Option<PathBuf> {
    let path = resource_path_in(&bundled_resources_dir()?, resource);
    path.is_file().then_some(path)
}

pub fn validate_bundled_resources(root: &Path) -> Result<(), MissingBundleResources> {
    let paths = BundledResource::REQUIRED
        .into_iter()
        .map(|resource| resource_path_in(root, resource))
        .filter(|path| !path.is_file())
        .collect::<Vec<_>>();
    if paths.is_empty() {
        Ok(())
    } else {
        Err(MissingBundleResources { paths })
    }
}

/// Puts the bundled helper tools on `PATH` before any subprocess is started. A missing
/// directory is ignored so source-tree development keeps using system SDKs.
pub fn configure_bundled_runtime() {
    let Some(resources) = bundled_resources_dir() else {
        return;
    };
    let runtime = resources.join("runtime");

    let tools = runtime.join("bin");
    if tools.is_dir() {
        let mut paths = vec![tools];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(joined) = std::env::join_paths(paths) {
            std::env::set_var("PATH", joined);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::resources_dir_for_executable;
    use std::path::{Path, PathBuf};

    #[test]
    fn macos_app_uses_the_contents_resources_directory() {
        assert_eq!(
            resources_dir_for_executable(Path::new("/Applications/Oca.app/Contents/MacOS/oca")),
            Some(PathBuf::from("/Applications/Oca.app/Contents/Resources"))
        );
    }

    #[test]
    fn portable_layout_keeps_resources_next_to_the_executable() {
        assert_eq!(
            resources_dir_for_executable(Path::new("/opt/oca/ui")),
            Some(PathBuf::from("/opt/oca/resources"))
        );
    }
}
