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

//! `ytbridge` — a small standalone binary that downloads a YouTube video as MP4 or MP3 by
//! calling yt-dlp's own Python library API (`yt_dlp.YoutubeDL`) directly, via an embedded
//! Python interpreter (`pyo3`, `auto-initialize`). Deliberately its own executable rather than
//! a module inside `core`/`ui`.
//!
//! **Why a separate process, not a library call from `core`:** an earlier version of this
//! feature linked `pyo3` straight into `core` (and transitively `ui.exe`). That dynamically
//! links against `libpython`/`python3*.dll` at **build time**, and needs that same (or
//! ABI-compatible) Python runtime present at **launch time** — on a machine with no matching
//! Python installed, `ui.exe` itself failed to start (a missing shared-library load), not just
//! this one feature. Spawning this as a subprocess instead — only when the user actually clicks
//! "Baixar do YouTube" — means `ui.exe` never touches Python at all; only this small helper
//! does, and only while it's running. `avcore::youtube_download` spawns it, reads its stdout,
//! and kills it to cancel — the exact "external tool the app shells out to" shape FFmpeg
//! already has elsewhere in this codebase, except this external tool is one oca
//! builds and ships itself (`target/.../ytbridge(.exe)`, found next to the running `ui.exe`)
//! rather than something the user installs separately.
//!
//! **Protocol:** three positional args, `<target> <url> <dest_dir>`, where `<target>` is
//! `mp4:360`/`mp4:480`/`mp4:720`/`mp4:1080`/`mp4:best`/`mp3:128`/`mp3:192`/`mp3:320` — see
//! [`Args::parse`]. Progress and the result are newline-delimited JSON on stdout — see
//! [`Event`] — so the parent process never has to scrape yt-dlp's own human-readable progress
//! text. Cancellation has no protocol message of its own: the parent just kills this process.
//!
//! **Verification caveat:** the `pyo3`/`yt_dlp` API usage (`Python::attach`,
//! `PyCFunction::new_closure`, `PyDictMethods::set_item`, `PyAnyMethods::call`/`call_method`,
//! `progress_hooks`' dict shape) was checked against pyo3 0.29.2's real published docs and
//! yt-dlp's own `YoutubeDL.py` source rather than guessed, and the crate builds and links
//! cleanly on this dev machine (confirming a compatible Python was discoverable at build time
//! here) — but has not been exercised against a real download, no Python environment with
//! `yt_dlp` installed was available in this sandbox to actually run it.

use std::io::Write;
use std::path::PathBuf;

use pyo3::types::{PyAnyMethods, PyCFunction, PyDict, PyDictMethods, PyTuple, PyTupleMethods};
use pyo3::{Bound, Python};
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Event {
    Progress { fraction: f32 },
    Done { path: String },
    Error { message: String },
}

fn emit(event: &Event) {
    if let Ok(line) = serde_json::to_string(event) {
        println!("{line}");
        let _ = std::io::stdout().flush();
    }
}

struct Args {
    target_kind: &'static str,
    height: Option<u32>,
    bitrate_kbps: Option<u32>,
    url: String,
    dest_dir: PathBuf,
}

