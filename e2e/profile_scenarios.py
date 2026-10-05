# Longer and more specific resource scenarios for the running app, next to
# `profile_resources.py` (which gives the general before/after table). Not pytest tests.
#
#   python e2e/profile_scenarios.py soak --minutes 10
#   python e2e/profile_scenarios.py export [--tracks N]
#   python e2e/profile_scenarios.py longmedia
#   python e2e/profile_scenarios.py renderers
#   python e2e/profile_scenarios.py render
#
# Media comes from `OCA_PROFILE_MEDIA` (see profile_resources.py) plus `long2h.mp4`, a two hour
# 540p clip with audio, for `longmedia`.
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import time
from pathlib import Path

import psutil
from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By

sys.path.insert(0, str(Path(__file__).resolve().parent))
from conftest import DRIVER_PORT, poll_until  # noqa: E402
from helpers import _active_timeline, get_session, invoke  # noqa: E402
from profile_resources import GPU_SCRIPT, MEDIA, REPO_ROOT, Sampler, _pairs, summarize, tree  # noqa: E402

DEFAULT_EXE = REPO_ROOT / "target" / "perf" / "oca-after.exe"
OUT_DIR = Path(__file__).resolve().parent / "out"


class App:
    """The app under tauri-driver with a resource sampler; `env` reaches the app process."""

    def __init__(self, exe, env=None):
        self.exe = Path(exe)
        self.env = env or {}

    def __enter__(self):
        os.environ.update(self.env)
        self.driver_process = subprocess.Popen(
            ["tauri-driver", "--port", str(DRIVER_PORT)], stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT
        )
        time.sleep(2.0)
        self.driver = webdriver.Remote(
            command_executor=f"http://127.0.0.1:{DRIVER_PORT}",
            desired_capabilities={"tauri:options": {"application": str(self.exe)}, "browserName": "wry"},
        )
        poll_until(lambda: self.driver.execute_script("return document.readyState") == "complete", timeout_secs=30)
        self.driver.set_script_timeout(900)
        self.pid = next(p.pid for p in psutil.process_iter(["name"]) if p.info["name"] == self.exe.name)
        self.sampler = Sampler(self.pid)
        if not os.environ.get("OCA_PROFILE_NO_SAMPLER"):
            self.sampler.start()
        return self

    def __exit__(self, *exc):
        self.sampler.running = False
        self.driver.quit()
        self.driver_process.terminate()
        for key in self.env:
            os.environ.pop(key, None)

    def heap_mb(self):
        return self.driver.execute_script(
            "return performance.memory ? performance.memory.usedJSHeapSize/1048576 : null"
        )

    def now_stats(self):
        recent = self.sampler.samples[-3:]
        return summarize(recent) if recent else {}

    def import_clips(self, names, timeout=900):
        invoke(self.driver, "import_media", {"paths": [str(MEDIA / n) for n in names]})
        poll_until(
            lambda: len(get_session(self.driver)["project"]["media_library"]) >= len(names)
            and all(a.get("has_waveform") for a in get_session(self.driver)["project"]["media_library"]),
            timeout_secs=timeout,
            interval_secs=0.5,
        )
        return get_session(self.driver)["project"]["media_library"]

    def lay_out(self, assets_in_order):
        invoke(self.driver, "add_track", {"kind": "Video"})
        track = [t for t in _active_timeline(get_session(self.driver))["tracks"] if t["kind"] == "Video"][-1]
        for asset in assets_in_order:
            invoke(self.driver, "add_asset_to_timeline", {"assetId": asset["id"], "trackId": track["id"]})
        return track


def gpu_engines(pids):
    """Utilization per GPU engine type (3D, VideoDecode, VideoEncode, ...) summed over `pids`."""
    raw = subprocess.run(
        ["powershell", "-NoProfile", "-Command", GPU_SCRIPT], capture_output=True, text=True, timeout=30
    ).stdout
    data = json.loads(raw)
    wanted = tuple(f"pid_{pid}_" for pid in pids)
    out = {}
    for name, value in _pairs(data.get("eng")):
        if name.startswith(wanted):
            kind = name.split("engtype_")[-1]
            out[kind] = out.get(kind, 0.0) + value
    return out


