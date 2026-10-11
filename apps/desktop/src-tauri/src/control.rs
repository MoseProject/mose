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

//! The control channel: an opt-in way to drive the running app from outside the window, for
//! automation, tests and agents. JSON lines over loopback TCP, one request per line:
//!
//! ```text
//! {"id":1,"token":"<from control.json>","method":"commands.run","params":{"id":"split"}}
//! {"id":1,"result":null}                      or      {"id":1,"error":"..."}
//! ```
//!
//! It is off unless `OCA_CONTROL_PORT` (or `--control <port>`) is set; port 0 picks a free one.
//! The port and a random token are written to `control.json` in the app config directory, and
//! every request must carry the token, so another local process cannot drive the app by
//! guessing the port. Only three methods exist: `session.get`, answered here, and
//! `commands.list` / `commands.run`, forwarded to the webview (see `src/control.ts`), which owns
//! the command table and answers through [`control_reply`]. Nothing lets a client call an
//! arbitrary backend command. At most [`MAX_CONNECTIONS`] clients are served at once, and one
//! that stays silent for [`IDLE_TIMEOUT`] is dropped.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::state::{lock_or_recover, AppState};

/// Event telling the webview to answer a forwarded request.
pub const CONTROL_REQUEST_EVENT: &str = "control-request";
/// The longest the webview gets to answer one request.
const REPLY_TIMEOUT: Duration = Duration::from_secs(60);
/// The longest accepted request line.
const MAX_LINE_BYTES: u64 = 1 << 20;
const TOKEN_BYTES: usize = 16;
/// Methods the webview answers.
const FORWARDED_METHODS: [&str; 2] = ["commands.list", "commands.run"];
/// Clients served at the same time.
pub const MAX_CONNECTIONS: usize = 8;
/// How long a connection may stay silent before it is closed.
const IDLE_TIMEOUT: Duration = Duration::from_secs(300);

/// What answers a request once it is authenticated and parsed.
pub trait Backend: Send + Sync {
    fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

/// Matches forwarded requests to the webview's replies.
#[derive(Default)]
pub struct ControlHub {
    /// Set once the window is listening for forwarded requests. An event sent before that is
    /// lost, so until then a forwarded request is refused at once instead of waiting out the
    /// reply timeout.
    window_ready: AtomicBool,
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, Sender<Result<Value, String>>>>,
}

impl ControlHub {
    pub fn set_window_ready(&self) {
        self.window_ready.store(true, Ordering::SeqCst);
    }

    fn register(&self) -> (u64, Receiver<Result<Value, String>>) {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) + 1;
        let (sender, receiver) = mpsc::channel();
        lock_or_recover(&self.pending).insert(id, sender);
        (id, receiver)
    }

    fn cancel(&self, id: u64) {
        lock_or_recover(&self.pending).remove(&id);
    }

    /// Delivers the webview's answer; false when nobody waits for it any more.
    pub fn reply(&self, id: u64, result: Result<Value, String>) -> bool {
        match lock_or_recover(&self.pending).remove(&id) {
            Some(sender) => sender.send(result).is_ok(),
            None => false,
        }
    }
}

/// The real backend: `session.get` natively, the rest through the webview.
struct AppBackend<R: Runtime> {
    app: AppHandle<R>,
}

impl<R: Runtime> Backend for AppBackend<R> {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        let state = self.app.state::<AppState>();
        if method == "session.get" {
            let session = state.session();
            let rev = lock_or_recover(&state.sync).rev;
            return Ok(crate::session_sync::snapshot(&session.view_at(rev)));
        }
        if !FORWARDED_METHODS.contains(&method) {
            return Err(format!("unknown method: {method}"));
        }
        if !state.control.window_ready.load(Ordering::SeqCst) {
            return Err("the window is not ready yet".to_string());
        }
        let (id, receiver) = state.control.register();
        let request = json!({ "requestId": id, "method": method, "params": params });
        if let Err(error) = self.app.emit(CONTROL_REQUEST_EVENT, request) {
            state.control.cancel(id);
            return Err(format!("the window could not be reached: {error}"));
        }
        match receiver.recv_timeout(REPLY_TIMEOUT) {
            Ok(result) => result,
            Err(_) => {
                state.control.cancel(id);
                Err("the window did not answer in time".to_string())
            }
        }
    }
}

/// The window has started listening for forwarded requests.
#[tauri::command]
pub fn control_ready(state: State<'_, AppState>) {
    state.control.set_window_ready();
}

/// The webview's answer to a forwarded request.
#[tauri::command]
pub fn control_reply(
    state: State<'_, AppState>,
    request_id: u64,
    ok: bool,
    value: Value,
) -> Result<(), String> {
    let result = if ok {
        Ok(value)
    } else {
        Err(value
            .as_str()
            .map_or_else(|| value.to_string(), str::to_owned))
    };
    state.control.reply(request_id, result);
    Ok(())
}

