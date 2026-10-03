# End-to-end check of the preview (Bush, the app's only preview engine): the first frame shows,
# seeks and plays through a cut produce new frames, text and shape overlays composite, and the
# audio meter moves. Besides asserting that frames flow, it measures how many distinct frames
# reach the webview per second while playing and how long a seek takes to produce a new frame,
# and saves screenshots next to the test (`e2e/out/`) for a visual check.
#
#   python -m pytest e2e/test_preview.py -s
# `OCA_BUSH_E2E_VIDEO` points it at another video (for example a 1080p clip) and
# `OCA_BUSH_E2E_PLAY_SECS` sets how long it plays.
from __future__ import annotations

import os
import time
from pathlib import Path

from conftest import poll_until
from helpers import (
    FIXTURE_VIDEO,
    _active_timeline,
    get_session,
    goto_editor,
    import_and_add_to_timeline,
    invoke,
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


# Width in percent of each meter bar (PreviewPanel.tsx square-roots the linear peak).
METER_WIDTHS = """
return Array.from(document.querySelectorAll('.meter > div'))
  .map(bar => parseFloat(bar.style.width) || 0);
"""


def latest_frame(driver):
    return driver.execute_async_script(FETCH_FRAME)


def screenshot(driver, name):
    OUT_DIR.mkdir(exist_ok=True)
    driver.save_screenshot(str(OUT_DIR / name))


def test_preview_plays_across_a_cut_between_two_clips(oca_driver):
    goto_editor(oca_driver)
    clip_id = import_and_add_to_timeline(oca_driver)
    poll_until(lambda: latest_frame(oca_driver), timeout_secs=15.0, interval_secs=0.05)

    invoke(oca_driver, "preview_seek", {"positionSecs": 0.5})
    time.sleep(0.4)
    invoke(oca_driver, "split_at_playhead", {"clipIds": [clip_id]})
    session = get_session(oca_driver)
    clips = _active_timeline(session)["tracks"][0]["clips"]
    assert len(clips) == 2, f"split produced {len(clips)} clips"
    time.sleep(0.4)

    # Play from before the cut to the end: frames must keep arriving on both sides of it.
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.2})
    time.sleep(0.4)
    invoke(oca_driver, "preview_set_playing", {"playing": True})
    seen = set()
    deadline = time.monotonic() + 0.75
    while time.monotonic() < deadline:
        frame = latest_frame(oca_driver)
        if frame:
            seen.add(frame[1])
        time.sleep(0.005)
    screenshot(oca_driver, "bush_05_across_cut.png")
    invoke(oca_driver, "preview_set_playing", {"playing": False})
    print(f"\nframes across the cut: {len(seen)} distinct")
    assert len(seen) >= 8, f"playback stalled around the cut ({len(seen)} distinct frames)"


def test_preview_plays_a_clip_and_reacts_to_seeks(oca_driver):
    goto_editor(oca_driver)
    video = Path(os.environ.get("OCA_BUSH_E2E_VIDEO", FIXTURE_VIDEO))
    import_and_add_to_timeline(oca_driver, video)
    added_at = time.monotonic()

    first = poll_until(lambda: latest_frame(oca_driver), timeout_secs=15.0, interval_secs=0.01)
    print(f"\nadd-to-timeline-to-first-frame: {(time.monotonic() - added_at) * 1000:.0f} ms")
    assert first and first[0] > 1000, "the preview never produced a first frame"
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

    # Text and shape overlays go through Bush as raster contributors: adding them must change
    # the picture at a time they cover.
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.2})
    time.sleep(0.4)
    plain = latest_frame(oca_driver)
    invoke(oca_driver, "add_text_clip", {"text": "Bush overlay", "posX": 0.2, "posY": 0.4})
    invoke(oca_driver, "add_shape_clip")
    with_overlays = poll_until(
        lambda: (lambda f: f and f[1] != plain[1])(latest_frame(oca_driver)),
        timeout_secs=5.0,
        interval_secs=0.05,
    )
    time.sleep(0.4)
    screenshot(oca_driver, "bush_04_overlays.png")
    assert with_overlays, "adding a text clip and a shape did not change the preview frame"

    # Playback must keep producing distinct frames.
    invoke(oca_driver, "preview_seek", {"positionSecs": 0.0})
    time.sleep(0.3)
    invoke(oca_driver, "preview_set_playing", {"playing": True})
    seen = set()
    meter_widths = []
    deadline = time.monotonic() + float(os.environ.get("OCA_BUSH_E2E_PLAY_SECS", "0.8"))
    started = time.monotonic()
    while time.monotonic() < deadline:
        frame = latest_frame(oca_driver)
        if frame:
            seen.add(frame[1])
        meter_widths.extend(oca_driver.execute_script(METER_WIDTHS))
        time.sleep(0.005)
    elapsed = time.monotonic() - started
    screenshot(oca_driver, "bush_03_playing.png")
    invoke(oca_driver, "preview_set_playing", {"playing": False})
    print(f"distinct frames while playing: {len(seen)} in {elapsed:.2f} s ({len(seen) / elapsed:.1f}/s)")
    assert len(seen) >= 4, f"playback delivered only {len(seen)} distinct frames"
    print(f"audio meter: max bar width {max(meter_widths, default=0.0):.1f}% over {len(meter_widths)} samples")
    assert max(meter_widths, default=0.0) > 0.0, "the audio level meter never moved while playing"
