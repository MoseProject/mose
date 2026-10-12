// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! Guards on the repository itself: a test file nothing compiles never fails, so it protects
//! nothing. Ten tests of the preview protocol went unrun for weeks after a module regrouping
//! because the new parent no longer pointed at their file.

use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            if !matches!(name.as_ref(), "target" | "node_modules" | ".git" | "dist") {
                rust_files(&path, found);
            }
        } else if name.ends_with(".rs") {
            found.push(path);
        }
    }
}

#[test]
fn every_unit_test_file_is_compiled_by_something() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    rust_files(&root.join("crates"), &mut files);
    rust_files(&root.join("apps"), &mut files);

    let sources: Vec<(PathBuf, String)> = files
        .iter()
        .map(|path| {
            (
                path.clone(),
                std::fs::read_to_string(path).unwrap_or_default(),
            )
        })
        .collect();
    let mut orphans = Vec::new();
    for (path, _) in &sources {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        // Integration tests are declared in `tests/it/main.rs` or are their own targets; only the
        // `*_test.rs` files that sit next to the code they test need a `#[path]` or a `mod`.
        if !name.ends_with("_test.rs") || path.components().any(|part| part.as_os_str() == "tests")
        {
            continue;
        }
        let stem = name.trim_end_matches(".rs");
        let referenced = sources.iter().any(|(other, text)| {
            other != path
                && (text.contains(&format!("\"{name}\""))
                    || text.contains(&format!("/{name}\""))
                    || text.contains(&format!("mod {stem};")))
        });
        if !referenced {
            orphans.push(
                path.strip_prefix(&root)
                    .unwrap_or(path)
                    .display()
                    .to_string(),
            );
        }
    }
    assert!(
        orphans.is_empty(),
        "test files that no `#[path = \"...\"]` or `mod` includes, so they never run: {orphans:?}"
    );
}
