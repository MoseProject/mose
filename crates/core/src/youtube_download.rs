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

//! Downloads a YouTube video as MP3 (audio only) or MP4 (video), at a chosen quality, by
//! spawning the workspace's own `ytbridge` helper binary (`crates/ytbridge`) as a subprocess —
//! `ytbridge` is the thing that actually embeds Python and calls yt-dlp's `YoutubeDL` library
//! API directly (see its own module doc). This crate deliberately does **not** link `pyo3`
//! itself, and neither does `ui` — an earlier version of this feature did exactly that, and it
//! meant `ui.exe` itself failed to *launch* on any machine without a matching Python installed,
//! not just this one feature. Isolating the Python dependency in a separate helper process,
//! spawned only when this feature is actually used, fixes that: `ui.exe` never touches Python
//! at all. Not part of `request.md`'s phased plan — an ad hoc addition, same footing as the
//! custom title bar.
//!
//! Progress and the final result come back as newline-delimited JSON on `ytbridge`'s stdout
//! (see `ytbridge`'s `Event` enum) rather than scraped human-readable text — sturdier than
//! this module's own first version, which shelled out to the external `yt-dlp` CLI and parsed
//! its `[download] NN.N%` lines directly. Cancellation is just killing the child process; no
//! IPC message needed for it.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mp4Quality {
    P360,
    P480,
    P720,
    P1080,
    Best,
}

impl Mp4Quality {
    pub const ALL: [Mp4Quality; 5] = [
        Mp4Quality::P360,
        Mp4Quality::P480,
        Mp4Quality::P720,
        Mp4Quality::P1080,
        Mp4Quality::Best,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Mp4Quality::P360 => "360p",
            Mp4Quality::P480 => "480p",
            Mp4Quality::P720 => "720p",
            Mp4Quality::P1080 => "1080p",
            Mp4Quality::Best => "Best",
        }
    }

    /// `ytbridge`'s `<target>` argument for this quality, e.g. `"mp4:720"`/`"mp4:best"`.
    fn target_arg(self) -> String {
        match self {
            Mp4Quality::Best => "mp4:best".to_string(),
            Mp4Quality::P360 => "mp4:360".to_string(),
            Mp4Quality::P480 => "mp4:480".to_string(),
            Mp4Quality::P720 => "mp4:720".to_string(),
            Mp4Quality::P1080 => "mp4:1080".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mp3Bitrate {
    K128,
    K192,
    K320,
}

impl Mp3Bitrate {
    pub const ALL: [Mp3Bitrate; 3] = [Mp3Bitrate::K128, Mp3Bitrate::K192, Mp3Bitrate::K320];

    pub fn label(self) -> &'static str {
        match self {
            Mp3Bitrate::K128 => "128 kbps",
            Mp3Bitrate::K192 => "192 kbps",
            Mp3Bitrate::K320 => "320 kbps",
        }
    }

    /// `ytbridge`'s `<target>` argument for this bitrate, e.g. `"mp3:192"`.
    fn target_arg(self) -> String {
        match self {
            Mp3Bitrate::K128 => "mp3:128".to_string(),
            Mp3Bitrate::K192 => "mp3:192".to_string(),
            Mp3Bitrate::K320 => "mp3:320".to_string(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YoutubeDownloadTarget {
    Mp4(Mp4Quality),
    Mp3(Mp3Bitrate),
}

impl YoutubeDownloadTarget {
    fn target_arg(self) -> String {
        match self {
            YoutubeDownloadTarget::Mp4(q) => q.target_arg(),
            YoutubeDownloadTarget::Mp3(b) => b.target_arg(),
        }
    }
}

#[derive(Debug)]
pub enum YoutubeDownloadError {
    /// The `ytbridge` helper binary isn't next to the running executable — not built/shipped
    /// alongside `ui.exe`.
    ToolNotFound,
    /// Failed to spawn or wait on the `ytbridge` process.
    Spawn(std::io::Error),
    /// `ytbridge` exited non-zero, or crashed outright (e.g. its own embedded Python failed to
    /// initialize) — the message is its last reported error, or a generic note if it exited
    /// without ever emitting one (a hard crash, not a clean `Event::Error`). Also used for a
    /// cancelled download (message `"cancelled"`).
    Failed(String),
    /// `ytbridge` exited zero but never emitted a `done` event.
    NoOutputFile,
}

impl std::fmt::Display for YoutubeDownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            YoutubeDownloadError::ToolNotFound => {
                write!(
                    f,
                    "ytbridge helper not found next to the running executable"
                )
            }
            YoutubeDownloadError::Spawn(e) => write!(f, "failed to run ytbridge: {e}"),
            YoutubeDownloadError::Failed(msg) => write!(f, "download failed: {msg}"),
            YoutubeDownloadError::NoOutputFile => {
                write!(f, "ytbridge finished but reported no output file")
            }
        }
    }
}

impl std::error::Error for YoutubeDownloadError {}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Progress { fraction: f32 },
    Done { path: String },
    Error { message: String },
}

/// The `ytbridge` binary's expected path — next to the currently running executable, same
/// directory `cargo build`/an installed layout would put it in. `None` if
/// `std::env::current_exe()` itself fails (should not happen in practice).
fn ytbridge_path() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let name = if cfg!(windows) {
        "ytbridge.exe"
    } else {
        "ytbridge"
    };
    Some(dir.join(name))
}