/// Compares without stopping at the first difference.
fn same_token(given: &str, expected: &str) -> bool {
    given.len() == expected.len()
        && given
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |diff, (a, b)| diff | (a ^ b))
            == 0
}

/// Answers one request line with one response line (no trailing newline).
pub fn handle_line(line: &str, token: &str, backend: &dyn Backend) -> String {
    let request: Value = match serde_json::from_str(line) {
        Ok(request) => request,
        Err(error) => {
            return json!({ "id": null, "error": format!("not valid JSON: {error}") }).to_string()
        }
    };
    let id = request.get("id").cloned().unwrap_or(Value::Null);
    let authorized = request
        .get("token")
        .and_then(Value::as_str)
        .is_some_and(|given| same_token(given, token));
    if !authorized {
        return json!({ "id": id, "error": "unauthorized" }).to_string();
    }
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return json!({ "id": id, "error": "missing method" }).to_string();
    };
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    match backend.call(method, params) {
        Ok(result) => json!({ "id": id, "result": result }).to_string(),
        Err(error) => json!({ "id": id, "error": error }).to_string(),
    }
}

/// Reads one line of at most [`MAX_LINE_BYTES`]; `None` at the end of the stream or when the
/// line is longer than that.
fn read_request_line(reader: &mut BufReader<TcpStream>) -> Option<String> {
    let mut line = String::new();
    let read = reader
        .by_ref()
        .take(MAX_LINE_BYTES)
        .read_line(&mut line)
        .ok()?;
    (read > 0 && line.ends_with('\n')).then(|| line.trim_end().to_owned())
}

fn serve_connection(stream: TcpStream, token: &str, backend: &dyn Backend) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let _ = stream.set_read_timeout(Some(IDLE_TIMEOUT));
    let mut reader = BufReader::new(stream);
    while let Some(line) = read_request_line(&mut reader) {
        if line.is_empty() {
            continue;
        }
        let mut response = handle_line(&line, token, backend);
        response.push('\n');
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

/// Counts a served connection and gives its slot back when it ends.
struct Slot(Arc<AtomicUsize>);

impl Slot {
    fn take(active: &Arc<AtomicUsize>, limit: usize) -> Option<Self> {
        let taken = active.fetch_add(1, Ordering::SeqCst);
        if taken >= limit {
            active.fetch_sub(1, Ordering::SeqCst);
            return None;
        }
        Some(Self(Arc::clone(active)))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Accepts connections on `listener` until the process ends, one thread each, at most
/// `max_connections` at a time; a client over the limit is told so and closed.
pub fn serve(
    listener: TcpListener,
    token: String,
    backend: Arc<dyn Backend>,
    max_connections: usize,
) {
    let active = Arc::new(AtomicUsize::new(0));
    std::thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let Some(slot) = Slot::take(&active, max_connections) else {
                let _ = stream.write_all(b"{\"id\":null,\"error\":\"too many connections\"}\n");
                continue;
            };
            let token = token.clone();
            let backend = Arc::clone(&backend);
            std::thread::spawn(move || {
                let _slot = slot;
                serve_connection(stream, &token, backend.as_ref());
            });
        }
    });
}

/// The port asked for through `OCA_CONTROL_PORT` or `--control <port>`; `None` keeps the
/// channel off.
pub fn requested_port(env: Option<String>, args: &[String]) -> Option<u16> {
    let from_args = args
        .iter()
        .position(|arg| arg == "--control")
        .and_then(|at| args.get(at + 1))
        .cloned();
    from_args.or(env)?.trim().parse().ok()
}

/// A fresh random token, as hex.
fn new_token() -> io::Result<String> {
    let mut bytes = [0u8; TOKEN_BYTES];
    getrandom::fill(&mut bytes).map_err(|error| io::Error::other(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

/// Binds the loopback listener and records where it is in `<config_dir>/control.json`.
pub fn bind(config_dir: &Path, port: u16) -> io::Result<(TcpListener, String)> {
    let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
    let token = new_token()?;
    let info = json!({ "port": listener.local_addr()?.port(), "token": token });
    std::fs::create_dir_all(config_dir)?;
    write_private(
        &config_dir.join("control.json"),
        info.to_string().as_bytes(),
    )?;
    Ok((listener, token))
}

/// Writes `bytes` readable by the current user only where the platform supports it.
fn write_private(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// Starts the channel on `port` for the running app.
pub fn start<R: Runtime>(app: &AppHandle<R>, config_dir: &Path, port: u16) -> io::Result<u16> {
    let (listener, token) = bind(config_dir, port)?;
    let bound = listener.local_addr()?.port();
    serve(
        listener,
        token,
        Arc::new(AppBackend { app: app.clone() }),
        MAX_CONNECTIONS,
    );
    tracing::info!(port = bound, "control channel listening on loopback");
    Ok(bound)
}

#[cfg(test)]
#[path = "control_test.rs"]
mod tests;