def soak(args):
    """Repeats a cycle of playback, edits, undo and scrubbing and reports whether memory drifts."""
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4", "clip4k.mp4"])
        app.lay_out([assets[0], assets[0], assets[1], assets[0]])
        clip = next(t for t in _active_timeline(get_session(app.driver))["tracks"] if t["kind"] == "Video")["clips"][0]
        parts = set(args.parts.split(","))
        rows = []
        began = time.time()
        end = began + args.minutes * 60
        while time.time() < end:
            if "play" in parts:
                invoke(app.driver, "preview_seek", {"positionSecs": 0})
                invoke(app.driver, "preview_set_playing", {"playing": True})
                time.sleep(8)
                invoke(app.driver, "preview_set_playing", {"playing": False})
            if "edit" in parts:
                for i in range(40):
                    invoke(app.driver, "update_clip", {"clipId": clip["id"], "patch": {"brightness": (i % 20) / 100.0}})
            if "undo" in parts:
                for _ in range(20):
                    invoke(app.driver, "undo")
            if "seek" in parts:
                for i in range(30):
                    invoke(app.driver, "preview_seek", {"positionSecs": (i * 3.1) % 100})
                    time.sleep(0.05)
            if not parts & {"play", "seek"}:
                time.sleep(1)
            stats = app.now_stats()
            rows.append(
                {
                    "cycle": len(rows) + 1,
                    "minute": round((time.time() - began) / 60, 1),
                    "rss_mb": stats["rss_mb"]["peak"],
                    "private_mb": stats["private_mb"]["peak"],
                    "js_heap_mb": app.heap_mb(),
                }
            )
            print(rows[-1], flush=True)
        third = max(1, len(rows) // 3)
        report = {
            "cycles": len(rows),
            "first_third_rss_mean": sum(r["rss_mb"] for r in rows[:third]) / third,
            "last_third_rss_mean": sum(r["rss_mb"] for r in rows[-third:]) / third,
            "first_third_private_mean": sum(r["private_mb"] for r in rows[:third]) / third,
            "last_third_private_mean": sum(r["private_mb"] for r in rows[-third:]) / third,
            "first_js_heap": rows[0]["js_heap_mb"],
            "last_js_heap": rows[-1]["js_heap_mb"],
            "rows": rows,
        }
        print(json.dumps({k: v for k, v in report.items() if k != "rows"}, indent=2))
        return report


def export(args):
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4"])
        for _ in range(args.tracks):
            app.lay_out([assets[0]])
        if args.effects:
            patch = json.loads(os.environ["OCA_FX_PATCH"]) if os.environ.get("OCA_FX_PATCH") else {"brightness": 0.1, "contrast": 1.1, "saturation": 1.2, "flippedH": True, "cropW": 0.9,
                     "voiceCleanupEnabled": True}
            for track in _active_timeline(get_session(app.driver))["tracks"]:
                for clip in track.get("clips", []):
                    invoke(app.driver, "update_clip", {"clipId": clip["id"], "patch": patch})
        out = REPO_ROOT / "target" / "perf" / f"export_out_{int(time.time())}.mp4"
        out.unlink(missing_ok=True)
        before = len(app.sampler.samples)
        started = time.time()
        invoke(app.driver, "start_export", {"path": str(out), "rename": False})
        engines = {}
        progress = []

        def done():
            jobs = invoke(app.driver, "list_exports")
            if jobs:
                progress.append((round(time.time() - started, 1), str(jobs[-1]["status"])))
            for engine, value in ({} if os.environ.get("OCA_PROFILE_NO_GPU") else gpu_engines([p.pid for p in tree(app.pid)])).items():
                engines[engine] = max(engines.get(engine, 0.0), value)
            status = jobs[-1]["status"] if jobs else None
            if status == "Done" or (isinstance(status, dict) and "Failed" in status):
                return jobs[-1]
            return None

        job = poll_until(done, timeout_secs=900, interval_secs=0.5)
        seconds = time.time() - started
        report = {
            "job": job,
            "wall_seconds": seconds,
            "source_seconds": 60,
            "speed_x": 60 / seconds,
            "output_mb": out.stat().st_size / 2**20 if out.exists() else None,
            "tree": summarize(app.sampler.samples[before:]),
            "gpu_engine_peaks_pct": engines,
            "progress": progress[:: max(1, len(progress) // 40)],
        }
        print(json.dumps(report, indent=2, default=str))
        return report


FRAME_COUNT_JS = """
const cb = arguments[arguments.length - 1];
const seconds = arguments[0];
const seen = new Set(); let total = 0; const end = performance.now() + seconds * 1000;
async function loop() {
  while (performance.now() < end) {
    try {
      const r = await fetch('http://oca.localhost/frame/' + Date.now());
      if (r.ok) {
        const b = new Uint8Array(await r.arrayBuffer()); let h = 5381;
        for (let i = 0; i < b.length; i += 97) { h = ((h * 33) ^ b[i]) >>> 0; }
        total += 1; seen.add(b.length + ':' + h);
      }
    } catch (e) {}
    await new Promise(r => setTimeout(r, 4));
  }
  cb({distinct: seen.size, polled: total});
}
loop();
"""

FETCH_WAVE_JS = """
const cb = arguments[arguments.length - 1]; const t = performance.now();
fetch('http://oca.localhost/wave/' + arguments[0]).then(r => r.arrayBuffer())
  .then(b => cb({bytes: b.byteLength, ms: performance.now() - t})).catch(e => cb({error: String(e)}));
"""

FETCH_TILES_JS = """
const cb = arguments[arguments.length - 1]; const [id, n, dur] = arguments; let ok = 0; const t = performance.now();
(async () => {
  for (let i = 0; i < n; i++) {
    const ms = Math.floor(((i * 7919) % n) / n * dur * 1000);
    try { const r = await fetch('http://oca.localhost/thumb/' + id + '/' + ms); if (r.ok) { await r.arrayBuffer(); ok++; } } catch (e) {}
  }
  cb({ok, ms: performance.now() - t});
})();
"""


def longmedia(args):
    with App(args.exe) as app:
        started = time.time()
        invoke(app.driver, "import_media", {"paths": [str(MEDIA / "long2h.mp4")]})
        poll_until(
            lambda: (get_session(app.driver)["project"]["media_library"] or [{}])[0].get("has_waveform"),
            timeout_secs=1200,
            interval_secs=1.0,
        )
        asset = get_session(app.driver)["project"]["media_library"][0]
        report = {
            "import_seconds": time.time() - started,
            "duration_secs": asset["duration_secs"],
            "after_import": app.now_stats(),
        }
        app.lay_out([asset])
        time.sleep(3)
        # The detailed waveform: a decode pass over two hours of audio, then MBs of floats.
        report["detail_waveform"] = app.driver.execute_async_script(FETCH_WAVE_JS, asset["id"])
        time.sleep(3)
        report["after_waveform"] = app.now_stats()
        # Thumbnail tiles spread over the recording: more distinct tiles than the cache keeps.
        for n in (400, 2000):
            report[f"tiles_{n}"] = app.driver.execute_async_script(FETCH_TILES_JS, asset["id"], n, asset["duration_secs"])
            time.sleep(2)
            report[f"after_tiles_{n}"] = app.now_stats()
        print(json.dumps(report, indent=2, default=str))
        return report


def renderers(args):
    """The same playback with the CPU or GPU compositor and software or hardware decode."""
    results = {}
    for label, env in (
        ("cpu_render_sw_decode", {}),
        ("gpu_render_sw_decode", {"OCA_BUSH_RENDERER": "gpu"}),
        ("cpu_render_hw_decode", {"OCA_BUSH_DECODE": "hardware"}),
        ("gpu_render_hw_decode", {"OCA_BUSH_RENDERER": "gpu", "OCA_BUSH_DECODE": "hardware"}),
    ):
        with App(args.exe, env) as app:
            assets = app.import_clips(["clip1080.mp4", "clip4k.mp4"])
            app.lay_out([assets[0], assets[1], assets[0]])
            invoke(app.driver, "preview_seek", {"positionSecs": 0})
            time.sleep(1)
            start = time.time()
            invoke(app.driver, "preview_set_playing", {"playing": True})
            time.sleep(2)
            frames = app.driver.execute_async_script(FRAME_COUNT_JS, 15)
            engines = gpu_engines([p.pid for p in tree(app.pid)])
            invoke(app.driver, "preview_set_playing", {"playing": False})
            results[label] = {
                "frames_seen_in_15s": frames,
                "resources": summarize(app.sampler.window(start + 2, time.time())),
                "gpu_engines_now_pct": engines,
            }
            print(label, json.dumps(results[label], default=str), flush=True)
    return results


PERF_HOOK_JS = """
window.__perf = {tasks: [], frames: []};
new PerformanceObserver(l => l.getEntries().forEach(e => window.__perf.tasks.push(e.duration)))
  .observe({entryTypes: ['longtask']});
let last = performance.now();
(function tick(now) { window.__perf.frames.push(now - last); last = now; requestAnimationFrame(tick); })(last);
"""
PERF_READ_JS = "return {tasks: window.__perf.tasks.slice(), frames: window.__perf.frames.slice()}"
PERF_RESET_JS = "window.__perf.tasks.length = 0; window.__perf.frames.length = 0;"


def render(args):
    """Main-thread cost of the webview in a 600-clip session: long tasks and slow frames."""
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4"])
        app.lay_out([assets[0]])
        invoke(app.driver, "add_track", {"kind": "Video"})
        video = [t for t in _active_timeline(get_session(app.driver))["tracks"] if t["kind"] == "Video"]
        for i in range(600):
            invoke(app.driver, "add_asset_to_timeline", {"assetId": assets[0]["id"], "trackId": video[i % 2]["id"]})
        time.sleep(2)
        clip = next(t for t in _active_timeline(get_session(app.driver))["tracks"] if t["kind"] == "Video")["clips"][0]
        app.driver.execute_script(PERF_HOOK_JS)
        # The session is dirty by now, so Home offers the way back into the editor.
        app.driver.execute_script("document.querySelector('[data-testid=\"back-to-editor\"]')?.click()")
        poll_until(lambda: app.driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-editor"]'), timeout_secs=5)
        # Selects the clip by clicking near its start (the helper clicks the middle, which is far
        # off screen for a one minute clip at the default zoom).
        canvas = app.driver.find_element(By.CSS_SELECTOR, '[data-testid="timeline-canvas"]')
        size = canvas.size
        ActionChains(app.driver).move_to_element_with_offset(
            canvas, 100 - size["width"] // 2, 24 + 28 - size["height"] // 2
        ).click().perform()
        poll_until(lambda: app.driver.find_elements(By.CSS_SELECTOR, '[data-testid="inspector-clip"]'), timeout_secs=3)
        time.sleep(1)
        app.driver.execute_script(PERF_RESET_JS)
        started = time.time()
        for i in range(300):
            invoke(app.driver, "update_clip", {"clipId": clip["id"], "patch": {"brightness": (i % 20) / 100.0}})
        edit_seconds = time.time() - started
        edits = app.driver.execute_script(PERF_READ_JS)
        app.driver.execute_script(PERF_RESET_JS)
        invoke(app.driver, "preview_seek", {"positionSecs": 0})
        invoke(app.driver, "preview_set_playing", {"playing": True})
        time.sleep(10)
        invoke(app.driver, "preview_set_playing", {"playing": False})
        playing = app.driver.execute_script(PERF_READ_JS)

        def stats(name, data, seconds):
            frames = sorted(data["frames"]) or [0]
            tasks = data["tasks"]
            return {
                "name": name,
                "seconds": seconds,
                "frames": len(frames),
                "frame_ms_p50": frames[len(frames) // 2],
                "frame_ms_p95": frames[int(len(frames) * 0.95)],
                "frame_ms_max": frames[-1],
                "long_tasks": len(tasks),
                "long_task_ms_total": sum(tasks),
                "long_task_ms_max": max(tasks) if tasks else 0,
            }

        report = [
            stats("300 edits, 604 clips, clip selected (inspector open)", edits, edit_seconds),
            stats("playing 10 s", playing, 10),
        ]
        print(json.dumps(report, indent=2))
        return report


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("scenario", choices=["soak", "export", "longmedia", "renderers", "render"])
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--minutes", type=float, default=10)
    parser.add_argument("--parts", default="play,edit,undo,seek")
    parser.add_argument("--tag", default="")
    parser.add_argument("--effects", action="store_true", help="export: colour, flip, crop and voice cleanup on every clip")
    parser.add_argument("--tracks", type=int, default=1, help="export: video tracks stacked, each with the same clip")
    args = parser.parse_args()
    report = {"soak": soak, "export": export, "longmedia": longmedia, "renderers": renderers, "render": render}[
        args.scenario
    ](args)
    OUT_DIR.mkdir(exist_ok=True)
    (OUT_DIR / f"scenario_{args.scenario}{args.tag}.json").write_text(json.dumps(report, indent=2, default=str))


if __name__ == "__main__":
    main()