fn python_home_for_bridge(bridge: &Path) -> Option<PathBuf> {
    let executable_dir = bridge.parent()?;
    if executable_dir
        .file_name()
        .is_some_and(|name| name == "MacOS")
    {
        let contents = executable_dir.parent()?;
        if contents.file_name().is_some_and(|name| name == "Contents") {
            return Some(contents.join("Resources").join("python"));
        }
    }
    Some(executable_dir.to_path_buf())
}

/// Whether the `ytbridge` helper binary is present — checked once up front so the UI can show
/// a clear message instead of a generic spawn failure once a download is already underway.
pub fn is_yt_dlp_available() -> bool {
    ytbridge_path().is_some_and(|p| p.is_file())
}

/// Downloads `url` into `dest_dir` (created if it doesn't exist, by `ytbridge` itself) as
/// `target`'s format/quality, reporting fractional progress (`0.0..=1.0`) as `ytbridge` reports
/// it. Checked against `cancel` between output lines — if set, kills the child process and
/// returns [`YoutubeDownloadError::Failed`] with a `"cancelled"` message.
pub fn download_youtube(
    url: &str,
    target: YoutubeDownloadTarget,
    dest_dir: &Path,
    cancel: &AtomicBool,
    on_progress: impl FnMut(f32),
) -> Result<PathBuf, YoutubeDownloadError> {
    let bridge = ytbridge_path().ok_or(YoutubeDownloadError::ToolNotFound)?;
    download_with_bridge(&bridge, url, target, dest_dir, cancel, on_progress)
}

