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

//! Checks GitHub Releases and applies a selected release to the running installation.
//!
//! The lightweight check remains a single request per app launch. Applying is always initiated
//! explicitly by the user and delegates the archive download, extraction, and atomic replacement
//! to `self_update`. Since release bundles now contain native libraries and models, in-place
//! update is offered only for AppImage, where replacing one file replaces the complete bundle.
//! Native portable ZIP/TAR installations use the release page's full-package download instead.
//! Release assets follow one strict contract: an archive named
//! `oca-<Rust target triple>.zip` on Windows, `oca-<Rust target triple>.tar.gz` for a native
//! Linux binary, or `oca-<Rust target triple>-appimage.tar.gz` when running from AppImage. The
//! exact asset is validated before replacement so `self_update`'s permissive substring fallback
//! can never install another platform's package.

use serde::Deserialize;

/// GitHub's "latest release" endpoint for this project — 404s until the repository has at
/// least one published (non-draft, non-prerelease) release.
const RELEASES_API_URL: &str = "https://api.github.com/repos/lucasgmagalhaes/oca/releases/latest";
pub(crate) const REPO_OWNER: &str = "lucasgmagalhaes";
pub(crate) const REPO_NAME: &str = "oca";
pub(crate) const BINARY_NAME: &str = "ui";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LatestRelease {
    /// Normalized version string with any leading `v` stripped (GitHub tags are conventionally
    /// `v1.2.3`; `Cargo.toml`'s own `CARGO_PKG_VERSION` has none), for [`is_newer`] to compare
    /// against `CARGO_PKG_VERSION` on equal footing.
    pub version: String,
    /// The release's own GitHub page, retained as the manual fallback and details link.
    pub html_url: String,
    /// Whether this release contains the exact archive required by the running build target.
    pub auto_update_available: bool,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
    name: String,
}

#[derive(Debug)]
pub enum UpdateCheckError {
    /// The HTTP request itself failed (DNS, TLS, connection, non-2xx status — including a
    /// plain 404 for "no releases published yet").
    Request(String),
    /// The response didn't parse as the JSON shape GitHub's release API is documented to
    /// return.
    Parse(String),
}

/// Result of an explicitly requested in-place update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyUpdateOutcome {
    /// The running executable was replaced. The current process still contains the old image;
    /// call [`restart_application`] after the UI has told the user the update is ready.
    Updated { version: String },
    /// The selected release resolved to the already-installed version.
    UpToDate,
}

/// Installation shape used to select the release archive and executable path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdatePackage {
    NativeBinary,
    LinuxAppImage,
}

impl UpdatePackage {
    pub fn current() -> Self {
        #[cfg(target_os = "linux")]
        if std::env::var_os("APPIMAGE").is_some_and(|path| !path.is_empty()) {
            return Self::LinuxAppImage;
        }
        Self::NativeBinary
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyUpdateError {
    /// In-place updates are intentionally limited to the two packaged platforms in Fase 8.
    UnsupportedPlatform,
    /// Refuses an empty/malformed release version before placing it into a GitHub tag URL.
    InvalidVersion,
    /// The release exists but does not contain the archive required by this exact build target.
    MissingAsset { expected: String },
    /// Downloading, extracting, or atomically replacing the executable failed.
    Update(String),
    /// Starting the freshly installed executable failed.
    Restart(String),
}

impl std::fmt::Display for ApplyUpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(
                    f,
                    "automatic updates require an atomically replaceable AppImage bundle"
                )
            }
            Self::InvalidVersion => write!(f, "release version is invalid"),
            Self::MissingAsset { expected } => {
                write!(f, "release does not contain required asset `{expected}`")
            }
            Self::Update(error) => write!(f, "automatic update failed: {error}"),
            Self::Restart(error) => write!(f, "could not restart updated application: {error}"),
        }
    }
}

impl std::error::Error for ApplyUpdateError {}

impl std::fmt::Display for UpdateCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpdateCheckError::Request(e) => write!(f, "update check request failed: {e}"),
            UpdateCheckError::Parse(e) => write!(f, "update check response failed to parse: {e}"),
        }
    }
}

impl std::error::Error for UpdateCheckError {}

/// Fetches the latest published GitHub release for this project. A plain `ureq::get` + manual
/// `serde_json::from_str` rather than `ureq`'s own `into_json` — that method is gated behind a
/// `json` Cargo feature this crate doesn't enable, and every other dependency already used here
/// (`serde`/`serde_json`) covers the same ground without adding one.
pub fn fetch_latest_release() -> Result<LatestRelease, UpdateCheckError> {
    fetch_release_from(RELEASES_API_URL)
}

/// [`fetch_latest_release`] against an explicit endpoint, so tests can serve their own JSON.
pub(crate) fn fetch_release_from(url: &str) -> Result<LatestRelease, UpdateCheckError> {
    // GitHub's API rejects requests with no User-Agent header.
    let mut response = ureq::get(url)
        .header("User-Agent", "oca-update-check")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| UpdateCheckError::Request(e.to_string()))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|e| UpdateCheckError::Request(e.to_string()))?;
    let parsed: GithubRelease =
        serde_json::from_str(&body).map_err(|e| UpdateCheckError::Parse(e.to_string()))?;
    let auto_update_available = release_supports_auto_update(
        parsed.assets.iter().map(|asset| asset.name.as_str()),
        self_update::get_target(),
        UpdatePackage::current(),
    );
    Ok(LatestRelease {
        version: parsed.tag_name.trim_start_matches('v').to_string(),
        html_url: parsed.html_url,
        auto_update_available,
    })
}

