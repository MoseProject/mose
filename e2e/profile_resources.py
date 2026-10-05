# Resource profile of the real app: launches oca-desktop.exe through tauri-driver, walks it through
# a few editing scenarios and samples RAM, CPU and GPU of the whole process tree (the app plus
# its WebView2 children) while each one runs. Not a pytest test: it prints a table and writes
# `e2e/out/profile_<label>.json`.
#
#   python e2e/profile_resources.py [--label NAME] [--exe PATH]
#
# `OCA_PROFILE_MEDIA` is a directory holding `clip1080.mp4` (1080p60, with audio) and
# `clip4k.mp4` (2160p30); `ffmpeg` can synthesize both with `testsrc2` and `sine`.
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import threading
import time
from pathlib import Path

import psutil
from selenium import webdriver

sys.path.insert(0, str(Path(__file__).resolve().parent))
from conftest import DRIVER_PORT, poll_until  # noqa: E402
from helpers import _active_timeline, get_session, invoke  # noqa: E402

REPO_ROOT = Path(__file__).resolve().parents[1]
OUT_DIR = Path(__file__).resolve().parent / "out"
MEDIA = Path(os.environ.get("OCA_PROFILE_MEDIA", REPO_ROOT / "target" / "perf" / "media"))
CORES = psutil.cpu_count(logical=True)


def tree(root_pid):
    try:
        root = psutil.Process(root_pid)
        return [root] + root.children(recursive=True)
    except psutil.NoSuchProcess:
        return []


GPU_SCRIPT = r"""
$m = (Get-Counter '\GPU Process Memory(*)\Dedicated Usage' -ErrorAction SilentlyContinue).CounterSamples
$e = (Get-Counter '\GPU Engine(*)\Utilization Percentage' -ErrorAction SilentlyContinue).CounterSamples
$out = @{ mem = @(); eng = @() }
foreach ($s in $m) { if ($s.CookedValue -gt 0) { $out.mem += ,@($s.InstanceName, [double]$s.CookedValue) } }
foreach ($s in $e) { if ($s.CookedValue -gt 0) { $out.eng += ,@($s.InstanceName, [double]$s.CookedValue) } }
$out | ConvertTo-Json -Compress -Depth 4
"""


def _pairs(items):
    if not items:
        return []
    if isinstance(items[0], str):
        return [(items[0], items[1])]
    return [(a, b) for a, b in items]


def gpu_by_pid(pids):
    """Dedicated GPU memory (bytes) and engine utilization (sum over engines, percent) of `pids`."""
    try:
        raw = subprocess.run(
            ["powershell", "-NoProfile", "-Command", GPU_SCRIPT],
            capture_output=True,
            text=True,
            timeout=20,
        ).stdout
        data = json.loads(raw)
    except Exception:
        return None, None
    wanted = tuple(f"pid_{pid}_" for pid in pids)
    mem = sum(v for name, v in _pairs(data.get("mem")) if name.startswith(wanted))
    eng = sum(v for name, v in _pairs(data.get("eng")) if name.startswith(wanted))
    return mem / (1024 * 1024), eng


class Sampler(threading.Thread):
    """RAM and CPU twice a second; GPU (a slow PowerShell query) on its own thread."""

    def __init__(self, pid):
        super().__init__(daemon=True)
        self.pid = pid
        self.samples = []
        self.running = True
        self.gpu = (None, None)
        self.procs = {}
        if not os.environ.get("OCA_PROFILE_NO_GPU"):
            threading.Thread(target=self._gpu_loop, daemon=True).start()

    def _gpu_loop(self):
        while self.running:
            self.gpu = gpu_by_pid([p.pid for p in tree(self.pid)])

    def run(self):
        while self.running:
            rss = priv = 0
            cpu = 0.0
            count = 0
            for p in tree(self.pid):
                # The same Process object across samples: its CPU percent is the change since
                # the previous call.
                p = self.procs.setdefault(p.pid, p)
                try:
                    info = p.memory_full_info()
                    rss += info.rss
                    priv += getattr(info, "private", info.uss)
                    cpu += p.cpu_percent(interval=None)
                    count += 1
                except (psutil.NoSuchProcess, psutil.AccessDenied):
                    continue
            self.samples.append(
                {
                    "t": time.time(),
                    "rss_mb": rss / 2**20,
                    "private_mb": priv / 2**20,
                    "cpu_pct": cpu / CORES,
                    "gpu_mem_mb": self.gpu[0],
                    "gpu_eng_pct": self.gpu[1],
                    "procs": count,
                }
            )
            time.sleep(0.5)

    def window(self, start, end):
        return [s for s in self.samples if start <= s["t"] <= end]