/// [`download_youtube`] against an explicit `ytbridge` executable.
fn download_with_bridge(
    bridge: &Path,
    url: &str,
    target: YoutubeDownloadTarget,
    dest_dir: &Path,
    cancel: &AtomicBool,
    mut on_progress: impl FnMut(f32),
) -> Result<PathBuf, YoutubeDownloadError> {
    if !bridge.is_file() {
        return Err(YoutubeDownloadError::ToolNotFound);
    }

    let mut cmd = Command::new(bridge);
    cmd.arg(target.target_arg()).arg(url).arg(dest_dir);
    // Points the bundled interpreter at its own vendored stdlib/site-packages next to the
    // binary (Windows: python310.dll, DLLs/, Lib/; Linux: lib/) or the conventional macOS
    // Contents/Resources/python directory — see ytbridge's build.rs — instead of relying on
    // CPython's own DLL-directory auto-detection, which isn't a
    // documented/verified behavior for an embedding host like this (as opposed to running
    // python.exe/python3 itself).
    if let (Some(dir), Some(python_home)) = (bridge.parent(), python_home_for_bridge(bridge)) {
        cmd.env("PYTHONHOME", python_home);
        cmd.env("PYTHONNOUSERSITE", "1");
        cmd.env("PYTHONDONTWRITEBYTECODE", "1");
        cmd.env("DENO_NO_UPDATE_CHECK", "1");
        let mut paths = vec![dir.to_path_buf()];
        if let Some(existing) = std::env::var_os("PATH") {
            paths.extend(std::env::split_paths(&existing));
        }
        if let Ok(path) = std::env::join_paths(paths) {
            // Deno is shipped beside ytbridge. Prepending this directory ensures yt-dlp uses
            // that verified runtime instead of depending on a machine-wide installation.
            cmd.env("PATH", path);
        }
    }
    let mut child: Child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(YoutubeDownloadError::Spawn)?;

    let stdout = child.stdout.take().expect("stdout is piped");
    let mut result: Option<Result<PathBuf, String>> = None;
    let mut cancelled = false;
    for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            let _ = child.kill();
            break;
        }
        let Ok(event) = serde_json::from_str::<Event>(&line) else {
            continue;
        };
        match event {
            Event::Progress { fraction } => on_progress(fraction),
            Event::Done { path } => result = Some(Ok(PathBuf::from(path))),
            Event::Error { message } => result = Some(Err(message)),
        }
    }

    let status = child.wait().map_err(YoutubeDownloadError::Spawn)?;

    if cancelled {
        return Err(YoutubeDownloadError::Failed("cancelled".to_string()));
    }
    match result {
        Some(Ok(path)) => Ok(path),
        Some(Err(message)) => Err(YoutubeDownloadError::Failed(message)),
        None if status.success() => Err(YoutubeDownloadError::NoOutputFile),
        None => Err(YoutubeDownloadError::Failed(format!(
            "ytbridge exited with {status} without reporting a result — it may have crashed \
             (e.g. no compatible Python installation found)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mp4_quality_target_args_encode_height_except_best() {
        assert_eq!(Mp4Quality::P720.target_arg(), "mp4:720");
        assert_eq!(Mp4Quality::Best.target_arg(), "mp4:best");
    }

    #[test]
    fn mp3_bitrate_target_args_encode_kbps() {
        assert_eq!(Mp3Bitrate::K192.target_arg(), "mp3:192");
    }

    #[test]
    fn macos_app_uses_the_private_python_resources_directory() {
        assert_eq!(
            python_home_for_bridge(Path::new("/Applications/Oca.app/Contents/MacOS/ytbridge")),
            Some(PathBuf::from(
                "/Applications/Oca.app/Contents/Resources/python"
            ))
        );
    }

    #[test]
    fn portable_layout_uses_the_helper_directory_as_python_home() {
        assert_eq!(
            python_home_for_bridge(Path::new("/opt/oca/ytbridge")),
            Some(PathBuf::from("/opt/oca"))
        );
    }

    #[test]
    fn every_quality_and_bitrate_has_a_label_and_a_target() {
        let qualities = [
            (Mp4Quality::P360, "360p", "mp4:360"),
            (Mp4Quality::P480, "480p", "mp4:480"),
            (Mp4Quality::P720, "720p", "mp4:720"),
            (Mp4Quality::P1080, "1080p", "mp4:1080"),
            (Mp4Quality::Best, "Best", "mp4:best"),
        ];
        for (quality, label, target) in qualities {
            assert_eq!(quality.label(), label);
            assert_eq!(YoutubeDownloadTarget::Mp4(quality).target_arg(), target);
        }
        let bitrates = [
            (Mp3Bitrate::K128, "128 kbps", "mp3:128"),
            (Mp3Bitrate::K192, "192 kbps", "mp3:192"),
            (Mp3Bitrate::K320, "320 kbps", "mp3:320"),
        ];
        for (bitrate, label, target) in bitrates {
            assert_eq!(bitrate.label(), label);
            assert_eq!(YoutubeDownloadTarget::Mp3(bitrate).target_arg(), target);
        }
    }

    #[test]
    fn every_download_error_has_a_message() {
        assert!(YoutubeDownloadError::ToolNotFound
            .to_string()
            .contains("ytbridge"));
        let spawn = YoutubeDownloadError::Spawn(std::io::Error::other("boom"));
        assert!(spawn.to_string().contains("boom"));
        assert!(YoutubeDownloadError::Failed("why".into())
            .to_string()
            .contains("why"));
        assert!(!YoutubeDownloadError::NoOutputFile.to_string().is_empty());
    }

    #[test]
    fn without_a_helper_next_to_the_executable_nothing_is_available() {
        assert!(!is_yt_dlp_available());
        let cancel = AtomicBool::new(false);
        let result = download_youtube(
            "https://example.invalid/v",
            YoutubeDownloadTarget::Mp3(Mp3Bitrate::K128),
            Path::new("."),
            &cancel,
            |_| {},
        );
        assert!(matches!(result, Err(YoutubeDownloadError::ToolNotFound)));
    }

    /// A stand-in `ytbridge`: a script that prints `body` (one event per line) and exits with
    /// `exit_code`.
    fn fake_bridge(name: &str, body: &[&str], exit_code: i32) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("avcore_fake_ytbridge_{name}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path;
        let mut script = String::new();
        if cfg!(windows) {
            path = dir.join("ytbridge.bat");
            script.push_str("@echo off\r\n");
            for line in body {
                script.push_str(&format!("echo {line}\r\n"));
            }
            script.push_str(&format!("exit /b {exit_code}\r\n"));
        } else {
            path = dir.join("ytbridge");
            script.push_str("#!/bin/sh\n");
            for line in body {
                script.push_str(&format!("echo '{line}'\n"));
            }
            script.push_str(&format!("exit {exit_code}\n"));
        }
        std::fs::write(&path, script).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    fn run(
        bridge: &Path,
        cancel: &AtomicBool,
        progress: &mut Vec<f32>,
    ) -> Result<PathBuf, YoutubeDownloadError> {
        download_with_bridge(
            bridge,
            "https://example.invalid/v",
            YoutubeDownloadTarget::Mp4(Mp4Quality::P720),
            Path::new("."),
            cancel,
            |fraction| progress.push(fraction),
        )
    }

    #[test]
    fn a_bridge_reports_progress_then_the_output_path() {
        let bridge = fake_bridge(
            "ok",
            &[
                r#"{"type":"progress","fraction":0.5}"#,
                "not json at all",
                r#"{"type":"done","path":"out.mp4"}"#,
            ],
            0,
        );
        let mut progress = Vec::new();
        let path = run(&bridge, &AtomicBool::new(false), &mut progress).unwrap();
        assert_eq!(path, PathBuf::from("out.mp4"));
        assert_eq!(progress, vec![0.5]);
    }

    #[test]
    fn bridge_failures_are_reported_by_kind() {
        let cancel = AtomicBool::new(false);
        let mut progress = Vec::new();

        let error = fake_bridge(
            "error",
            &[r#"{"type":"error","message":"private video"}"#],
            0,
        );
        assert!(matches!(
            run(&error, &cancel, &mut progress),
            Err(YoutubeDownloadError::Failed(message)) if message == "private video"
        ));

        let silent = fake_bridge("silent", &[], 0);
        assert!(matches!(
            run(&silent, &cancel, &mut progress),
            Err(YoutubeDownloadError::NoOutputFile)
        ));

        let crashed = fake_bridge("crashed", &[], 3);
        assert!(matches!(
            run(&crashed, &cancel, &mut progress),
            Err(YoutubeDownloadError::Failed(message)) if message.contains("without reporting")
        ));

        // A file that is not an executable cannot be spawned.
        let plain = std::env::temp_dir().join("avcore_fake_ytbridge_plain.txt");
        std::fs::write(&plain, "not a program").unwrap();
        assert!(matches!(
            run(&plain, &cancel, &mut progress),
            Err(YoutubeDownloadError::Spawn(_))
        ));
        assert!(matches!(
            run(Path::new("missing-ytbridge"), &cancel, &mut progress),
            Err(YoutubeDownloadError::ToolNotFound)
        ));
    }

    #[test]
    fn cancelling_stops_the_download() {
        let bridge = fake_bridge(
            "cancel",
            &[
                r#"{"type":"progress","fraction":0.1}"#,
                r#"{"type":"progress","fraction":0.2}"#,
            ],
            0,
        );
        let mut progress = Vec::new();
        let result = run(&bridge, &AtomicBool::new(true), &mut progress);
        assert!(matches!(
            result,
            Err(YoutubeDownloadError::Failed(message)) if message == "cancelled"
        ));
        assert!(progress.is_empty());
    }
}
