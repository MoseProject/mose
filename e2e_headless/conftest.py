# Fixtures for the headless end-to-end suite: launch the real oca-desktop binary with the control
# channel on, in a private home directory, and drive it over loopback TCP -- no WebDriver, no
# Selenium, no native dialogs. See README.md.
from __future__ import annotations

import json
import os
import shutil
import signal
import socket
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parents[1]
IS_WINDOWS = sys.platform == "win32"
IS_MAC = sys.platform == "darwin"
EXE_SUFFIX = ".exe" if IS_WINDOWS else ""
APP_DIR_NAME = "com.pacopacoca.oca"
READY_TIMEOUT_SECS = float(os.environ.get("OCA_E2E_READY_TIMEOUT", "180"))
# How long the app waits for the window to answer one forwarded request (control.rs).
REPLY_WINDOW_SECS = 60.0


def poll_until(predicate, timeout_secs=10.0, interval_secs=0.1):
    """Retries `predicate()` until it returns something truthy or the time runs out."""
    deadline = time.monotonic() + timeout_secs
    result = predicate()
    while not result and time.monotonic() < deadline:
        time.sleep(interval_secs)
        result = predicate()
    return result


def _find_binary(env_name: str, name: str) -> Path | None:
    explicit = os.environ.get(env_name)
    if explicit:
        return Path(explicit)
    for profile in ("release", "debug"):
        candidate = REPO_ROOT / "target" / profile / (name + EXE_SUFFIX)
        if candidate.exists():
            return candidate
    return None


class ControlError(RuntimeError):
    """The app answered a control request with an error."""


