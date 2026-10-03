# End-to-end check of the Bush preview backend (build with `--features bush-preview`).
#
# Run it on its own so the app launches with the engine switch in its environment:
#   OCA_PREVIEW_ENGINE=bush python -m pytest e2e/test_bush_preview.py -s
# Skipped when the variable is absent, so the normal GStreamer e2e run is unaffected. Besides
# asserting that frames flow, it measures how many distinct frames reach the webview per second
# while playing and how long a seek takes to produce a new frame, and saves screenshots next to
# the test (`e2e/out/`) for a visual check.
from __future__ import annotations

import os
import time
from pathlib import Path

import pytest

from conftest import poll_until
from helpers import FIXTURE_VIDEO, goto_editor, import_and_add_to_timeline, invoke

pytestmark = pytest.mark.skipif(
    os.environ.get("OCA_PREVIEW_ENGINE", "").lower() != "bush",
    reason="set OCA_PREVIEW_ENGINE=bush (and build with --features bush-preview)",
)

OUT_DIR = Path(__file__).resolve().parent / "out"

FETCH_FRAME = """
const cb = arguments[arguments.length - 1];
fetch('http://oca.localhost/frame/' + Date.now())
  .then(r => r.ok ? r.arrayBuffer() : null)
  .then(buf => {
    if (!buf) { cb(null); return; }
    const bytes = new Uint8Array(buf);
    let hash = 5381;
    for (let i = 0; i < bytes.length; i += 97) { hash = ((hash * 33) ^ bytes[i]) >>> 0; }
    cb([bytes.length, hash]);
  })
  .catch(() => cb(null));
"""


def latest_frame(driver):
    return driver.execute_async_script(FETCH_FRAME)


def screenshot(driver, name):
    OUT_DIR.mkdir(exist_ok=True)
    driver.save_screenshot(str(OUT_DIR / name))


def test_bush_plays_a_clip_and_reacts_to_seeks(oca_driver):
    goto_editor(oca_driver)
    video = Path(os.environ.get("OCA_BUSH_E2E_VIDEO", FIXTURE_VIDEO))
    import_and_add_to_timeline(oca_driver, video)

    first = poll_until(lambda: latest_frame(oca_driver), timeout_secs=15.0, interval_secs=0.2)
    assert first and first[0] > 1000, "Bush never produced a first preview frame"
    screenshot(oca_driver, "bush_01_loaded.png")

    # A seek must yield a different picture, and quickly.
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.0})
    time.sleep(0.4)
    start_frame = latest_frame(oca_driver)
    started = time.monotonic()
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.6})
    changed = poll_until(
        lambda: (lambda f: f and f[1] != start_frame[1])(latest_frame(oca_driver)),
        timeout_secs=5.0,
        interval_secs=0.01,
    )
    seek_ms = (time.monotonic() - started) * 1000.0
    print(f"\nseek-to-new-frame: {seek_ms:.0f} ms")
    assert changed, "seeking did not change the preview frame"
    screenshot(oca_driver, "bush_02_after_seek.png")

    # Playback must keep producing distinct frames.
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.0})
    time.sleep(0.3)
    invoke(oca_driver, "preview_set_playing", {"playing": True})
    seen = set()
    deadline = time.monotonic() + float(os.environ.get("OCA_BUSH_E2E_PLAY_SECS", "0.8"))
    started = time.monotonic()
    while time.monotonic() < deadline:
        frame = latest_frame(oca_driver)
        if frame:
            seen.add(frame[1])
        time.sleep(0.005)
    elapsed = time.monotonic() - started
    screenshot(oca_driver, "bush_03_playing.png")
    invoke(oca_driver, "preview_set_playing", {"playing": False})
    print(f"distinct frames while playing: {len(seen)} in {elapsed:.2f} s ({len(seen) / elapsed:.1f}/s)")
    assert len(seen) >= 4, f"playback delivered only {len(seen)} distinct frames"
