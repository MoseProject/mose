// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>

use std::fs;

use avcore::{resource_path_in, validate_bundled_resources, BundledResource};

fn temp_dir(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("oca_bundle_test_{name}_{}", std::process::id()))
}

#[test]
fn resource_paths_use_the_versioned_bundle_layout() {
    let root = std::path::Path::new("resources");
    assert_eq!(
        resource_path_in(root, BundledResource::WhisperBase),
        root.join("models/ggml-base.bin")
    );
    assert_eq!(
        resource_path_in(root, BundledResource::TtsVoiceConfig),
        root.join("models/pt_BR-faber-medium.onnx.json")
    );
}

#[test]
fn validation_reports_every_missing_model() {
    let root = temp_dir("missing");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let error = validate_bundled_resources(&root).unwrap_err();
    assert_eq!(error.paths.len(), BundledResource::REQUIRED.len());
    assert!(error.paths.iter().all(|path| path.starts_with(&root)));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validation_accepts_a_complete_model_bundle() {
    let root = temp_dir("complete");
    let _ = fs::remove_dir_all(&root);
    for resource in BundledResource::REQUIRED {
        let path = resource_path_in(&root, resource);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"fixture").unwrap();
    }

    validate_bundled_resources(&root).unwrap();
    fs::remove_dir_all(root).unwrap();
}
