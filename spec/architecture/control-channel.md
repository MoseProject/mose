# Control channel

An opt-in way to drive the running app from outside the window (automation, end-to-end tests,
agents). Idea taken from FilmCraft's JSON control channel, deliberately smaller.

- **Off by default.** `OCA_CONTROL_PORT=<port>` or `--control <port>` turns it on (port 0 picks a
  free one). It listens on `127.0.0.1` only.
- **Token.** A random 128-bit token is written, with the port, to `control.json` in the app
  config directory (owner-only on Unix). Every request must carry it.
- **Protocol.** JSON lines, one request per line, one response per line:
  `{"id":1,"token":"...","method":"commands.run","params":{"id":"split"}}` answers
  `{"id":1,"result":null}` or `{"id":1,"error":"..."}`.
- **Methods.** `session.get` (answered by the backend), `commands.list` and `commands.run {id}`
  (answered by the window, which owns the command table in `src/commands/`). `commands.run`
  refuses a disabled command, exactly as a key press would. There is no method that calls an
  arbitrary backend command, by design.
- **Limits.** At most 8 clients at once, requests up to 1 MiB, a silent connection is closed
  after 5 minutes, the window has 60 s to answer one request.

Code: `src-tauri/src/control.rs` (server, token, hub), `src/control.ts` (window side).
Not built, on purpose: calling backend commands by name, remote (non-loopback) access.

## MCP server

`crates/mcp` builds `oca-mcp`, a Model Context Protocol server on stdio (newline-delimited
JSON-RPC 2.0) that lets an agent drive the running app through the channel above. It is a
standalone binary, like `ytbridge`: it depends only on `serde_json`.

1. Start the app with `OCA_CONTROL_PORT=0`; it writes `control.json` to its config directory
   (`%APPDATA%\com.pacopacoca.oca`, `~/.config/com.pacopacoca.oca` or
   `~/Library/Application Support/com.pacopacoca.oca`).
2. Register the server with the agent, e.g. `oca-mcp --control-file <path to control.json>`
   (or set `OCA_CONTROL_FILE`).

Tools: `list_commands`, `run_command {id}`, `get_session`. A command that fails (for example a
disabled one) is a tool error, not a protocol error. The server can do exactly what the control
channel allows and nothing more.
