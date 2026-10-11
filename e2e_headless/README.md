# Headless end-to-end tests

These tests launch the **real `oca-desktop` binary** and drive it over its control channel
(`spec/architecture/control-channel.md`), so they need no WebDriver, no Selenium and no native
dialogs, and run on Windows, macOS and Linux. They are the automated replacement for opening the
app and trying things by hand.

What they cover, against the live app (real webview, real backend, real files):

| File | Checks |
| --- | --- |
| `test_control_channel.py` | the token, the command table the window really holds, running and undoing commands, disabled and unknown commands, the connection limit |
| `test_crash_recovery.py` | the journal for a never-saved project is written, survives `kill -9`, is not consumed by the next launch, and abandoned directories are cleaned |
| `test_mcp.py` | `oca-mcp` speaking JSON-RPC over stdio to the live app: tools, edits, errors, the app going away |

Each test gets a fresh app in a private home directory (`HOME`, `APPDATA`, `XDG_*` are pointed at
a temp folder), so nothing of the user's is read or changed.

## Running

```bash
# 1. Build the app THROUGH the Tauri CLI (a plain cargo build embeds the dev server URL):
cd apps/desktop && npm run tauri build -- --no-bundle     # add --debug for a debug build
# 2. Build the MCP server:
cargo build -p oca-mcp
# 3. Run (FFmpeg's DLLs/libs must be on PATH / LD_LIBRARY_PATH for the launched app):
python -m pip install pytest
make test-e2e-headless
```

`OCA_DESKTOP_EXE` and `OCA_MCP_EXE` pick specific binaries (default: `target/release` then
`target/debug`). Without a built app the tests skip with that instruction. On Linux without a
`DISPLAY` the app is started under `xvfb-run`. A debug build takes about 15 s to be ready, so a
full run takes a few minutes.

## What they do not cover

The visual layout and pointer interaction (dragging clips, the canvas timeline) still need the
WebDriver suite in `e2e/` (Windows only). The recovery section of the home screen is covered by
`apps/desktop/tests/screens/Home.test.tsx` and, through the real handler, by
`apps/desktop/src-tauri/src/handler_test.rs`.
