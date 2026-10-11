# The control channel against the real app: the token, the command table the window really
# holds, running commands, and the limits. These cross every layer the unit tests stub out: the
# TCP server, the Tauri event to the webview, the React handler, and the reply command back.
from __future__ import annotations

import socket

import pytest

from conftest import ControlError

CORE_COMMANDS = {"split", "undo", "redo", "addMarker", "playPause", "copy", "delete", "save"}


def test_the_session_is_the_blank_project_the_app_starts_with(app):
    session = app.session()
    assert session["project"]["name"] == "Untitled"
    assert len(session["project"]["sequences"]) == 1
    assert session["dirty"] is False
    assert session["filePath"] is None
    assert session["project"]["frame_rate"] is None


def test_a_request_without_the_right_token_is_refused(app):
    for token in (None, "", "wrong", app.info()["token"][:-1]):
        request = {"id": 7, "method": "session.get"}
        if token is not None:
            request["token"] = token
        assert app.raw(request) == {"id": 7, "error": "unauthorized"}


def test_garbage_gets_an_answer_and_the_channel_keeps_working(app):
    with socket.create_connection(("127.0.0.1", app.info()["port"]), timeout=10) as sock:
        sock.sendall(b"this is not json\n")
        assert b"not valid JSON" in sock.makefile("rb").readline()
    assert app.session()["project"]["name"] == "Untitled"


def test_the_command_table_is_listed_by_the_real_window(app):
    commands = app.call("commands.list")
    ids = {command["id"] for command in commands}
    assert CORE_COMMANDS <= ids, CORE_COMMANDS - ids
    assert len(commands) >= 50
    for command in commands:
        assert set(command) == {"id", "label", "group", "keys", "enabled"}
        assert command["label"] and command["group"]
        assert isinstance(command["keys"], list)
        assert isinstance(command["enabled"], bool)
    split = next(command for command in commands if command["id"] == "split")
    assert split["keys"] == ["S", "C"]


def test_running_a_command_edits_the_project(app):
    assert app.markers() == []
    app.call("commands.run", {"id": "addMarker"})
    assert len(app.markers()) == 1
    assert app.session()["dirty"] is True


def test_an_edit_made_through_the_channel_can_be_undone_by_one_too(app):
    app.call("commands.run", {"id": "addMarker"})
    assert len(app.markers()) == 1
    app.call("commands.run", {"id": "undo"})
    assert app.markers() == []
    app.call("commands.run", {"id": "redo"})
    assert len(app.markers()) == 1


def test_a_disabled_command_is_refused_like_a_key_press(app):
    disabled = [command for command in app.call("commands.list") if not command["enabled"]]
    assert disabled, "a fresh project has commands that cannot run yet (nothing is selected)"
    with pytest.raises(ControlError, match="command is disabled"):
        app.call("commands.run", {"id": disabled[0]["id"]})
    assert app.markers() == []


def test_unknown_commands_and_methods_are_refused(app):
    with pytest.raises(ControlError, match="unknown command: nope"):
        app.call("commands.run", {"id": "nope"})
    with pytest.raises(ControlError, match="unknown command"):
        app.call("commands.run", {})
    for method in ("nope", "ipc.invoke", "start_export", "invoke"):
        with pytest.raises(ControlError, match="unknown method"):
            app.call(method, {"command": "start_export", "args": {}})


def test_at_most_eight_clients_are_served_at_once(app):
    port = app.info()["port"]
    held = [socket.create_connection(("127.0.0.1", port), timeout=10) for _ in range(8)]
    try:
        # The server counts a connection once it has accepted it, so ask on each to be sure.
        token = app.info()["token"]
        for sock in held:
            sock.sendall(b'{"id":1,"token":"%s","method":"session.get"}\n' % token.encode())
            assert b'"result"' in sock.makefile("rb").readline()
        with socket.create_connection(("127.0.0.1", port), timeout=10) as ninth:
            assert b"too many connections" in ninth.makefile("rb").readline()
    finally:
        for sock in held:
            sock.close()
    # A freed slot is reused.
    assert app.session()["project"]["name"] == "Untitled"


def test_the_window_stays_responsive_over_many_requests(app):
    for _ in range(40):
        assert len(app.call("commands.list")) >= 50


def test_the_window_reports_ready_so_forwarded_requests_never_wait_out_the_timeout(app):
    import time

    started = time.monotonic()
    app.call("commands.list")
    assert time.monotonic() - started < 10
