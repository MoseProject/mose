# What an idle app costs: CPU time and thread wakeups (voluntary context switches) per second of
# the app process, with the home screen up and with a clip open in the editor.
#
#   python e2e/profile_idle.py [--exe PATH] [--seconds 15]
from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

import psutil

sys.path.insert(0, str(Path(__file__).resolve().parent))
from profile_scenarios import DEFAULT_EXE, App  # noqa: E402


def rates(pid, seconds):
    process = psutil.Process(pid)
    cpu0, sw0, t0 = sum(process.cpu_times()[:2]), process.num_ctx_switches().voluntary, time.time()
    threads0 = {t.id: t.user_time + t.system_time for t in process.threads()}
    time.sleep(seconds)
    cpu1, sw1, t1 = sum(process.cpu_times()[:2]), process.num_ctx_switches().voluntary, time.time()
    busy = sorted(
        ((t.user_time + t.system_time - threads0.get(t.id, 0.0), t.id) for t in process.threads()), reverse=True
    )[:3]
    span = t1 - t0
    return f"cpu {100 * (cpu1 - cpu0) / span:.2f}% of a core, wakeups {(sw1 - sw0) / span:.0f}/s, busiest threads {[(round(b / span * 100, 2), i) for b, i in busy]}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--seconds", type=float, default=15)
    args = parser.parse_args()
    with App(args.exe) as app:
        time.sleep(3)
        print("home screen:", rates(app.pid, args.seconds), flush=True)
        assets = app.import_clips(["clip1080.mp4"])
        app.lay_out([assets[0]])
        time.sleep(3)
        print("clip loaded:", rates(app.pid, args.seconds), flush=True)


if __name__ == "__main__":
    main()
