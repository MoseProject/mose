# Which process grows when the same IPC call is repeated: the app, the webview, or the harness.
#
#   python e2e/profile_leak.py [--exe PATH] [--calls 20000]
#
# Each probe runs `calls` invocations and prints the change in private memory per process.
from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

import psutil

sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import _active_timeline, get_session, invoke  # noqa: E402
from profile_resources import MEDIA  # noqa: E402
from profile_scenarios import DEFAULT_EXE, App  # noqa: E402

LOOP_JS = """
const cb = arguments[arguments.length - 1]; const [cmd, n, argsJson] = arguments;
(async () => {
  const args = JSON.parse(argsJson);
  for (let i = 0; i < n; i++) { await window.__TAURI_INTERNALS__.invoke(cmd, args(i)); }
  cb(true);
})();
"""


def private_by_process(root_pid):
    out = {}
    root = psutil.Process(root_pid)
    for p in [root] + root.children(recursive=True):
        try:
            out[p.pid] = (p.name(), p.memory_full_info().uss / 2**20)
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return out


def probe(app, label, run):
    before = private_by_process(app.pid)
    started = time.time()
    run()
    time.sleep(1)
    after = private_by_process(app.pid)
    grew = {f"{after[p][0]}:{p}": round(after[p][1] - before[p][1], 1) for p in after if p in before}
    total = sum(grew.values())
    print(f"{label:34} {time.time() - started:6.1f}s  total {total:+7.1f} MB  {grew}", flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--calls", type=int, default=20000)
    args = parser.parse_args()
    n = args.calls
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4"])
        app.lay_out([assets[0], assets[0]])
        clip = next(t for t in _active_timeline(get_session(app.driver))["tracks"] if t["kind"] == "Video")["clips"][0]
        time.sleep(3)

        def js(cmd, make_args):
            # `make_args` is JS source of a function of i.
            def run():
                app.driver.execute_async_script(LOOP_JS.replace("JSON.parse(argsJson)", make_args), cmd, n, "")

            return run

        edit = js("update_clip", f"(i) => ({{ clipId: {clip['id']}, patch: {{ brightness: (i % 20) / 100 }} }})")
        probe(app, "warm-up edits", edit)
        for round_ in range(3):
            probe(app, f"{n} update_clip (round {round_})", edit)
            probe(app, "idle 30 s", lambda: time.sleep(30))


if __name__ == "__main__":
    main()
