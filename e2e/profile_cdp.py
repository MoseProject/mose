# Heap accounting of the webview through the DevTools protocol the WebDriver bridge already
# opens: JS heap after a forced collection against the renderer's private memory, around bursts
# of edits. Tells apart retained JS objects, garbage waiting for a collection, and native memory.
#
#   python e2e/profile_cdp.py [--exe PATH] [--calls 20000]
from __future__ import annotations

import argparse
import json
import re
import sys
import time
import urllib.request
from pathlib import Path

import psutil
import trio
from trio_websocket import open_websocket_url

sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import _active_timeline, get_session  # noqa: E402
from profile_leak import LOOP_JS  # noqa: E402
from profile_scenarios import DEFAULT_EXE, App  # noqa: E402


def debug_port(app_pid):
    """WebView2 is started with port 0; the real one is written to `DevToolsActivePort`."""
    for p in psutil.Process(app_pid).children(recursive=True):
        try:
            cmdline = " ".join(p.cmdline())
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            continue
        found = re.search(r'--user-data-dir="?([^"]+?)"?(?: --|$)', cmdline)
        if found and "--remote-debugging-port" in cmdline:
            port_file = Path(found.group(1)) / "DevToolsActivePort"
            if port_file.exists():
                return int(port_file.read_text().splitlines()[0])
    return None


def renderer_private_mb(app_pid, page_pid=None):
    best = 0.0
    for p in psutil.Process(app_pid).children(recursive=True):
        try:
            if "--type=renderer" in " ".join(p.cmdline()):
                best = max(best, p.memory_full_info().uss / 2**20)
        except (psutil.NoSuchProcess, psutil.AccessDenied):
            pass
    return best


async def cdp_session(port, work):
    targets = json.load(urllib.request.urlopen(f"http://127.0.0.1:{port}/json"))
    page = next(t for t in targets if t.get("type") == "page")
    async with open_websocket_url(page["webSocketDebuggerUrl"]) as ws:
        counter = 0

        async def call(method, params=None):
            nonlocal counter
            counter += 1
            await ws.send_message(json.dumps({"id": counter, "method": method, "params": params or {}}))
            while True:
                reply = json.loads(await ws.get_message())
                if reply.get("id") == counter:
                    return reply.get("result", {})

        await work(call)


def cycle(app, clip, parts):
    from helpers import invoke

    if "play" in parts:
        invoke(app.driver, "preview_seek", {"positionSecs": 0})
        invoke(app.driver, "preview_set_playing", {"playing": True})
        time.sleep(8)
        invoke(app.driver, "preview_set_playing", {"playing": False})
    if "edit" in parts:
        for i in range(40):
            invoke(app.driver, "update_clip", {"clipId": clip["id"], "patch": {"brightness": (i % 20) / 100.0}})
        for _ in range(20):
            invoke(app.driver, "undo")
    if "seek" in parts:
        for i in range(30):
            invoke(app.driver, "preview_seek", {"positionSecs": (i * 3.1) % 100})
            time.sleep(0.05)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--calls", type=int, default=20000)
    parser.add_argument("--parts", default="play,edit,seek")
    parser.add_argument("--soak-minutes", type=float, default=0, help="mixed play/edit/undo/seek cycles instead of the edit burst")
    args = parser.parse_args()
    with App(args.exe) as app:
        port = debug_port(app.pid)
        print("debug port", port, flush=True)
        if port is None:
            return
        assets = app.import_clips(["clip1080.mp4"])
        app.lay_out([assets[0], assets[0]])
        clip = next(t for t in _active_timeline(get_session(app.driver))["tracks"] if t["kind"] == "Video")["clips"][0]
        edit_args = f"(i) => ({{ clipId: {clip['id']}, patch: {{ brightness: (i % 20) / 100 }} }})"
        script = LOOP_JS.replace("JSON.parse(argsJson)", edit_args)

        def native_private():
            return sum(
                p.memory_full_info().uss / 2**20
                for p in [psutil.Process(app.pid)]
            )

        async def soak(call):
            from profile_scenarios import invoke as inv

            await call("HeapProfiler.collectGarbage")
            await trio.sleep(3)
            print(f"start: renderer after GC {renderer_private_mb(app.pid):.0f} MB", flush=True)
            end = time.time() + args.soak_minutes * 60
            began = time.time()
            nxt = 0
            while time.time() < end:
                await trio.to_thread.run_sync(lambda: cycle(app, clip, args.parts))
                if time.time() - began >= nxt:
                    nxt += 120
                    print(f"min {(time.time() - began) / 60:4.1f}: app {native_private():.0f} MB, renderer {renderer_private_mb(app.pid):.0f} MB", flush=True)
            app_before = native_private()
            await call("HeapProfiler.collectGarbage")
            await trio.sleep(3)
            print(f"end: app {app_before:.0f} MB, renderer after GC {renderer_private_mb(app.pid):.0f} MB", flush=True)

        async def control(call):
            await call("HeapProfiler.collectGarbage")
            await trio.sleep(3)
            print(f"start: renderer after GC {renderer_private_mb(app.pid):.0f} MB", flush=True)
            variants = {
                "async callback only": lambda: [app.driver.execute_async_script("arguments[arguments.length-1](1)") for _ in range(args.calls)],
                "async + invoke(preview_seek)": lambda: [
                    app.driver.execute_async_script(
                        "const cb = arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke('preview_seek', {positionSecs: 1}).then(r => cb(r))"
                    )
                    for _ in range(args.calls)
                ],
            }
            for label, run in variants.items():
                await trio.to_thread.run_sync(run)
                await call("HeapProfiler.collectGarbage")
                await trio.sleep(3)
                print(f"after {args.calls} x {label}: renderer after GC {renderer_private_mb(app.pid):.0f} MB", flush=True)

        async def work(call):
            if args.parts == "control":
                return await control(call)
            if args.soak_minutes:
                return await soak(call)
            await call("Performance.enable")
            await call("HeapProfiler.startSampling", {"samplingInterval": 4096})
            await trio.to_thread.run_sync(lambda: app.driver.execute_async_script(script, "update_clip", args.calls, ""))
            profile = (await call("HeapProfiler.stopSampling"))["profile"]
            totals = {}

            def walk(node):
                frame = node["callFrame"]
                key = f"{frame['functionName'] or '(anon)'} {frame['url'].rsplit('/', 1)[-1]}:{frame['lineNumber']}"
                totals[key] = totals.get(key, 0) + node["selfSize"]
                for child in node["children"]:
                    walk(child)

            walk(profile["head"])
            total = sum(totals.values())
            print(f"sampled allocations over {args.calls} edits: {total / 2**20:.1f} MB ({total / args.calls:.0f} B per edit)")
            for key, size in sorted(totals.items(), key=lambda kv: -kv[1])[:25]:
                print(f"{size / args.calls:8.0f} B/edit  {key}")

        trio.run(cdp_session, port, work)


if __name__ == "__main__":
    import os

    os.environ.setdefault("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--remote-debugging-port=9222")
    main()
