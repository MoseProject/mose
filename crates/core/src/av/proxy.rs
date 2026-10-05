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

//! Generates lightweight "proxy" transcodes for editing/scrubbing — the same trick CapCut,
//! Premiere and Resolve use: decode a small, easy-to-seek proxy file while editing, then
//! conform back to the original full-resolution source only at export time. Keeps timeline
//! scrubbing and preview responsive on large 4K/60fps gameplay captures that would otherwise
//! be expensive to decode every frame.
//!
//! Like [`crate::probe`]/[`crate::render`], the transcode ([`ensure_proxy`]) goes through
//! `oca-avbridge`'s FFI — no subprocess — wrapped around a pure path/freshness check
//! ([`proxy_path_for`], [`is_up_to_date`]).

use std::fs;
use std::path::{Path, PathBuf};

use crate::project::Project;

/// User-selectable editing-proxy/preview resolution (`request.md`'s Fase 7 "Qualidade do
/// preview selecionável" — 360p/480p/720p, capped at 720p). Width always follows the source's
/// aspect ratio, same as the old fixed-height behavior this replaces. Only affects proxies
/// generated *after* the preference changes — an asset that already has a proxy from a
/// previous quality keeps it until re-imported, since nothing re-triggers `ensure_proxy` for
/// already-imported assets. [`proxy_path_for`] bakes the height into the filename precisely so
/// switching quality can never silently keep serving a stale-resolution file under the same
/// path (mtime-based freshness alone couldn't tell the two apart).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl PreviewQuality {
    /// Target proxy height in pixels — matches the resolution most NLEs default proxies to at
    /// [`PreviewQuality::Medium`]: enough detail to judge framing and cuts, cheap enough to
    /// decode in real time while scrubbing. [`PreviewQuality::High`] is the 720p cap
    /// `request.md` specifies — "enough to judge framing, text and color without forcing the
    /// preview to decode at full resolution."
    pub fn height(self) -> u32 {
        match self {
            PreviewQuality::Low => 360,
            PreviewQuality::Medium => 480,
            PreviewQuality::High => 720,
        }
    }
}

#[derive(Debug)]
pub enum ProxyError {
    /// Couldn't create the proxy cache directory.
    Io(std::io::Error),
    /// `oca-avbridge` failed before or during the decode/scale/encode pipeline.
    Bridge(avbridge::ProxyError),
}

impl std::fmt::Display for ProxyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProxyError::Io(e) => write!(f, "failed to prepare the proxy cache directory: {e}"),
            ProxyError::Bridge(e) => write!(f, "failed to generate proxy: {e}"),
        }
    }
}

impl std::error::Error for ProxyError {}

/// Where a project's imported clips' editing proxies are cached: a hidden sibling folder next
/// to the project file (`myproject.json` -> `.myproject_proxies/`), or a temp folder for a
/// project that hasn't been saved yet (proxies there won't survive a reboot, but neither would
/// anything else about an unsaved project).
pub fn cache_dir_for_project(project: &Project) -> PathBuf {
    match &project.file_path {
        Some(path) => {
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("project");
            path.with_file_name(format!(".{stem}_proxies"))
        }
        None => std::env::temp_dir().join("oca_unsaved_proxies"),
    }
}

/// The proxy file's path for `source` inside `proxy_dir` at `quality`, without checking
/// whether it exists yet. Pure so it's usable both to look up an existing proxy and to test
/// the naming scheme. The height is part of the filename (not just an internal encode
/// parameter) so that two different [`PreviewQuality`] choices for the same source never
/// collide on one path — switching quality always resolves to a distinct file rather than
/// silently overwriting or reusing a proxy encoded at the previous height.
pub fn proxy_path_for(source: &Path, proxy_dir: &Path, quality: PreviewQuality) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("clip");
    proxy_dir.join(format!("{stem}_proxy_{}p.mp4", quality.height()))
}

/// Returns the proxy for `source` at `quality`, generating it into `proxy_dir` via
/// `oca-avbridge`'s FFI first if it doesn't already exist or is older than the source (e.g.
/// the source was re-recorded or replaced). Skips the transcode entirely when the cached proxy
/// is already fresh.
pub fn ensure_proxy(
    source: &Path,
    proxy_dir: &Path,
    quality: PreviewQuality,
) -> Result<PathBuf, ProxyError> {
    let proxy_path = proxy_path_for(source, proxy_dir, quality);

    if is_up_to_date(source, &proxy_path) {
        return Ok(proxy_path);
    }

    fs::create_dir_all(proxy_dir).map_err(ProxyError::Io)?;

    avbridge::generate_proxy(source, &proxy_path, quality.height()).map_err(ProxyError::Bridge)?;

    Ok(proxy_path)
}

/// Whether `proxy_path` exists and was last modified at or after `source` — i.e. whether it's
/// safe to reuse instead of re-transcoding.
fn is_up_to_date(source: &Path, proxy_path: &Path) -> bool {
    let (Ok(source_meta), Ok(proxy_meta)) = (fs::metadata(source), fs::metadata(proxy_path)) else {
        return false;
    };
    let (Ok(source_modified), Ok(proxy_modified)) = (source_meta.modified(), proxy_meta.modified())
    else {
        return false;
    };
    proxy_modified >= source_modified
}

#[cfg(test)]
#[path = "proxy/proxy_test.rs"]
mod tests;
