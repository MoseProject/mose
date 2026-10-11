// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! `oca-mcp --control-file <path>` (or `OCA_CONTROL_FILE`): an MCP server on stdio for the
//! running app. Start the app with `OCA_CONTROL_PORT=0` first; it writes `control.json` to its
//! config directory (`%APPDATA%\com.pacopacoca.oca` on Windows, `~/.config/com.pacopacoca.oca`
//! on Linux, `~/Library/Application Support/com.pacopacoca.oca` on macOS).

use std::io::{stdin, stdout, BufReader};
use std::path::PathBuf;
use std::process::ExitCode;

use oca_mcp::{serve, TcpControl};

fn control_file() -> Option<PathBuf> {
    let args: Vec<String> = std::env::args().collect();
    let from_args = args
        .iter()
        .position(|arg| arg == "--control-file")
        .and_then(|at| args.get(at + 1))
        .map(PathBuf::from);
    from_args.or_else(|| std::env::var_os("OCA_CONTROL_FILE").map(PathBuf::from))
}

fn main() -> ExitCode {
    let Some(path) = control_file() else {
        eprintln!("usage: oca-mcp --control-file <control.json>   (or set OCA_CONTROL_FILE)");
        return ExitCode::from(2);
    };
    let control = match TcpControl::from_file(&path) {
        Ok(control) => control,
        Err(message) => {
            eprintln!("oca-mcp: {message}");
            return ExitCode::FAILURE;
        }
    };
    match serve(&control, BufReader::new(stdin().lock()), stdout().lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("oca-mcp: {error}");
            ExitCode::FAILURE
        }
    }
}
