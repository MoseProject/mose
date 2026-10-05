# Frames per second the preview image actually changes at during playback, counted from the
# `src` mutations of the program monitor's <img> (what the user sees), for each source clip.
#
#   python e2e/profile_fps.py [--exe PATH] [--seconds 10]
from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from conftest import poll_until  # noqa: E402
from helpers import invoke  # noqa: E402
from selenium.webdriver.common.by import By  # noqa: E402
from profile_scenarios import DEFAULT_EXE, App  # noqa: E402

COUNT_JS = """
const cb = arguments[arguments.length - 1]; const seconds = arguments[0];
const img = document.querySelector('img[src*="/frame/"]');
if (!img) { cb({error: 'no preview image'}); return; }
let changes = 0; let loads = 0;
const mo = new MutationObserver((list) => { for (const m of list) if (m.attributeName === 'src') changes++; });
mo.observe(img, { attributes: true });
img.addEventListener('load', () => { loads++; });
let ticks = 0; let firstSeq = null; let lastSeq = null;
const handler = window.__TAURI_INTERNALS__.transformCallback((e) => { ticks++; const q = e.payload.frameSeq; if (firstSeq === null) firstSeq = q; lastSeq = q; });
window.__TAURI_INTERNALS__.invoke('plugin:event|listen', { event: 'preview-tick', target: { kind: 'Any' }, handler });
const t0 = performance.now(); let frames = 0; let longFrames = 0; let last = t0;
function raf(now) { frames++; if (now - last > 25) longFrames++; last = now; if (now - t0 < seconds * 1000) requestAnimationFrame(raf); else { mo.disconnect(); cb({ticksPerSec: ticks / seconds, seqPerSec: (lastSeq - firstSeq) / seconds, srcChanges: changes / seconds, loads: loads / seconds, rafPerSec: frames / seconds, rafOver25ms: longFrames}); } }
requestAnimationFrame(raf);
"""


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--exe", default=str(DEFAULT_EXE))
    parser.add_argument("--seconds", type=float, default=10)
    args = parser.parse_args()
    with App(args.exe) as app:
        assets = app.import_clips(["clip1080.mp4", "clip4k.mp4"])
        app.lay_out([assets[0]])
        # The session is dirty by now, so Home offers the way back into the editor.
        app.driver.execute_script("document.querySelector('[data-testid=back-to-editor]')?.click()")
        poll_until(lambda: app.driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-editor"]'), timeout_secs=5)
        for label, asset in (("1080p60", assets[0]), ("2160p30", assets[1])):
            app.lay_out([asset])
            invoke(app.driver, "preview_seek", {"positionSecs": 0})
            time.sleep(1.5)
            invoke(app.driver, "preview_set_playing", {"playing": True})
            time.sleep(1.5)
            print(label, app.driver.execute_async_script(COUNT_JS, args.seconds), flush=True)
            invoke(app.driver, "preview_set_playing", {"playing": False})
            invoke(app.driver, "undo")


if __name__ == "__main__":
    main()
