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

/// Extraction budgets for collaboration imports. Project data additionally follows the
/// persistence format's encoded/decompressed size limits.
#[derive(Debug, Clone, Copy)]
pub struct CollabBundleImportLimits {
    /// Maximum archive entries, including entries that will not be extracted.
    pub max_entries: usize,
    /// Maximum combined bytes of extracted proxy files, independent of ZIP size metadata.
    pub max_proxy_bytes: u64,
}

impl Default for CollabBundleImportLimits {
    fn default() -> Self {
        // Handoffs may contain many hours of encoded video. These are safety ceilings,
        // not performance targets; callers can supply a smaller disk budget or a larger handoff.
        Self {
            max_entries: 10_000,
            max_proxy_bytes: 64 * 1024 * 1024 * 1024,
        }
    }
}

#[derive(Debug)]
pub enum CollabBundleError {
    Io(std::io::Error),
    Zip(zip::result::ZipError),
    Persist(PersistError),
    Invalid(&'static str),
}

impl std::fmt::Display for CollabBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollabBundleError::Io(e) => write!(f, "failed to access the bundle file: {e}"),
            CollabBundleError::Zip(e) => write!(f, "failed to read/write the bundle archive: {e}"),
            CollabBundleError::Persist(e) => write!(f, "failed to (de)serialize the project: {e}"),
            CollabBundleError::Invalid(reason) => {
                write!(f, "invalid collaboration bundle: {reason}")
            }
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
/// Uses [`CollabBundleImportLimits::default`]: at most 10,000 entries and 64 GiB of proxies.
/// Validates and stages all extracted files before replacing the destination project.
/// A successful import replaces that project's proxy cache with the bundled proxies, preventing
/// unrelated cached files from being reused. Callers must serialize imports to the same path.
pub fn import_collab_bundle(
    zip_path: &Path,
    dest_project_path: &Path,
) -> Result<Project, CollabBundleError> {
    import_collab_bundle_with_limits(
        zip_path,
        dest_project_path,
        CollabBundleImportLimits::default(),
    )
}

/// Imports with an explicit extraction budget. The project is replaced atomically only after
/// proxy publication succeeds. On publication failure, the previous cache is restored; if
/// restoration also fails, the error identifies the retained recovery directory.
/// Project/cache replacement is not one crash-atomic filesystem transaction.
pub fn import_collab_bundle_with_limits(
    zip_path: &Path,
    dest_project_path: &Path,
    limits: CollabBundleImportLimits,
) -> Result<Project, CollabBundleError> {
    let file = File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    if zip.len() > limits.max_entries {
        return Err(CollabBundleError::Invalid("too many archive entries"));
    }

    let project_bytes = {
        let entry = zip.by_name(PROJECT_ENTRY_NAME)?;
        if entry.size() > persistence::MAX_PROJECT_FILE_BYTES {
            return Err(CollabBundleError::Invalid("project entry is too large"));
        }
        let mut bytes = Vec::new();
        entry
            .take(persistence::MAX_PROJECT_FILE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > persistence::MAX_PROJECT_FILE_BYTES {
            return Err(CollabBundleError::Invalid("project entry is too large"));
        }
        bytes
    };
    let mut project: Project = persistence::from_project_bytes(&project_bytes)?;
    if project.active_sequence >= project.sequences.len() {
        return Err(CollabBundleError::Invalid("active sequence is missing"));
    }
    project.file_path = Some(dest_project_path.to_path_buf());

    let parent = dest_project_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staging = tempfile::tempdir_in(parent)?;
    let staged_proxies = staging.path().join("proxies");
    fs::create_dir(&staged_proxies)?;
    let proxy_dir = proxy::cache_dir_for_project(&project);
    let mut proxy_bytes = 0u64;
    for index in 0..zip.len() {
        let entry = zip.by_index_raw(index)?;
        if entry.is_dir() {
            continue;
        }
        let Some(file_name) = entry.name().strip_prefix(PROXIES_ENTRY_PREFIX) else {
            continue;
        };
        // Preserve flat-name extraction, but reject collisions instead of silently overwriting
        // an earlier staged entry. All files are created in a fresh, private staging directory.
        let Some(safe_name) = safe_proxy_entry_filename(file_name) else {
            continue;
        };
        let remaining = limits.max_proxy_bytes - proxy_bytes;
        if entry.size() > remaining {
            return Err(CollabBundleError::Invalid(
                "proxy extraction budget exceeded",
            ));
        }
        // Open a decoder only for an entry that will actually be extracted.
        drop(entry);
        let entry = zip.by_index(index)?;
        let mut out = File::create_new(staged_proxies.join(safe_name))?;
        let copied = io::copy(&mut entry.take(remaining.saturating_add(1)), &mut out)?;
        if copied > remaining {
            return Err(CollabBundleError::Invalid(
                "proxy extraction budget exceeded",
            ));
        }
        proxy_bytes += copied;
        out.sync_all()?;
    }

    for asset in &mut project.media_library {
        asset.proxy_path = [
            PreviewQuality::High,
            PreviewQuality::Medium,
            PreviewQuality::Low,
        ]
        .into_iter()
        .map(|quality| proxy::proxy_path_for(&asset.source_path, &proxy_dir, quality))
        .find(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| staged_proxies.join(name).is_file())
        });
    }

    let mut staged_project = tempfile::NamedTempFile::new_in(staging.path())?;
    staged_project.write_all(&project_bytes)?;
    staged_project.as_file().sync_all()?;
    publish_import(staging, staged_project, dest_project_path, &proxy_dir)?;
    Ok(project)
}

fn publish_import(
    staging: tempfile::TempDir,
    project: tempfile::NamedTempFile,
    dest_project_path: &Path,
    proxy_dir: &Path,
) -> Result<(), CollabBundleError> {
    let backup = staging.path().join("previous-proxies");
    let had_cache = match fs::symlink_metadata(proxy_dir) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => return Err(CollabBundleError::Invalid("proxy cache is not a directory")),
        Err(error) if error.kind() == io::ErrorKind::NotFound => false,
        Err(error) => return Err(error.into()),
    };
    if had_cache {
        fs::rename(proxy_dir, &backup)?;
    }
    let mut installed_cache = false;
    let published = fs::rename(staging.path().join("proxies"), proxy_dir).and_then(|()| {
        installed_cache = true;
        project
            .persist(dest_project_path)
            .map(drop)
            .map_err(|error| error.error)
    });
    if let Err(publish_error) = published {
        let restored = (|| {
            if installed_cache {
                fs::rename(proxy_dir, staging.path().join("failed-proxies"))?;
            }
            if had_cache {
                fs::rename(&backup, proxy_dir)?;
            }
            Ok::<(), io::Error>(())
        })();
        if let Err(restore_error) = restored {
            let recovery = staging.keep();
            return Err(io::Error::other(format!(
                "bundle publication failed: {publish_error}; cache restoration failed: \
                 {restore_error}; recovery files retained at {}",
                recovery.display()
            ))
            .into());
        }
        return Err(publish_error.into());
    }
    Ok(())
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