/// Whether this installation can atomically replace its complete dependency bundle.
pub fn auto_update_supported() -> bool {
    cfg!(target_os = "linux") && package_supports_atomic_update(UpdatePackage::current())
}

pub const fn package_supports_atomic_update(package: UpdatePackage) -> bool {
    matches!(package, UpdatePackage::LinuxAppImage)
}

/// Exact release-asset name for a Rust target triple. Kept public and pure so release tooling
/// and integration tests can enforce the same naming contract without making a network call.
pub fn expected_update_asset_name(target: &str, package: UpdatePackage) -> Option<String> {
    let extension = if target.contains("windows") {
        "zip"
    } else if target.contains("linux") {
        "tar.gz"
    } else {
        return None;
    };
    let package_suffix = match package {
        UpdatePackage::NativeBinary => "",
        UpdatePackage::LinuxAppImage if target.contains("linux") => "-appimage",
        UpdatePackage::LinuxAppImage => return None,
    };
    Some(format!("oca-{target}{package_suffix}.{extension}"))
}

/// Whether an asset list contains the exact archive selected for `target`.
pub fn release_supports_auto_update<'a>(
    asset_names: impl IntoIterator<Item = &'a str>,
    target: &str,
    package: UpdatePackage,
) -> bool {
    if !package_supports_atomic_update(package) {
        return false;
    }
    let Some(expected) = expected_update_asset_name(target, package) else {
        return false;
    };
    asset_names.into_iter().any(|name| name == expected)
}

/// What [`apply_update`] hands to the installer once the request has been validated.
pub(crate) struct InstallPlan<'a> {
    pub tag: &'a str,
    pub target: &'a str,
    pub package: UpdatePackage,
    /// The exact release asset name this install must pick, per [`expected_update_asset_name`].
    pub expected_asset: &'a str,
}

/// A release version made safe to place into a GitHub tag URL: whitespace and a leading `v`
/// dropped, only alphanumerics, `.` and `-` allowed.
pub(crate) fn normalize_update_version(version: &str) -> Result<String, ApplyUpdateError> {
    let normalized = version.trim().trim_start_matches('v');
    if normalized.is_empty()
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '-'))
    {
        return Err(ApplyUpdateError::InvalidVersion);
    }
    Ok(normalized.to_string())
}

/// Downloads the requested GitHub release and atomically replaces the current executable.
///
/// This function blocks and must run on a worker thread. It does not restart or exit the
/// process: the UI remains responsive and can let the user choose when to restart.
pub fn apply_update(version: &str) -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
    apply_update_with(
        version,
        auto_update_supported(),
        UpdatePackage::current(),
        self_update::get_target(),
        crate::update_install::install,
    )
}

/// [`apply_update`] with the platform facts and the installer passed in.
pub(crate) fn apply_update_with(
    version: &str,
    supported: bool,
    package: UpdatePackage,
    target: &str,
    install: impl FnOnce(&InstallPlan<'_>) -> Result<ApplyUpdateOutcome, ApplyUpdateError>,
) -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
    if !supported {
        return Err(ApplyUpdateError::UnsupportedPlatform);
    }
    let normalized = normalize_update_version(version)?;
    if !package_supports_atomic_update(package) {
        return Err(ApplyUpdateError::UnsupportedPlatform);
    }
    let expected =
        expected_update_asset_name(target, package).ok_or(ApplyUpdateError::UnsupportedPlatform)?;
    let tag = format!("v{normalized}");
    install(&InstallPlan {
        tag: &tag,
        target,
        package,
        expected_asset: &expected,
    })
}
/// Whether `latest` is a newer version than `current` — both dotted `major.minor.patch`-style
/// strings. Compares each dot-separated segment's leading digits numerically, left to right;
/// a segment with no leading digits (e.g. a `-beta` suffix glued onto the last number instead
/// of separated by its own dot) reads as `0` rather than erroring, so this never panics on
/// unexpected input — worst case it just under- or over-estimates equality on a malformed
/// version, never crashes the update check.
pub fn is_newer(current: &str, latest: &str) -> bool {
    fn segment_value(segment: &str) -> u64 {
        segment
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse()
            .unwrap_or(0)
    }
    fn segments(v: &str) -> Vec<u64> {
        v.split('.').map(segment_value).collect()
    }

    let current_segments = segments(current);
    let latest_segments = segments(latest);
    let len = current_segments.len().max(latest_segments.len());
    for i in 0..len {
        let c = current_segments.get(i).copied().unwrap_or(0);
        let l = latest_segments.get(i).copied().unwrap_or(0);
        if l != c {
            return l > c;
        }
    }
    false
}

#[cfg(test)]
#[path = "update_check/update_check_test.rs"]
mod flow_tests;
