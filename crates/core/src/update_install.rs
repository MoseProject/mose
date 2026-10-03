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

//! The two update steps that cannot run without a published GitHub release and a replaceable
//! installation: downloading and swapping the executable through `self_update`, and starting
//! the freshly installed copy. Everything that decides *whether* to do either (version
//! validation, asset naming, platform support) lives in [`crate::update_check`] and is tested
//! there; this file is excluded from coverage as a process and network boundary.

use crate::update_check::{
    ApplyUpdateError, ApplyUpdateOutcome, InstallPlan, UpdatePackage, BINARY_NAME, REPO_NAME,
    REPO_OWNER,
};

pub(crate) fn install(plan: &InstallPlan<'_>) -> Result<ApplyUpdateOutcome, ApplyUpdateError> {
    let mut update_builder = self_update::backends::github::Update::configure();
    update_builder
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .current_version(env!("CARGO_PKG_VERSION"))
        .target_version_tag(plan.tag)
        .show_download_progress(false)
        .show_output(false)
        .no_confirm(true);
    match plan.package {
        UpdatePackage::NativeBinary => {
            update_builder.bin_name(BINARY_NAME);
        }
        UpdatePackage::LinuxAppImage => {
            let appimage = std::env::var_os("APPIMAGE")
                .filter(|path| !path.is_empty())
                .ok_or(ApplyUpdateError::UnsupportedPlatform)?;
            update_builder
                .identifier("appimage")
                .bin_name("oca.AppImage")
                .bin_path_in_archive("oca.AppImage")
                .bin_install_path(appimage);
        }
    }
    let updater = update_builder
        .build()
        .map_err(|error| ApplyUpdateError::Update(error.to_string()))?;

    // `self_update::Release::asset_for` intentionally falls back from a full target match to
    // OS/arch and finally an identifier. Validate what it would select before update() performs
    // any download, keeping a malformed release from crossing platform boundaries.
    let release = updater
        .get_release_version(plan.tag)
        .map_err(|error| ApplyUpdateError::Update(error.to_string()))?;
    let selected = release.asset_for(plan.target, updater.identifier().as_deref());
    if selected.as_ref().map(|asset| asset.name.as_str()) != Some(plan.expected_asset) {
        return Err(ApplyUpdateError::MissingAsset {
            expected: plan.expected_asset.to_string(),
        });
    }

    let status = updater
        .update()
        .map_err(|error| ApplyUpdateError::Update(error.to_string()))?;
    if status.updated() {
        Ok(ApplyUpdateOutcome::Updated {
            version: status.version().trim_start_matches('v').to_string(),
        })
    } else {
        Ok(ApplyUpdateOutcome::UpToDate)
    }
}

/// Starts the executable currently installed at this process's path, preserving command-line
/// arguments. The caller must close the old process only after this returns successfully.
pub fn restart_application() -> Result<(), ApplyUpdateError> {
    #[cfg(target_os = "linux")]
    let executable = match std::env::var_os("APPIMAGE").filter(|path| !path.is_empty()) {
        Some(path) => std::path::PathBuf::from(path),
        None => {
            std::env::current_exe().map_err(|error| ApplyUpdateError::Restart(error.to_string()))?
        }
    };
    #[cfg(not(target_os = "linux"))]
    let executable =
        std::env::current_exe().map_err(|error| ApplyUpdateError::Restart(error.to_string()))?;
    std::process::Command::new(executable)
        .args(std::env::args_os().skip(1))
        .spawn()
        .map_err(|error| ApplyUpdateError::Restart(error.to_string()))?;
    Ok(())
}
