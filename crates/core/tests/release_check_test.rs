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

//! Network access and executable replacement are deliberately excluded. The pure version and
//! release-asset contract are covered here; the UI tests inject updater completion events rather
//! than mutating the test runner's executable.

use avcore::{
    auto_update_supported, expected_update_asset_name, is_newer, package_supports_atomic_update,
    release_supports_auto_update, UpdatePackage,
};

#[test]
fn a_newer_patch_version_is_newer() {
    assert!(is_newer("0.1.0", "0.1.1"));
}

#[test]
fn a_newer_minor_version_is_newer() {
    assert!(is_newer("0.1.9", "0.2.0"));
}

#[test]
fn a_newer_major_version_is_newer() {
    assert!(is_newer("0.9.9", "1.0.0"));
}

#[test]
fn the_same_version_is_not_newer() {
    assert!(!is_newer("1.2.3", "1.2.3"));
}

#[test]
fn an_older_version_is_not_newer() {
    assert!(!is_newer("1.2.3", "1.2.2"));
}

#[test]
fn a_shorter_latest_version_compares_missing_segments_as_zero() {
    // "1.3" reads as "1.3.0" - not newer than "1.3.0", but newer than "1.2.9".
    assert!(!is_newer("1.3.0", "1.3"));
    assert!(is_newer("1.2.9", "1.3"));
}

#[test]
fn non_numeric_suffixes_do_not_panic_and_are_stripped_from_each_segment() {
    // "3-beta"'s leading digits ("3") are what gets compared - the "-beta" tail is ignored,
    // not treated as an error or a fourth segment.
    assert!(!is_newer("1.2.3", "1.2.3-beta"));
    assert!(is_newer("1.2.3", "1.2.4-beta"));
}

#[test]
fn a_segment_with_no_leading_digits_at_all_reads_as_zero() {
    assert!(!is_newer("1.2.0", "1.2.beta"));
}

#[test]
fn empty_strings_do_not_panic() {
    assert!(!is_newer("", ""));
    assert!(!is_newer("1.0.0", ""));
}

#[test]
fn windows_target_uses_zip_release_asset() {
    assert_eq!(
        expected_update_asset_name("x86_64-pc-windows-msvc", UpdatePackage::NativeBinary),
        Some("oca-x86_64-pc-windows-msvc.zip".to_string())
    );
}

#[test]
fn linux_target_uses_tar_gz_release_asset() {
    assert_eq!(
        expected_update_asset_name("x86_64-unknown-linux-gnu", UpdatePackage::NativeBinary),
        Some("oca-x86_64-unknown-linux-gnu.tar.gz".to_string())
    );
}

#[test]
fn unsupported_target_has_no_automatic_update_asset() {
    assert_eq!(
        expected_update_asset_name("aarch64-apple-darwin", UpdatePackage::NativeBinary),
        None
    );
}

#[test]
fn linux_appimage_uses_a_distinct_tar_gz_release_asset() {
    assert_eq!(
        expected_update_asset_name("x86_64-unknown-linux-gnu", UpdatePackage::LinuxAppImage),
        Some("oca-x86_64-unknown-linux-gnu-appimage.tar.gz".to_string())
    );
    assert_eq!(
        expected_update_asset_name("x86_64-pc-windows-msvc", UpdatePackage::LinuxAppImage),
        None
    );
}

#[test]
fn runtime_support_matches_the_packaged_platform_contract() {
    assert_eq!(
        auto_update_supported(),
        cfg!(target_os = "linux")
            && std::env::var_os("APPIMAGE").is_some_and(|path| !path.is_empty())
    );
    assert!(!package_supports_atomic_update(UpdatePackage::NativeBinary));
    assert!(package_supports_atomic_update(UpdatePackage::LinuxAppImage));
}

#[test]
fn auto_update_requires_an_exact_target_asset_name() {
    let assets = [
        "oca-x86_64-pc-windows-msvc.zip.sha256",
        "oca-aarch64-pc-windows-msvc.zip",
        "oca-x86_64-unknown-linux-gnu.tar.gz",
    ];

    assert!(!release_supports_auto_update(
        assets,
        "x86_64-pc-windows-msvc",
        UpdatePackage::NativeBinary,
    ));
    assert!(!release_supports_auto_update(
        assets,
        "x86_64-unknown-linux-gnu",
        UpdatePackage::NativeBinary,
    ));
    assert!(release_supports_auto_update(
        ["oca-x86_64-unknown-linux-gnu-appimage.tar.gz"],
        "x86_64-unknown-linux-gnu",
        UpdatePackage::LinuxAppImage,
    ));
}
