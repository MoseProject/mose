# How long a seek takes to reach the preview image: invokes `preview_seek` and polls the frame the
# webview would load until its bytes change.
#
#   python e2e/profile_latency.py [--exe PATH] [--seeks 40]
from __future__ import annotations

import argparse
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from helpers import invoke  # noqa: E402
from profile_scenarios import DEFAULT_EXE, App  # noqa: E402

SEEK_JS = """
const cb = arguments[arguments.length - 1]; const [positions] = arguments;
(async () => {
  const sig = async () => { const r = await fetch('http://oca.localhost/frame/' + Math.random()); if (!r.ok) return null;
    const b = new Uint8Array(await r.arrayBuffer()); let h = 5381; for (let i = 0; i < b.length; i += 53) h = ((h * 33) ^ b[i]) >>> 0; return b.length + ':' + h; };
  const out = [];
  for (const p of positions) {
    const before = await sig();
    const t0 = performance.now();
    await window.__TAURI_INTERNALS__.invoke('preview_seek', { positionSecs: p });
    let ms = null;
    while (performance.now() - t0 < 3000) { const now = await sig(); if (now && now !== before) { ms = performance.now() - t0; break; } await new Promise(r => setTimeout(r, 2)); }
    out.push(ms);
    await new Promise(r => setTimeout(r, 150));
  }
  cb(out);
})();
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--seeks", type=int, default=40)
    args = parser.parse_args()
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4", "clip4k.mp4"])
        for label, asset in (("1080p60", assets[0]), ("2160p30", assets[1])):
            app.lay_out([asset])
            time.sleep(2)
            positions = [(i * 7.3) % 55 + 1 for i in range(args.seeks)]
            times = [t for t in app.driver.execute_async_script(SEEK_JS, positions) if t is not None]
            times.sort()
            print(
                f"{label}: {len(times)}/{args.seeks} seeks answered, median {statistics.median(times):.0f} ms, "
                f"p90 {times[int(len(times) * 0.9)]:.0f} ms, max {times[-1]:.0f} ms",
                flush=True,
            )
            invoke(app.driver, "undo")


if __name__ == "__main__":
    main()
