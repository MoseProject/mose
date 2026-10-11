// Copyright (C) 2026 by Lucas Gomes <lucasgsm88@gmail.com>
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::Mutex;

use oca_mcp::{handle_message, serve, Control, TcpControl};
use serde_json::{json, Value};

/// Records what it was asked and answers from a script.
struct Fake {
    calls: Mutex<Vec<(String, Value)>>,
    answer: Result<Value, String>,
}

impl Fake {
    fn answering(answer: Result<Value, String>) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            answer,
        }
    }
}

impl Control for Fake {
    fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push((method.to_owned(), params));
        self.answer.clone()
    }
}

fn reply(control: &Fake, message: Value) -> Value {
    let line = handle_message(control, &message.to_string()).expect("a response");
    serde_json::from_str(&line).unwrap()
}

#[test]
fn initialize_agrees_on_a_known_protocol_version() {
    let control = Fake::answering(Ok(Value::Null));
    let response = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2024-11-05" } }),
    );
    assert_eq!(response["result"]["protocolVersion"], "2024-11-05");
    assert_eq!(response["result"]["serverInfo"]["name"], "oca");
    assert!(response["result"]["capabilities"]["tools"].is_object());

    let newer = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 2, "method": "initialize",
                "params": { "protocolVersion": "2099-01-01" } }),
    );
    assert_eq!(newer["result"]["protocolVersion"], "2025-06-18");
}

#[test]
fn notifications_get_no_response() {
    let control = Fake::answering(Ok(Value::Null));
    let message = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    assert!(handle_message(&control, &message.to_string()).is_none());
}

#[test]
fn the_three_tools_are_listed_and_nothing_else() {
    let control = Fake::answering(Ok(Value::Null));
    let response = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/list" }),
    );
    let names: Vec<&str> = response["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["list_commands", "run_command", "get_session"]);
    assert!(control.calls.lock().unwrap().is_empty());
}

#[test]
fn tools_map_onto_the_control_channel_methods() {
    let control = Fake::answering(Ok(json!([{ "id": "split" }])));
    let call = |name: &str, arguments: Value| {
        reply(
            &control,
            json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                    "params": { "name": name, "arguments": arguments } }),
        )
    };
    let listed = call("list_commands", json!({}));
    assert_eq!(listed["result"]["isError"], false);
    assert!(listed["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("split"));
    call("run_command", json!({ "id": "split" }));
    call("get_session", json!({}));
    let calls = control.calls.lock().unwrap();
    assert_eq!(calls[0], ("commands.list".to_owned(), Value::Null));
    assert_eq!(
        calls[1],
        ("commands.run".to_owned(), json!({ "id": "split" }))
    );
    assert_eq!(calls[2], ("session.get".to_owned(), Value::Null));
}

#[test]
fn a_failing_command_is_a_tool_error_not_a_protocol_error() {
    let control = Fake::answering(Err("command is disabled: split".to_owned()));
    let response = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 5, "method": "tools/call",
                "params": { "name": "run_command", "arguments": { "id": "split" } } }),
    );
    assert!(response.get("error").is_none());
    assert_eq!(response["result"]["isError"], true);
    assert_eq!(
        response["result"]["content"][0]["text"],
        "command is disabled: split"
    );
}

#[test]
fn bad_calls_are_refused_before_reaching_the_app() {
    let control = Fake::answering(Ok(Value::Null));
    let missing_id = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 6, "method": "tools/call",
                "params": { "name": "run_command", "arguments": {} } }),
    );
    assert_eq!(missing_id["result"]["isError"], true);
    let unknown = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 7, "method": "tools/call",
                "params": { "name": "start_export", "arguments": {} } }),
    );
    assert_eq!(unknown["error"]["code"], -32602);
    assert!(control.calls.lock().unwrap().is_empty());
}

#[test]
fn unknown_methods_and_garbage_are_answered() {
    let control = Fake::answering(Ok(Value::Null));
    let unknown = reply(
        &control,
        json!({ "jsonrpc": "2.0", "id": 8, "method": "resources/list" }),
    );
    assert_eq!(unknown["error"]["code"], -32601);
    let garbage: Value =
        serde_json::from_str(&handle_message(&control, "{not json").unwrap()).unwrap();
    assert_eq!(garbage["error"]["code"], -32700);
}

#[test]
fn serve_answers_each_line_in_order_and_skips_blank_ones() {
    let control = Fake::answering(Ok(Value::Null));
    let input = [
        json!({ "jsonrpc": "2.0", "id": 1, "method": "ping" }).to_string(),
        String::new(),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }).to_string(),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "ping" }).to_string(),
    ]
    .join("\n");
    let mut output = Vec::new();
    serve(&control, input.as_bytes(), &mut output).unwrap();
    let ids: Vec<Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap()["id"].clone())
        .collect();
    assert_eq!(ids, [json!(1), json!(2)]);
}

#[test]
fn the_control_file_gives_the_port_and_token() {
    let dir = tempfile_dir("file");
    let path = dir.join("control.json");
    std::fs::write(&path, r#"{"port":4242,"token":"abc"}"#).unwrap();
    assert!(TcpControl::from_file(&path).is_ok());
    std::fs::write(&path, r#"{"port":4242}"#).unwrap();
    assert!(TcpControl::from_file(&path).is_err());
    std::fs::write(&path, "nonsense").unwrap();
    assert!(TcpControl::from_file(&path).is_err());
    assert!(TcpControl::from_file(&dir.join("missing.json")).is_err());
}

#[test]
fn requests_reach_a_control_channel_with_the_token_and_return_its_answer() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let request: Value = serde_json::from_str(&line).unwrap();
        let mut writer = stream;
        let response = if request["token"] == "secret" && request["method"] == "commands.run" {
            json!({ "id": request["id"], "result": { "ran": request["params"]["id"] } })
        } else {
            json!({ "id": request["id"], "error": "unauthorized" })
        };
        writeln!(writer, "{response}").unwrap();
    });
    let dir = tempfile_dir("tcp");
    let path = dir.join("control.json");
    std::fs::write(
        &path,
        json!({ "port": port, "token": "secret" }).to_string(),
    )
    .unwrap();
    let control = TcpControl::from_file(&path).unwrap();
    let result = control.request("commands.run", json!({ "id": "split" }));
    assert_eq!(result, Ok(json!({ "ran": "split" })));
    server.join().unwrap();
}

#[test]
fn an_error_from_the_app_comes_back_as_text() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        let mut writer = stream;
        writeln!(writer, r#"{{"id":1,"error":"unauthorized"}}"#).unwrap();
    });
    let dir = tempfile_dir("err");
    let path = dir.join("control.json");
    std::fs::write(&path, json!({ "port": port, "token": "wrong" }).to_string()).unwrap();
    let control = TcpControl::from_file(&path).unwrap();
    assert_eq!(
        control.request("commands.list", Value::Null),
        Err("unauthorized".to_owned())
    );
    server.join().unwrap();
}

#[test]
fn an_app_that_is_not_running_is_reported() {
    let free = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = free.local_addr().unwrap().port();
    drop(free);
    let dir = tempfile_dir("down");
    let path = dir.join("control.json");
    std::fs::write(&path, json!({ "port": port, "token": "x" }).to_string()).unwrap();
    let control = TcpControl::from_file(&path).unwrap();
    let error = control.request("commands.list", Value::Null).unwrap_err();
    assert!(error.contains("not listening"), "{error}");
}

fn tempfile_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("oca-mcp-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