class App:
    """One launch of the app, with its own home directory."""

    def __init__(self, exe: Path, home: Path):
        self.exe = exe
        self.home = home
        self.process: subprocess.Popen | None = None
        self.log_path = home / "app.log"

    # -- where the app keeps things, per platform ------------------------------------------------

    @property
    def config_dir(self) -> Path:
        if IS_WINDOWS:
            return self.home / "AppData" / "Roaming" / APP_DIR_NAME
        if IS_MAC:
            return self.home / "Library" / "Application Support" / APP_DIR_NAME
        return self.home / ".config" / APP_DIR_NAME

    @property
    def control_file(self) -> Path:
        return self.config_dir / "control.json"

    @property
    def recovery_root(self) -> Path:
        return self.config_dir / "recovery"

    def _environment(self) -> dict:
        env = os.environ.copy()
        env["HOME"] = str(self.home)
        env["OCA_CONTROL_PORT"] = "0"
        env.setdefault("OCA_LOG", "info")
        if IS_WINDOWS:
            env["USERPROFILE"] = str(self.home)
            env["APPDATA"] = str(self.home / "AppData" / "Roaming")
            env["LOCALAPPDATA"] = str(self.home / "AppData" / "Local")
        elif not IS_MAC:
            env["XDG_CONFIG_HOME"] = str(self.home / ".config")
            env["XDG_DATA_HOME"] = str(self.home / ".local" / "share")
            env["XDG_CACHE_HOME"] = str(self.home / ".cache")
            # Software rendering keeps WebKitGTK from needing a GPU under a virtual display.
            env.setdefault("WEBKIT_DISABLE_COMPOSITING_MODE", "1")
            env.setdefault("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        return env

    def _command(self) -> list[str]:
        needs_display = not IS_WINDOWS and not IS_MAC and not os.environ.get("DISPLAY")
        if needs_display:
            if shutil.which("xvfb-run") is None:
                pytest.skip("no DISPLAY and no xvfb-run to provide one")
            return ["xvfb-run", "-a", str(self.exe)]
        return [str(self.exe)]

    # -- lifecycle -------------------------------------------------------------------------------

    def start(self) -> "App":
        self.home.mkdir(parents=True, exist_ok=True)
        # A leftover file from an earlier launch would point at a dead port.
        self.control_file.unlink(missing_ok=True)
        log = open(self.log_path, "ab")
        kwargs = {}
        if IS_WINDOWS:
            kwargs["creationflags"] = subprocess.CREATE_NEW_PROCESS_GROUP
        else:
            kwargs["start_new_session"] = True
        self.process = subprocess.Popen(
            self._command(),
            env=self._environment(),
            stdout=log,
            stderr=subprocess.STDOUT,
            **kwargs,
        )
        self._wait_ready()
        return self

    def _wait_ready(self) -> None:
        """Waits for the control port, then for the window to answer a forwarded request.

        The window can take a while to load. A request it has not answered yet is held by the
        server for up to REPLY_WINDOW_SECS, so the probe waits longer than that instead of giving
        up early and leaving requests (and connection slots) behind.
        """
        deadline = time.monotonic() + READY_TIMEOUT_SECS
        last_error = "the control file never appeared"
        port_up = False
        while time.monotonic() < deadline:
            if self.process.poll() is not None:
                raise RuntimeError(f"the app exited early (code {self.process.returncode})\n{self.log()}")
            if self.control_file.exists():
                try:
                    if not port_up:
                        self.call("session.get", timeout=10)  # answered by the backend itself
                        port_up = True
                    self.call("commands.list", timeout=REPLY_WINDOW_SECS + 15)
                    return
                except (OSError, ControlError, ValueError) as error:
                    last_error = str(error)
            time.sleep(0.5)
        raise RuntimeError(f"the app was not ready in {READY_TIMEOUT_SECS}s: {last_error}\n{self.log()}")

    def log(self) -> str:
        try:
            return self.log_path.read_text(errors="replace")[-4000:]
        except OSError:
            return ""

    def kill(self) -> None:
        """What a crash looks like: the process and its children die with no cleanup."""
        if self.process is None or self.process.poll() is not None:
            return
        if IS_WINDOWS:
            subprocess.run(["taskkill", "/F", "/T", "/PID", str(self.process.pid)], capture_output=True)
        else:
            os.killpg(os.getpgid(self.process.pid), signal.SIGKILL)
        self.process.wait(timeout=30)

    def stop(self) -> None:
        self.kill()

    # -- the control channel ---------------------------------------------------------------------

    def info(self) -> dict:
        return json.loads(self.control_file.read_text())

    def raw(self, request: dict, timeout: float = 30.0) -> dict:
        """Sends `request` as is and returns the decoded response line."""
        with socket.create_connection(("127.0.0.1", self.info()["port"]), timeout=timeout) as sock:
            sock.sendall((json.dumps(request) + "\n").encode())
            data = b""
            while not data.endswith(b"\n"):
                chunk = sock.recv(65536)
                if not chunk:
                    break
                data += chunk
        return json.loads(data)

    def call(self, method: str, params=None, timeout: float = 30.0):
        """Calls a control method and returns its result; raises `ControlError` on an error."""
        response = self.raw(
            {"id": 1, "token": self.info()["token"], "method": method, "params": params},
            timeout=timeout,
        )
        if "error" in response:
            raise ControlError(response["error"])
        return response.get("result")

    def session(self) -> dict:
        return self.call("session.get")

    def markers(self) -> list:
        return self.session()["project"]["sequences"][0]["timeline"]["markers"]


@pytest.fixture(scope="session")
def app_exe() -> Path:
    exe = _find_binary("OCA_DESKTOP_EXE", "oca-desktop")
    if exe is None or not exe.exists():
        pytest.skip(
            "no built app: run `cd apps/desktop && npm run tauri build -- --no-bundle` "
            "(never a plain cargo build) or set OCA_DESKTOP_EXE"
        )
    return exe


@pytest.fixture(scope="session")
def mcp_exe() -> Path:
    exe = _find_binary("OCA_MCP_EXE", "oca-mcp")
    if exe is None or not exe.exists():
        pytest.skip("no built oca-mcp: run `cargo build -p oca-mcp` or set OCA_MCP_EXE")
    return exe


@pytest.fixture
def home(tmp_path_factory) -> Path:
    # Short, fresh and outside the repository, so nothing of the user's is touched.
    path = Path(tempfile.mkdtemp(prefix="oca-e2e-"))
    yield path
    shutil.rmtree(path, ignore_errors=True)


@pytest.fixture
def launch(app_exe, home):
    """Starts the app in `home`; every app it started is killed at the end of the test."""
    started: list[App] = []

    def _launch(at: Path | None = None) -> App:
        app = App(app_exe, at or home)
        started.append(app)
        return app.start()

    yield _launch
    for app in started:
        app.stop()


@pytest.fixture
def app(launch) -> App:
    return launch()
