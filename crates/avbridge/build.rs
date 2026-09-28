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

//! Locates FFmpeg (libavformat/libavcodec/libavutil) via the `FFMPEG_DIR` env var
//! (`FFMPEG_DIR/include`, `FFMPEG_DIR/lib`) and compiles `csrc/*.c` against it.
//!
//! On Windows the import libs are copied into OUT_DIR under unique names to avoid the
//! GStreamer-vs-FFmpeg naming conflict (see comment below).  On macOS/Linux we link
//! directly with -L/-l; the naming conflict does not arise there.

use std::env;
use std::path::{Path, PathBuf};

fn is_ffmpeg_dev_root(path: &Path) -> bool {
    let include_dir = path.join("include");
    let lib_dir = path.join("lib");
    include_dir.join("libavcodec").join("avcodec.h").is_file()
        && lib_dir.is_dir()
        && (lib_dir.join("avcodec.lib").is_file()
            || lib_dir.join("libavcodec.dll.a").is_file()
            || lib_dir.join("libavcodec.so").is_file()
            || lib_dir.join("libavcodec.dylib").is_file())
}

fn find_ffmpeg_dir() -> PathBuf {
    if let Some(explicit) = env::var_os("FFMPEG_DIR") {
        let path = PathBuf::from(explicit);
        if is_ffmpeg_dev_root(&path) {
            return path;
        }
        panic!(
            "FFMPEG_DIR={} is not an FFmpeg development root: expected include/ and lib/.",
            path.display()
        );
    }

    // A normal BtbN/shared FFmpeg installation exposes <root>/bin on PATH. Deriving the root
    // from that entry avoids a user-specific path or a separate environment-variable setup.
    if let Some(path) = env::var_os("PATH") {
        for entry in env::split_paths(&path) {
            let root = entry
                .parent()
                .filter(|_| entry.file_name().is_some_and(|name| name == "bin"));
            if let Some(root) = root.filter(|candidate| is_ffmpeg_dev_root(candidate)) {
                return root.to_path_buf();
            }
        }
    }

    panic!(
        "could not locate an FFmpeg development root. Install a shared FFmpeg build with bin/, \
         include/, and lib/ and put its bin/ directory on PATH, or set FFMPEG_DIR explicitly."
    );
}

fn main() {
    println!("cargo:rerun-if-env-changed=FFMPEG_DIR");
    println!("cargo:rerun-if-changed=csrc");

    println!("cargo:rerun-if-env-changed=PATH");
    let ffmpeg_dir = find_ffmpeg_dir();
    let include_dir = ffmpeg_dir.join("include");
    let lib_dir = ffmpeg_dir.join("lib");

    let mut build = cc::Build::new();
    build.include(&include_dir);
    for entry in std::fs::read_dir("csrc").expect("csrc/ directory should exist") {
        let path = entry.expect("failed to read csrc/ directory entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("c") {
            build.file(path);
        }
    }
    build.compile("avbridge_c");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let is_windows = target_os == "windows";

    if is_windows {
        // On Windows, GStreamer's SDK bundles its own FFmpeg build (gst-libav) with
        // generically named import libs (avformat.lib etc.) for a different, older FFmpeg
        // version.  If gstreamer-sys's -L also ends up on the link line a bare -lavformat
        // can silently resolve to the wrong copy — ABI mismatch that only surfaces at
        // runtime (see features/fase1/commit_plan.md, chore-002).
        //
        // Fix: copy the exact import libs into OUT_DIR under unique names and link those,
        // so bare-name search always finds our copy first.  cargo:rustc-link-arg with a
        // full path would also work, but it does NOT propagate from a lib crate's build
        // script to downstream binary link steps — only rustc-link-lib/search do.
        let is_msvc = env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR set by cargo"));
        for name in ["avfilter", "avformat", "avcodec", "swscale", "avutil"] {
            let (src_name, unique_name) = if is_msvc {
                (format!("{name}.lib"), format!("avbridge_ffmpeg_{name}.lib"))
            } else {
                (
                    format!("lib{name}.dll.a"),
                    format!("libavbridge_ffmpeg_{name}.dll.a"),
                )
            };
            let src_path = lib_dir.join(&src_name);
            if !src_path.is_file() {
                panic!(
                    "expected {} to exist under FFMPEG_DIR/lib ({})",
                    src_name,
                    lib_dir.display()
                );
            }
            std::fs::copy(&src_path, out_dir.join(&unique_name)).unwrap_or_else(|e| {
                panic!("failed to copy {} into OUT_DIR: {e}", src_path.display())
            });
            println!("cargo:rustc-link-lib=dylib=avbridge_ffmpeg_{name}");
        }
        println!("cargo:rustc-link-search=native={}", out_dir.display());
    } else {
        // macOS / Linux: no GStreamer naming conflict; link directly.
        // Verify the expected shared-lib files exist so the error is actionable.
        let (ext, prefix) = if target_os == "macos" {
            ("dylib", "lib")
        } else {
            ("so", "lib")
        };
        for name in ["avfilter", "avformat", "avcodec", "swscale", "avutil"] {
            let file_name = format!("{prefix}{name}.{ext}");
            // On macOS Homebrew the dylibs are symlinks like libavformat.dylib -> libavformat.61.dylib.
            // Accept either the versioned or unversioned name; just check the directory has something.
            let found = std::fs::read_dir(&lib_dir)
                .map(|mut d| {
                    d.any(|e| {
                        e.map(|e| {
                            e.file_name()
                                .to_string_lossy()
                                .starts_with(&format!("{prefix}{name}."))
                        })
                        .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if !found {
                panic!(
                    "expected {file_name} (or a versioned variant) under FFMPEG_DIR/lib ({})",
                    lib_dir.display()
                );
            }
            println!("cargo:rustc-link-lib=dylib={name}");
        }
        println!("cargo:rustc-link-search=native={}", lib_dir.display());
    }
}
