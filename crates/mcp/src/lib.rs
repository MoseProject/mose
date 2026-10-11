// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

//! A Model Context Protocol server over stdio (newline-delimited JSON-RPC 2.0) that exposes the
//! app's control channel as three tools: `list_commands`, `run_command` and `get_session`.
//! It adds nothing the control channel does not already allow, so an agent can do exactly what
//! the keyboard and palette can, and no more.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, TcpStream};
use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};

/// Protocol versions this server can speak, newest first.
const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];
/// How long one control request may take, matching the window's own limit plus a margin.
const CONTROL_TIMEOUT: Duration = Duration::from_secs(70);

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Where requests for the app go.
pub trait Control {
    fn request(&self, method: &str, params: Value) -> Result<Value, String>;
}

/// The app's control channel over loopback TCP, one connection per request.
pub struct TcpControl {
    port: u16,
    token: String,
}

impl TcpControl {
    /// Reads the port and token the app wrote to `control.json`.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        let info: Value =
            serde_json::from_str(&text).map_err(|error| format!("bad control file: {error}"))?;
        let port = info
            .get("port")
            .and_then(Value::as_u64)
            .and_then(|port| u16::try_from(port).ok())
            .ok_or("the control file has no port")?;
        let token = info
            .get("token")
            .and_then(Value::as_str)
            .ok_or("the control file has no token")?
            .to_owned();
        Ok(Self { port, token })
    }
}

impl Control for TcpControl {
    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, self.port))
            .map_err(|error| format!("the app is not listening: {error}"))?;
        stream
            .set_read_timeout(Some(CONTROL_TIMEOUT))
            .map_err(|error| error.to_string())?;
        let mut writer = stream.try_clone().map_err(|error| error.to_string())?;
        let request = json!({ "id": 1, "token": self.token, "method": method, "params": params });
        writeln!(writer, "{request}").map_err(|error| error.to_string())?;
        let mut line = String::new();
        BufReader::new(stream)
            .read_line(&mut line)
            .map_err(|error| format!("no answer from the app: {error}"))?;
        let response: Value =
            serde_json::from_str(&line).map_err(|error| format!("bad answer: {error}"))?;
        match response.get("error") {
            Some(error) => Err(error
                .as_str()
                .map_or_else(|| error.to_string(), str::to_owned)),
            None => Ok(response.get("result").cloned().unwrap_or(Value::Null)),
        }
    }
}

fn error_response(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn tool_list() -> Value {
    json!([
        {
            "name": "list_commands",
            "description": "List every editor command: id, label, group, key bindings and whether it can run right now.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "run_command",
            "description": "Run one editor command by id, as a key press would. Refused when the command is disabled.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": { "type": "string", "description": "A command id from list_commands" } },
                "required": ["id"]
            }
        },
        {
            "name": "get_session",
            "description": "The open project and editor state: sequences, tracks, clips, playhead, undo state.",
            "inputSchema": { "type": "object", "properties": {} }
        }
    ])
}

fn tool_result(outcome: Result<Value, String>) -> Value {
    match outcome {
        Ok(value) => json!({
            "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
            "isError": false
        }),
        Err(message) => json!({
            "content": [{ "type": "text", "text": message }],
            "isError": true
        }),
    }
}

fn call_tool(control: &dyn Control, params: &Value) -> Result<Value, (i64, String)> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or((INVALID_PARAMS, "missing tool name".to_owned()))?;
    let arguments = params.get("arguments").cloned().unwrap_or(Value::Null);
    let outcome = match name {
        "list_commands" => control.request("commands.list", Value::Null),
        "get_session" => control.request("session.get", Value::Null),
        "run_command" => match arguments.get("id").and_then(Value::as_str) {
            Some(id) => control.request("commands.run", json!({ "id": id })),
            None => Err("run_command needs a string `id`".to_owned()),
        },
        other => return Err((INVALID_PARAMS, format!("unknown tool: {other}"))),
    };
    Ok(tool_result(outcome))
}

/// Answers one message line; `None` for notifications, which get no response.
pub fn handle_message(control: &dyn Control, line: &str) -> Option<String> {
    let message: Value = match serde_json::from_str(line) {
        Ok(message) => message,
        Err(_) => return Some(error_response(Value::Null, PARSE_ERROR, "parse error").to_string()),
    };
    let Some(method) = message.get("method").and_then(Value::as_str) else {
        // A response or something else: nothing to answer.
        return message
            .get("id")
            .map(|id| error_response(id.clone(), INVALID_REQUEST, "invalid request").to_string());
    };
    let id = message.get("id").cloned()?;
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let response = match method {
        "initialize" => {
            let asked = params.get("protocolVersion").and_then(Value::as_str);
            let version = asked
                .filter(|asked| PROTOCOL_VERSIONS.contains(asked))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            json!({ "jsonrpc": "2.0", "id": id, "result": {
                "protocolVersion": version,
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "oca", "version": env!("CARGO_PKG_VERSION") }
            }})
        }
        "ping" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
        "tools/list" => json!({ "jsonrpc": "2.0", "id": id, "result": { "tools": tool_list() } }),
        "tools/call" => match call_tool(control, &params) {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err((code, message)) => error_response(id, code, &message),
        },
        _ => error_response(id, METHOD_NOT_FOUND, "method not found"),
    };
    Some(response.to_string())
}

/// Serves messages from `input` until it ends, writing responses to `output`.
pub fn serve(
    control: &dyn Control,
    input: impl BufRead,
    mut output: impl Write,
) -> std::io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Some(response) = handle_message(control, &line) {
            writeln!(output, "{response}")?;
            output.flush()?;
        }
    }
    Ok(())
}
