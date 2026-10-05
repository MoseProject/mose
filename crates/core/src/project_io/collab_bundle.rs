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

//! D7 (`spec/architecture/differentiators.md`): packages a project handoff bundle — the
//! project's `.ocproj` snapshot plus its already-generated editing proxies (see
//! [`crate::proxy`]) — into one portable `.zip`, so handing an edit off to a collaborator never
//! means moving the multi-GB source recordings around. [`export_collab_bundle`] only ever reads
//! what's already on disk (nothing is (re)transcoded); [`import_collab_bundle`] unpacks it back
//! into the shape the recipient's own app already knows how to find (the same
//! [`crate::proxy::cache_dir_for_project`] location their app would compute for wherever they
//! choose to save the project), so their preview picks the proxies up with no extra wiring —
//! even though the original source is never part of the bundle and likely isn't at
//! [`crate::media::MediaAsset::source_path`] on their machine at all.

use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use crate::persistence::{self, PersistError};
use crate::project::Project;
use crate::proxy::{self, PreviewQuality};

const PROJECT_ENTRY_NAME: &str = "project.ocproj";
const PROXIES_ENTRY_PREFIX: &str = "proxies/";

#[derive(Debug)]
pub enum CollabBundleError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Persist(PersistError),
}

impl std::fmt::Display for CollabBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollabBundleError::Io(e) => write!(f, "failed to access the bundle file: {e}"),
            CollabBundleError::Zip(e) => write!(f, "failed to read/write the bundle archive: {e}"),
            CollabBundleError::Persist(e) => write!(f, "failed to (de)serialize the project: {e}"),
        }
    }
}

impl std::error::Error for CollabBundleError {}

impl From<std::io::Error> for CollabBundleError {
    fn from(e: std::io::Error) -> Self {
        CollabBundleError::Io(e)
    }
}

impl From<zip::result::ZipError> for CollabBundleError {
    fn from(e: zip::result::ZipError) -> Self {
        CollabBundleError::Zip(e)
    }
}

impl From<PersistError> for CollabBundleError {
    fn from(e: PersistError) -> Self {
        CollabBundleError::Persist(e)
    }
}

