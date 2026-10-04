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

//! The environment-driven half of the bundle lookup. It lives in its own test binary with a
//! single environment-changing test because it changes process-wide environment variables.

use avcore::{
    bundled_resource_path, bundled_resources_dir, configure_bundled_runtime, BundledResource,
    MissingBundleResources,
};

#[test]
fn the_resource_override_drives_lookup_and_the_runtime_path() {
    let root = std::env::temp_dir().join(format!("oca_bundle_runtime_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("models")).unwrap();
    std::env::set_var("OCA_RESOURCE_DIR", &root);

    assert_eq!(bundled_resources_dir(), Some(root.clone()));

    // A resource is only reported once its file exists.
    assert_eq!(bundled_resource_path(BundledResource::WhisperBase), None);
    std::fs::write(root.join("models/ggml-base.bin"), b"model").unwrap();
    assert_eq!(
        bundled_resource_path(BundledResource::WhisperBase),
        Some(root.join("models/ggml-base.bin"))
    );

    // Without a runtime/bin folder the search path is left alone.
    let before = std::env::var_os("PATH");
    configure_bundled_runtime();
    assert_eq!(std::env::var_os("PATH"), before);

    // With one, it is prepended so bundled tools win over system ones.
    let tools = root.join("runtime").join("bin");
    std::fs::create_dir_all(&tools).unwrap();
    configure_bundled_runtime();
    let path = std::env::var_os("PATH").unwrap();
    assert_eq!(std::env::split_paths(&path).next(), Some(tools));

    std::env::remove_var("OCA_RESOURCE_DIR");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_bundle_reports_how_many_resources_are_absent() {
    let error = MissingBundleResources {
        paths: vec!["a".into(), "b".into(), "c".into()],
    };
    assert!(error.to_string().contains("3 required resource"));
}