impl Args {
    /// Parses `<target> <url> <dest_dir>` — `<target>` is `mp4:<360|480|720|1080|best>` or
    /// `mp3:<128|192|320>`. Deliberately terse/positional (no flag parsing dependency) since
    /// this binary has exactly one caller ([`avcore::youtube_download::download_youtube`]),
    /// not a general-purpose CLI.
    fn parse() -> Result<Self, String> {
        let raw: Vec<String> = std::env::args().skip(1).collect();
        let [target, url, dest_dir] = raw.as_slice() else {
            return Err("usage: ytbridge <mp4:HEIGHT|mp4:best|mp3:KBPS> <url> <dest_dir>".into());
        };
        let (kind, value) = target.split_once(':').ok_or("target must be KIND:VALUE")?;
        let (target_kind, height, bitrate_kbps): (&'static str, Option<u32>, Option<u32>) =
            match kind {
                "mp4" if value == "best" => ("mp4", None, None),
                "mp4" => (
                    "mp4",
                    Some(value.parse().map_err(|_| "bad mp4 height")?),
                    None,
                ),
                "mp3" => (
                    "mp3",
                    None,
                    Some(value.parse().map_err(|_| "bad mp3 bitrate")?),
                ),
                _ => return Err("target kind must be mp4 or mp3".into()),
            };
        Ok(Args {
            target_kind,
            height,
            bitrate_kbps,
            url: url.clone(),
            dest_dir: PathBuf::from(dest_dir),
        })
    }
}

fn main() {
    let args = match Args::parse() {
        Ok(a) => a,
        Err(message) => {
            emit(&Event::Error { message });
            std::process::exit(2);
        }
    };

    if let Err(e) = std::fs::create_dir_all(&args.dest_dir) {
        emit(&Event::Error {
            message: format!("failed to create destination directory: {e}"),
        });
        std::process::exit(1);
    }

    match run(&args) {
        Ok(path) => {
            emit(&Event::Done {
                path: path.display().to_string(),
            });
        }
        Err(message) => {
            emit(&Event::Error { message });
            std::process::exit(1);
        }
    }
}

fn run(args: &Args) -> Result<PathBuf, String> {
    let outtmpl = args
        .dest_dir
        .join("%(title).200B [%(id)s].%(ext)s")
        .display()
        .to_string();

    Python::attach(|py| {
        let yt_dlp = py
            .import("yt_dlp")
            .map_err(|_| "yt_dlp Python package not importable — pip install yt-dlp".to_string())?;

        let opts = PyDict::new(py);
        opts.set_item("outtmpl", &outtmpl)
            .and_then(|_| opts.set_item("noplaylist", true))
            .and_then(|_| opts.set_item("quiet", true))
            .and_then(|_| opts.set_item("no_warnings", true))
            .map_err(|e| e.to_string())?;

        match args.target_kind {
            "mp4" => {
                let selector = match args.height {
                    Some(h) => format!("bestvideo[height<={h}]+bestaudio/best[height<={h}]"),
                    None => "bestvideo+bestaudio/best".to_string(),
                };
                opts.set_item("format", selector)
                    .and_then(|_| opts.set_item("merge_output_format", "mp4"))
                    .map_err(|e| e.to_string())?;
            }
            "mp3" => {
                let pp = PyDict::new(py);
                pp.set_item("key", "FFmpegExtractAudio")
                    .and_then(|_| pp.set_item("preferredcodec", "mp3"))
                    .and_then(|_| {
                        pp.set_item(
                            "preferredquality",
                            args.bitrate_kbps.unwrap_or(192).to_string(),
                        )
                    })
                    .map_err(|e| e.to_string())?;
                let pps = PyTuple::new(py, [pp]).map_err(|e| e.to_string())?;
                opts.set_item("postprocessors", pps)
                    .map_err(|e| e.to_string())?;
            }
            _ => unreachable!("Args::parse only produces mp4/mp3"),
        }

        let hook = PyCFunction::new_closure(
            py,
            None,
            None,
            |args: &Bound<'_, PyTuple>,
             _kwargs: Option<&Bound<'_, PyDict>>|
             -> pyo3::PyResult<()> {
                let Ok(d) = args.get_item(0) else {
                    return Ok(());
                };
                let status: String = d
                    .get_item("status")
                    .and_then(|v| v.extract())
                    .unwrap_or_default();
                let fraction = if status == "finished" {
                    Some(1.0)
                } else if status == "downloading" {
                    let downloaded: f64 = d
                        .get_item("downloaded_bytes")
                        .and_then(|v| v.extract())
                        .unwrap_or(0.0);
                    let total: f64 = d
                        .get_item("total_bytes")
                        .and_then(|v| v.extract())
                        .or_else(|_| d.get_item("total_bytes_estimate").and_then(|v| v.extract()))
                        .unwrap_or(0.0);
                    (total > 0.0).then(|| (downloaded / total).clamp(0.0, 1.0) as f32)
                } else {
                    None
                };
                if let Some(fraction) = fraction {
                    emit(&Event::Progress { fraction });
                }
                Ok(())
            },
        )
        .map_err(|e| e.to_string())?;
        let hooks = PyTuple::new(py, [hook]).map_err(|e| e.to_string())?;
        opts.set_item("progress_hooks", hooks)
            .map_err(|e| e.to_string())?;

        let ydl_cls = yt_dlp.getattr("YoutubeDL").map_err(|e| e.to_string())?;
        let ydl = ydl_cls.call1((opts,)).map_err(|e| e.to_string())?;

        let kwargs = PyDict::new(py);
        kwargs
            .set_item("download", true)
            .map_err(|e| e.to_string())?;
        let info = ydl
            .call_method("extract_info", (&args.url,), Some(&kwargs))
            .map_err(|e| e.to_string())?;

        extract_final_path(&info)
            .ok_or_else(|| "yt-dlp finished but reported no output file".to_string())
    })
}

/// Reads `info['requested_downloads'][0]['filepath']` — the same field yt-dlp's own CLI
/// `--print after_move:filepath` template resolves to, set only after any post-processing
/// (merge, audio extraction) has produced the truly final file.
fn extract_final_path(info: &Bound<'_, pyo3::PyAny>) -> Option<PathBuf> {
    let downloads = info.get_item("requested_downloads").ok()?;
    let first = downloads.get_item(0).ok()?;
    let filepath: String = first.get_item("filepath").ok()?.extract().ok()?;
    Some(PathBuf::from(filepath))
}
