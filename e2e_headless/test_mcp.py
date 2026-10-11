# The MCP server against the real app: oca-mcp is started the way an agent would start it, speaks
# JSON-RPC over stdio, and everything it does goes through the live control channel.
from __future__ import annotations

import json
import subprocess

import pytest


class McpClient:
    def __init__(self, exe, control_file):
        self.process = subprocess.Popen(
            [str(exe), "--control-file", str(control_file)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        self.next_id = 0

    def send(self, message: dict) -> None:
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method: str, params=None) -> dict:
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params or {}})
        line = self.process.stdout.readline()
        assert line, self.process.stderr.read()
        response = json.loads(line)
        assert response["id"] == self.next_id
        return response

    def call_tool(self, name: str, arguments=None) -> dict:
        return self.request("tools/call", {"name": name, "arguments": arguments or {}})["result"]

    def close(self) -> None:
        self.process.stdin.close()
        self.process.wait(timeout=15)


@pytest.fixture
def mcp(app, mcp_exe):
    client = McpClient(mcp_exe, app.control_file)
    init = client.request("initialize", {"protocolVersion": "2025-06-18", "capabilities": {}})
    assert init["result"]["serverInfo"]["name"] == "oca"
    client.send({"jsonrpc": "2.0", "method": "notifications/initialized"})
    yield client
    client.close()


def text_of(result: dict):
    return json.loads(result["content"][0]["text"])


def test_the_three_tools_are_offered(mcp):
    tools = mcp.request("tools/list")["result"]["tools"]
    assert [tool["name"] for tool in tools] == ["list_commands", "run_command", "get_session"]


def test_list_commands_returns_the_windows_command_table(mcp):
    result = mcp.call_tool("list_commands")
    assert result["isError"] is False
    ids = {command["id"] for command in text_of(result)}
    assert {"split", "addMarker", "undo"} <= ids


def test_run_command_edits_the_project_the_app_holds(mcp, app):
    assert app.markers() == []
    result = mcp.call_tool("run_command", {"id": "addMarker"})
    assert result["isError"] is False
    assert len(app.markers()) == 1
    session = text_of(mcp.call_tool("get_session"))
    assert len(session["project"]["sequences"][0]["timeline"]["markers"]) == 1
    assert session["dirty"] is True


def test_a_failing_command_is_a_tool_error_not_a_protocol_error(mcp):
    unknown = mcp.call_tool("run_command", {"id": "nope"})
    assert unknown["isError"] is True
    assert "unknown command" in unknown["content"][0]["text"]
    missing = mcp.call_tool("run_command", {})
    assert missing["isError"] is True


def test_a_tool_that_does_not_exist_is_refused(mcp):
    response = mcp.request("tools/call", {"name": "start_export", "arguments": {}})
    assert response["error"]["code"] == -32602


def test_the_server_reports_when_the_app_is_gone(mcp, app):
    app.kill()
    result = mcp.call_tool("get_session")
    assert result["isError"] is True
    # Right after the kill the port either refuses the connection or resets it.
    message = result["content"][0]["text"]
    assert "not listening" in message or "no answer from the app" in message, message
