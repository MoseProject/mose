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

use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::sync::Mutex;

use tauri::Listener;

use super::*;
use crate::testing::harness;

/// Answers `echo` with its parameters and everything else with an error.
struct Echo {
    calls: Mutex<Vec<String>>,
}

impl Backend for Echo {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push(method.to_owned());
        match method {
            "echo" => Ok(params),
            other => Err(format!("unknown method {other}")),
        }
    }
}

fn echo() -> Echo {
    Echo {
        calls: Mutex::new(Vec::new()),
    }
}

fn reply_to(line: &str, backend: &Echo) -> Value {
    serde_json::from_str(&handle_line(line, "secret", backend)).unwrap()
}

#[test]
fn a_request_with_the_token_gets_the_result() {
    let backend = echo();
    let response = reply_to(
        r#"{"id":7,"token":"secret","method":"echo","params":{"a":1}}"#,
        &backend,
    );
    assert_eq!(response, json!({ "id": 7, "result": { "a": 1 } }));
}

#[test]
fn a_failing_method_reports_its_error() {
    let backend = echo();
    let response = reply_to(r#"{"id":1,"token":"secret","method":"nope"}"#, &backend);
    assert_eq!(response, json!({ "id": 1, "error": "unknown method nope" }));
}

#[test]
fn a_request_without_the_right_token_never_reaches_the_backend() {
    let backend = echo();
    for line in [
        r#"{"id":1,"method":"echo"}"#,
        r#"{"id":1,"token":"wrong","method":"echo"}"#,
        r#"{"id":1,"token":"secre","method":"echo"}"#,
        r#"{"id":1,"token":7,"method":"echo"}"#,
    ] {
        assert_eq!(
            reply_to(line, &backend),
            json!({ "id": 1, "error": "unauthorized" }),
            "{line}"
        );
    }
    assert!(backend.calls.lock().unwrap().is_empty());
}

#[test]
fn malformed_requests_are_answered_not_dropped() {
    let backend = echo();
    let response = reply_to("not json", &backend);
    assert_eq!(response["id"], Value::Null);
    assert!(response["error"]
        .as_str()
        .unwrap()
        .contains("not valid JSON"));
    assert_eq!(
        reply_to(r#"{"id":2,"token":"secret"}"#, &backend),
        json!({ "id": 2, "error": "missing method" })
    );
}

#[test]
fn the_channel_is_only_on_when_a_port_is_asked_for() {
    let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    assert_eq!(requested_port(None, &args(&["oca"])), None);
    assert_eq!(
        requested_port(Some("9876".into()), &args(&["oca"])),
        Some(9876)
    );
    assert_eq!(
        requested_port(Some("1".into()), &args(&["oca", "--control", "0"])),
        Some(0),
        "the flag wins over the environment"
    );
    assert_eq!(requested_port(Some("abc".into()), &args(&["oca"])), None);
    assert_eq!(requested_port(None, &args(&["oca", "--control"])), None);
}

#[test]
fn binding_records_the_port_and_a_fresh_token() {
    let h = harness();
    let (listener, token) = bind(&h.dir, 0).unwrap();
    let info: Value =
        serde_json::from_slice(&std::fs::read(h.dir.join("control.json")).unwrap()).unwrap();
    assert_eq!(info["port"], listener.local_addr().unwrap().port());
    assert_eq!(info["token"], token.as_str());
    assert_eq!(token.len(), TOKEN_BYTES * 2);
    let (_, other) = bind(&h.dir, 0).unwrap();
    assert_ne!(token, other);
}

#[test]
fn requests_travel_over_a_real_loopback_connection() {
    let h = harness();
    let (listener, token) = bind(&h.dir, 0).unwrap();
    let port = listener.local_addr().unwrap().port();
    serve(listener, token.clone(), Arc::new(echo()), MAX_CONNECTIONS);

    let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    let mut writer = stream.try_clone().unwrap();
    let mut reader = BufReader::new(stream);
    for id in 1..=2 {
        let request = json!({ "id": id, "token": token, "method": "echo", "params": [id] });
        writeln!(writer, "{request}").unwrap();
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let response: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(response, json!({ "id": id, "result": [id] }));
    }
}

#[test]
fn the_session_is_answered_without_the_window() {
    let h = harness();
    let backend = AppBackend { app: h.handle() };
    let session = backend.call("session.get", Value::Null).unwrap();
    assert!(session.get("project").is_some(), "{session}");
}

#[test]
fn other_methods_are_forwarded_to_the_window_and_answered_through_the_hub() {
    let h = harness();
    let app = h.handle();
    let listener_app = app.clone();
    app.listen(CONTROL_REQUEST_EVENT, move |event| {
        let request: Value = serde_json::from_str(event.payload()).unwrap();
        assert_eq!(request["method"], "commands.run");
        assert_eq!(request["params"], json!({ "id": "split" }));
        let id = request["requestId"].as_u64().unwrap();
        listener_app
            .state::<AppState>()
            .control
            .reply(id, Ok(json!("done")));
    });
    let backend = AppBackend { app };
    let result = backend.call("commands.run", json!({ "id": "split" }));
    assert_eq!(result, Ok(json!("done")));
}

#[test]
fn a_window_error_comes_back_as_the_error() {
    let h = harness();
    let app = h.handle();
    let listener_app = app.clone();
    app.listen(CONTROL_REQUEST_EVENT, move |event| {
        let request: Value = serde_json::from_str(event.payload()).unwrap();
        let id = request["requestId"].as_u64().unwrap();
        listener_app
            .state::<AppState>()
            .control
            .reply(id, Err("no such command".into()));
    });
    let backend = AppBackend { app };
    assert_eq!(
        backend.call("commands.run", json!({})),
        Err("no such command".to_string())
    );
}

#[test]
fn a_late_or_unknown_reply_is_ignored() {
    let hub = ControlHub::default();
    assert!(!hub.reply(99, Ok(Value::Null)));
    let (id, receiver) = hub.register();
    assert!(hub.reply(id, Ok(json!(1))));
    assert_eq!(receiver.recv().unwrap(), Ok(json!(1)));
    assert!(!hub.reply(id, Ok(json!(2))), "an answer is taken once");
}

#[test]
fn a_client_over_the_connection_limit_is_turned_away_and_a_freed_slot_is_reused() {
    let h = harness();
    let (listener, token) = bind(&h.dir, 0).unwrap();
    let port = listener.local_addr().unwrap().port();
    serve(listener, token.clone(), Arc::new(echo()), 1);

    let ask = |stream: &TcpStream, id: u32| -> Value {
        let mut writer = stream.try_clone().unwrap();
        let request = json!({ "id": id, "token": token, "method": "echo", "params": [id] });
        writeln!(writer, "{request}").unwrap();
        let mut line = String::new();
        BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        serde_json::from_str(&line).unwrap()
    };

    let first = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    assert_eq!(ask(&first, 1)["result"], json!([1]));

    let second = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    let mut line = String::new();
    BufReader::new(second).read_line(&mut line).unwrap();
    assert!(line.contains("too many connections"), "{line}");

    drop(first);
    let started = std::time::Instant::now();
    loop {
        let third = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        let answer = ask(&third, 3);
        if answer["result"] == json!([3]) {
            break;
        }
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the slot was never freed"
        );
    }
}

#[test]
fn only_the_documented_methods_reach_the_window() {
    let h = harness();
    let backend = AppBackend { app: h.handle() };
    for method in ["ipc.invoke", "export", "anything.else"] {
        assert_eq!(
            backend.call(method, json!({ "command": "start_export" })),
            Err(format!("unknown method: {method}")),
        );
    }
}