def summarize(samples):
    out = {"samples": len(samples)}
    for key in ("rss_mb", "private_mb", "cpu_pct", "gpu_mem_mb", "gpu_eng_pct"):
        values = [s[key] for s in samples if s[key] is not None]
        out[key] = {"peak": max(values), "mean": sum(values) / len(values)} if values else {}
    return out


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--label", default="run")
    parser.add_argument("--exe", default=str(REPO_ROOT / "target" / "fast" / "release" / "oca-desktop.exe"))
    args = parser.parse_args()

    driver_process = subprocess.Popen(
        ["tauri-driver", "--port", str(DRIVER_PORT)], stdout=subprocess.DEVNULL, stderr=subprocess.STDOUT
    )
    time.sleep(2.0)
    driver = None
    results = {"label": args.label, "exe": args.exe, "phases": {}, "notes": {}}
    sampler = None
    try:
        driver = webdriver.Remote(
            command_executor=f"http://127.0.0.1:{DRIVER_PORT}",
            desired_capabilities={"tauri:options": {"application": args.exe}, "browserName": "wry"},
        )
        poll_until(lambda: driver.execute_script("return document.readyState") == "complete", timeout_secs=30)
        app_pid = next(p.pid for p in psutil.process_iter(["name"]) if p.info["name"] == Path(args.exe).name)
        sampler = Sampler(app_pid)
        sampler.start()

        def phase(name, action, settle=0.0):
            start = time.time()
            action()
            if settle:
                time.sleep(settle)
            end = time.time()
            results["phases"][name] = {"seconds": end - start, **summarize(sampler.window(start, end))}
            print(f"[{name}] {end - start:.1f}s", flush=True)

        def js_heap():
            return driver.execute_script(
                "return performance.memory ? performance.memory.usedJSHeapSize/1048576 : null"
            )

        phase("idle_home", lambda: None, settle=8)
        results["notes"]["js_heap_mb_idle"] = js_heap()

        paths = [str(MEDIA / "clip1080.mp4"), str(MEDIA / "clip4k.mp4")]

        def import_all():
            invoke(driver, "import_media", {"paths": paths})
            poll_until(
                lambda: len(get_session(driver)["project"]["media_library"]) >= 2
                and all(a.get("has_waveform") or a.get("waveform_peaks") for a in get_session(driver)["project"]["media_library"]),
                timeout_secs=300,
                interval_secs=0.5,
            )

        phase("import_2_clips", import_all, settle=2)
        assets = get_session(driver)["project"]["media_library"]
        results["notes"]["assets"] = [a["file_name"] for a in assets]

        invoke(driver, "add_track", {"kind": "Video"})
        invoke(driver, "add_track", {"kind": "Audio"})
        track = next(t for t in _active_timeline(get_session(driver))["tracks"] if t["kind"] == "Video")
        # Four cuts back to back (three of the 1080p clip, one of the 4K) so playback crosses sources.
        for asset in (assets[0], assets[0], assets[1], assets[0]):
            invoke(driver, "add_asset_to_timeline", {"assetId": asset["id"], "trackId": track["id"]})
        time.sleep(1)

        phase("editor_idle", lambda: None, settle=8)
        results["notes"]["js_heap_mb_editor"] = js_heap()

        def scrub():
            for i in range(120):
                invoke(driver, "preview_seek", {"positionSecs": (i * 1.7) % 100})
                time.sleep(0.05)

        phase("scrub_120_seeks", scrub, settle=1)

        def play():
            invoke(driver, "preview_seek", {"positionSecs": 0})
            invoke(driver, "preview_set_playing", {"playing": True})
            time.sleep(30)
            invoke(driver, "preview_set_playing", {"playing": False})

        phase("play_30s", play, settle=1)

        clip = next(t for t in _active_timeline(get_session(driver))["tracks"] if t["kind"] == "Video")["clips"][0]

        def edit_burst():
            started = time.time()
            for i in range(300):
                invoke(driver, "update_clip", {"clipId": clip["id"], "patch": {"brightness": (i % 20) / 100.0}})
            results["notes"]["edit_ms_each"] = (time.time() - started) / 300 * 1000

        phase("edit_burst_300", edit_burst, settle=2)

        def edits_while_playing():
            invoke(driver, "preview_set_playing", {"playing": True})
            for i in range(150):
                invoke(driver, "update_clip", {"clipId": clip["id"], "patch": {"contrast": 1.0 + (i % 10) / 100.0}})
            invoke(driver, "preview_set_playing", {"playing": False})

        phase("edit_while_playing_150", edits_while_playing, settle=1)

        # A big timeline: 600 clips over two video tracks, then edits against it.
        def grow():
            invoke(driver, "add_track", {"kind": "Video"})
            video = [t for t in _active_timeline(get_session(driver))["tracks"] if t["kind"] == "Video"]
            for i in range(600):
                invoke(
                    driver,
                    "add_asset_to_timeline",
                    {"assetId": assets[0]["id"], "trackId": video[i % len(video)]["id"]},
                )

        phase("grow_to_600_clips", grow, settle=2)
        results["notes"]["clips"] = sum(
            len(t["clips"]) for t in _active_timeline(get_session(driver))["tracks"]
        )

        def big_edits():
            started = time.time()
            for i in range(300):
                invoke(driver, "update_clip", {"clipId": clip["id"], "patch": {"brightness": (i % 20) / 100.0}})
            results["notes"]["edit_ms_each_big"] = (time.time() - started) / 300 * 1000

        phase("edit_burst_300_big", big_edits, settle=2)

        phase("idle_after", lambda: None, settle=10)
        results["notes"]["js_heap_mb_end"] = js_heap()
    finally:
        if sampler is not None:
            sampler.running = False
        if driver is not None:
            driver.quit()
        driver_process.terminate()

    OUT_DIR.mkdir(exist_ok=True)
    (OUT_DIR / f"profile_{args.label}.json").write_text(json.dumps(results, indent=2))
    header = f"{'phase':26} {'rss pk':>7} {'priv pk':>7} {'cpu mean':>8} {'cpu pk':>6} {'gpuMB pk':>8} {'gpu% mean':>9}"
    print(header)
    for name, p in results["phases"].items():

        def g(key, field):
            v = p.get(key, {}).get(field)
            return "-" if v is None else f"{v:.0f}"

        print(
            f"{name:26} {g('rss_mb', 'peak'):>7} {g('private_mb', 'peak'):>7} {g('cpu_pct', 'mean'):>8} "
            f"{g('cpu_pct', 'peak'):>6} {g('gpu_mem_mb', 'peak'):>8} {g('gpu_eng_pct', 'mean'):>9}"
        )
    print(json.dumps(results["notes"], indent=2))


if __name__ == "__main__":
    main()