/// Packages `project`'s current proxy cache
/// ([`proxy::cache_dir_for_project`]) plus a `.ocproj` snapshot of `project` itself into one
/// `.zip` at `output_zip_path` — a portable handoff bundle. Only proxy files that already exist
/// on disk are included; an asset with no proxy yet (background enrichment hasn't reached it
/// yet, or it's audio-only) is simply absent from the bundle rather than triggering a fresh
/// transcode — this packages what already exists, per the D7 spec, it doesn't generate
/// anything new. The recipient's preview for that one clip just won't work until they get the
/// original media or a proxy some other way — the same "known gap, not silently wrong"
/// tolerance every other partial-enrichment path in this codebase already has.
pub fn export_collab_bundle(
    project: &Project,
    output_zip_path: &Path,
) -> Result<(), CollabBundleError> {
    let proxy_dir = proxy::cache_dir_for_project(project);
    let project_bytes = persistence::to_project_bytes(project)?;

    let file = File::create(output_zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    // Every entry here (already-gzip-compressed `.ocproj` bytes, already-encoded proxy `.mp4`s)
    // is incompressible in practice, and `default-features = false` on the `zip` dependency
    // means no codec but Stored is even linked in.
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip.start_file(PROJECT_ENTRY_NAME, options)?;
    zip.write_all(&project_bytes)?;

    if proxy_dir.is_dir() {
        for entry in fs::read_dir(&proxy_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_file() {
                continue;
            }
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            zip.start_file(format!("{PROXIES_ENTRY_PREFIX}{name}"), options)?;
            let mut proxy_file = File::open(entry.path())?;
            io::copy(&mut proxy_file, &mut zip)?;
        }
    }

    zip.finish()?;
    Ok(())
}

/// Reduces a zip entry's embedded path (everything after `proxies/`) down to a single safe
/// filename component, or `None` if it doesn't have one. `export_collab_bundle` only ever writes
/// flat filenames here (no subdirectories), so a legitimate bundle never needs anything this
/// throws away; a malicious one (`../../etc/cron.d/evil`, an absolute path, a bare `..`) gets
/// exactly the same treatment as a plain filename, since [`Path::file_name`] already normalizes
/// away `.`/`..` components and any leading root/prefix, returning only the last real segment.
fn safe_proxy_entry_filename(embedded_path: &str) -> Option<PathBuf> {
    Path::new(embedded_path).file_name().map(PathBuf::from)
}

/// Unpacks a bundle written by [`export_collab_bundle`]: writes its `.ocproj` snapshot to
/// `dest_project_path` and its bundled proxy files into that path's own proxy cache dir (see
/// [`proxy::cache_dir_for_project`]) — the same dir the recipient's own app computes for
/// wherever they save the project, so nothing beyond this function needs to know a bundle was
/// ever involved. Each media asset's `proxy_path` is populated to whichever unpacked proxy
/// matches its `source_path`'s filename stem (checked across every [`PreviewQuality`], highest
/// first), if any — this is what lets preview work immediately without a source file present.
/// Returns the loaded [`Project`] with `file_path` already set to `dest_project_path`, matching
/// [`persistence::load_project_from_file`]'s convention of leaving that to the caller otherwise.
pub fn import_collab_bundle(
    zip_path: &Path,
    dest_project_path: &Path,
) -> Result<Project, CollabBundleError> {
    let file = File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(file)?;

    let project_bytes = {
        let mut entry = zip.by_name(PROJECT_ENTRY_NAME)?;
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        bytes
    };
    if let Some(parent) = dest_project_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(dest_project_path, &project_bytes)?;

    let mut project: Project = persistence::from_project_bytes(&project_bytes)?;
    project.file_path = Some(dest_project_path.to_path_buf());

    let proxy_dir = proxy::cache_dir_for_project(&project);
    fs::create_dir_all(&proxy_dir)?;

    let proxy_entry_names: Vec<String> = zip
        .file_names()
        .filter(|n| n.starts_with(PROXIES_ENTRY_PREFIX) && n.len() > PROXIES_ENTRY_PREFIX.len())
        .map(str::to_string)
        .collect();
    for name in proxy_entry_names {
        let file_name = &name[PROXIES_ENTRY_PREFIX.len()..];
        // A bundle is attacker-controllable data (shared by a collaborator, or downloaded from
        // anywhere) -- never trust an embedded entry path. `safe_proxy_entry_filename` keeps only
        // the final path component, discarding any `../`/absolute-path segments a malicious
        // archive might embed to write outside `proxy_dir` (zip-slip, CWE-22); an entry with no
        // valid basename at all (e.g. `..`, `.`, or a bare separator) is skipped rather than
        // guessed at, matching this function's existing "known gap, not silently wrong" tolerance
        // for other partial/malformed bundle contents.
        let Some(safe_name) = safe_proxy_entry_filename(file_name) else {
            continue;
        };
        let mut entry = zip.by_name(&name)?;
        let mut out = File::create(proxy_dir.join(safe_name))?;
        io::copy(&mut entry, &mut out)?;
    }

    for asset in &mut project.media_library {
        asset.proxy_path = [
            PreviewQuality::High,
            PreviewQuality::Medium,
            PreviewQuality::Low,
        ]
        .into_iter()
        .map(|quality| proxy::proxy_path_for(&asset.source_path, &proxy_dir, quality))
        .find(|candidate| candidate.exists());
    }

    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_plain_filename_unchanged() {
        assert_eq!(
            safe_proxy_entry_filename("clip.mp4"),
            Some(PathBuf::from("clip.mp4"))
        );
    }

    #[test]
    fn strips_a_relative_traversal_prefix_down_to_the_basename() {
        assert_eq!(
            safe_proxy_entry_filename("../../../etc/cron.d/evil"),
            Some(PathBuf::from("evil"))
        );
    }

    #[test]
    fn strips_an_absolute_unix_path_down_to_the_basename() {
        assert_eq!(
            safe_proxy_entry_filename("/etc/passwd"),
            Some(PathBuf::from("passwd"))
        );
    }

    #[test]
    fn rejects_a_bare_parent_dir_component() {
        assert_eq!(safe_proxy_entry_filename(".."), None);
    }

    #[test]
    fn rejects_a_bare_current_dir_component() {
        assert_eq!(safe_proxy_entry_filename("."), None);
    }

    #[test]
    fn rejects_a_trailing_traversal_with_nothing_after_it() {
        assert_eq!(safe_proxy_entry_filename("proxies/.."), None);
    }

    #[test]
    fn keeps_the_final_component_of_a_nested_but_otherwise_ordinary_path() {
        // `export_collab_bundle` never nests entries under a subdirectory, but even if a bundle
        // did, only the last component should ever end up in the destination path.
        assert_eq!(
            safe_proxy_entry_filename("sub/dir/clip.mp4"),
            Some(PathBuf::from("clip.mp4"))
        );
    }
}
