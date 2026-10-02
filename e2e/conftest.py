# pytest + Selenium/WebDriver end-to-end harness for the oca desktop app (Tauri + WebView2).
#
# Drives the real target/debug/oca-desktop.exe through tauri-driver, which wraps Microsoft's
# msedgedriver and speaks the standard WebDriver protocol against the app's WebView2 window --
# unlike the retired pywinauto/UI Automation harness (egui had no DOM), interactions here are
# plain CSS selectors against the real React tree, the same one a browser devtools inspector
# would show. Prefer `[data-testid=...]` selectors over text content, since display strings are
# locale-dependent (see apps/desktop/src/i18n.ts).
#
# One-time setup:
#   cargo install tauri-driver
#   python -m pip install -r e2e/requirements.txt
# Needs a build first -- **through the Tauri CLI** (`cd apps/desktop && npm run tauri build --
# --no-bundle`, or `make desktop-build`), never a plain `cargo build -p desktop`. Confirmed the
# hard way: a binary built by plain `cargo build` (even `--release`) embeds `devUrl`
# ("http://localhost:5173") instead of the built `frontendDist`, so the launched window shows a
# WebView2 "can't reach this page" error with no Vite dev server running -- this is exactly the
# packaging bug `release.yml` had until this was diagnosed (see tauri-migration.md gap 7); the
# Tauri CLI sets env/cfg state a bare `cargo build` never does. Also needs the FFmpeg/GStreamer
# runtime DLL directories on PATH for the *launched process* -- same requirement as running
# target/release/oca-desktop.exe by hand (see CLAUDE.md's Commands section). `make test-e2e`
# exports both before invoking pytest.
#
# tauri-driver listens on TAURI_DRIVER_PORT (default 4444) and spawns msedgedriver itself; only
# one instance may own that port at a time, so it's started once per test session, not per test.
# TAURI_NATIVE_DRIVER points tauri-driver at a specific msedgedriver.exe when it can't find one
# matching the installed WebView2 Runtime automatically.
#
# OCA_DESKTOP_EXE overrides which built binary to launch (default: target/release/oca-desktop.exe,
# since a *debug* build can abort at startup with an unlogged "Microsoft Visual C++ Runtime
# Library" dialog on some machines -- confirmed via a real WinDbg/procdump capture to be a
# debug-CRT-only heap-validation false alarm on a statically-linked C++ dependency's global
# std::map destructor, absent from `--release` builds, and not a shipping concern since
# release.yml only ever builds `--release`).
from __future__ import annotations

import os
import subprocess
import time
from pathlib import Path

import pytest
from selenium import webdriver

REPO_ROOT = Path(__file__).resolve().parents[1]
APP_EXE = Path(os.environ.get("OCA_DESKTOP_EXE", REPO_ROOT / "target" / "release" / "oca-desktop.exe"))
DRIVER_PORT = int(os.environ.get("TAURI_DRIVER_PORT", "4444"))
NATIVE_DRIVER = os.environ.get("TAURI_NATIVE_DRIVER")
LAUNCH_TIMEOUT_SECS = 20


def poll_until(predicate, timeout_secs=5.0, interval_secs=0.1):
    """Retries `predicate()` until it returns a truthy value or `timeout_secs` elapses.

    A single snapshot isn't enough: `preview-tick`/`session-changed` events land asynchronously,
    so a UI change (navigation, a background import landing) isn't necessarily reflected in the
    DOM the instant the triggering action (click(), etc.) returns.
    """
    deadline = time.monotonic() + timeout_secs
    result = predicate()
    while not result and time.monotonic() < deadline:
        time.sleep(interval_secs)
        result = predicate()
    return result


@pytest.fixture(scope="session")
def tauri_driver_process():
    """Starts one tauri-driver for the whole test session; each test gets its own app process
    via the `oca_driver` fixture below, but they all speak WebDriver through this single port."""
    command = ["tauri-driver", "--port", str(DRIVER_PORT)]
    if NATIVE_DRIVER:
        command.extend(["--native-driver", NATIVE_DRIVER])
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    )
    time.sleep(2.0)  # tauri-driver has no ready signal; give msedgedriver time to bind.
    if process.poll() is not None:
        output = process.stdout.read().decode(errors="replace") if process.stdout else ""
        pytest.fail(
            "tauri-driver exited immediately. Install a matching msedgedriver.exe or set "
            f"TAURI_NATIVE_DRIVER. Driver output: {output.strip()}"
        )
    yield process
    process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()


@pytest.fixture
def oca_driver(tauri_driver_process):
    """Launches a fresh oca-desktop.exe under tauri-driver, yields a Selenium WebDriver bound to
    its WebView2 window, then tears both down."""
    if not APP_EXE.exists():
        pytest.fail(f"{APP_EXE} not found -- build it first: cd apps/desktop && npm run tauri build -- --no-bundle")

    capabilities = {
        "tauri:options": {"application": str(APP_EXE)},
        "browserName": "wry",
    }
    driver = webdriver.Remote(
        command_executor=f"http://127.0.0.1:{DRIVER_PORT}",
        desired_capabilities=capabilities,
    )
    try:
        poll_until(
            lambda: driver.execute_script("return document.readyState") == "complete",
            timeout_secs=LAUNCH_TIMEOUT_SECS,
        )
        yield driver
    finally:
        driver.quit()
