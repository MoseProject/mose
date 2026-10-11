# Layout at the sizes the window can have (960x600 is the app's minimum, 1280x800 its default).
#
# The built frontend is served to a headless Chromium with a stand-in for the Tauri bridge, so
# no app process is needed: the CSS and the React tree are the real ones, only the backend
# answers are canned. It catches what unit tests cannot see because jsdom has no layout: a page
# wider than the window, or a control pushed off the right edge.
#
# Needs `pip install playwright` and a Chromium (OCA_CHROMIUM=/path/to/chrome, or one installed
# by `playwright install chromium`), and a built frontend (`cd apps/desktop && npx vite build`).
from __future__ import annotations

import functools
import http.server
import json
import os
import threading
from pathlib import Path

import pytest

from conftest import REPO_ROOT

playwright_sync = pytest.importorskip("playwright.sync_api", reason="pip install playwright")

DIST = Path(os.environ.get("OCA_FRONTEND_DIST", REPO_ROOT / "apps" / "desktop" / "dist"))
SIZES = [(960, 600), (1100, 700), (1280, 800), (1920, 1080)]


def _clip(clip_id, start, duration):
    return dict(
        id=clip_id, asset_id=1, start_secs=start, source_in_secs=0, source_out_secs=duration,
        speed_factor=1, gain_db=0, color_label=None, frozen=False, nested_sequence_id=None,
        brightness=0, contrast=1, saturation=1, crop_x=0, crop_y=0, crop_w=1, crop_h=1,
        mask_shape="None", mask_corner_radius=0, transition_in="None", transition_duration_secs=0.5,
        fade_in_secs=0, fade_out_secs=0, blend_mode="Normal", layer_scale_x=1, layer_scale_y=1,
        flipped_h=False, voice_cleanup_enabled=False, speed_ramp_end_factor=None,
        **{name + "_keyframes": [] for name in (
            "position", "scale", "rotation", "opacity", "gain", "brightness", "contrast",
            "saturation", "crop_x", "crop_y", "crop_w", "crop_h",
        )},
    )


def _track(track_id, name, kind, clips):
    return dict(
        id=track_id, name=name, kind=kind, clips=clips, text_clips=[], shape_clips=[], visible=True,
        audio_role="Dialogue", color_label=None, locked=False, muted=False, solo=False,
    )


# A project with media open, which is when the top bar is widest (it also shows the source's
# frame rate and size).
SESSION = dict(
    canRedo=False, canUndo=False, dirty=False, filePath=None, rev=0,
    project=dict(
        active_sequence=0, frame_rate=None, id=1, name="Untitled", last_edited=dict(HoursAgo=0),
        summary="", panel_layout=None, recent_asset_ids=[], smart_bins=[],
        media_library=[dict(
            id=1, file_name="gameplay.mp4", duration_secs=30, has_waveform=False,
            source_path="gameplay.mp4", kind="Video", has_audio=True, codec="h264",
            source_bitrate_mbps=8.0, resolution=[1920, 1080], fps=60, sample_rate_khz=48,
            loudness=None, proxy_path=None, waveform_peaks=None, favorited=False,
        )],
        sequences=[dict(
            id=1, name="Sequence 1",
            export_settings=dict(aspect_ratio="Original", target_lufs=-14.0),
            timeline=dict(
                playhead_secs=0.0, multicam_groups=[],
                markers=[dict(id=1, position_secs=3, label="a", kind="Standard", completed=False)],
                tracks=[
                    _track(10, "V1", "Video", [_clip(1, 0, 8), _clip(2, 8, 6)]),
                    _track(11, "A1", "Audio", [_clip(3, 0, 14)]),
                ],
            ),
        )],
    ),
)
PREFS = dict(
    locale="pt-BR", lufsProfile=-14, outputFolder="", autosaveMinutes=5, previewHardwareDecode=False,
    recentProjectPaths=[], savedLayerTemplates=[], whisperModelPath="", reframeModelPath="",
    backgroundRemovalModelPath="", ttsModelPath="", soundLibraryPath="",
    errorReportingConsent="Unset", lastReviewedCrashUnix=0, telemetryEnabled=False, keybindings={},
)

BRIDGE_STUB = """
(() => {
  const data = %s;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback(cb) { const id = Math.floor(Math.random() * 1e9); window['_' + id] = cb; return id; },
    unregisterCallback() {},
    convertFileSrc: (path) => path,
    metadata: { currentWindow: { label: 'main' }, currentWebview: { windowLabel: 'main', label: 'main' } },
    invoke: async (cmd) => {
      if (cmd.startsWith('plugin:event|')) return 1;
      if (cmd === 'get_session') return data.session;
      if (cmd === 'get_prefs') return data.prefs;
      if (cmd === 'list_exports' || cmd === 'get_recent_projects' || cmd === 'list_recoveries') return [];
      if (cmd === 'get_keybindings') return {};
      if (cmd === 'create_project') return true;
      return null;
    },
  };
})();
""" % json.dumps(dict(session=SESSION, prefs=PREFS))


@pytest.fixture(scope="module")
def frontend():
    if not (DIST / "index.html").exists():
        pytest.skip("no built frontend: run `cd apps/desktop && npx vite build`")

    class Quiet(http.server.SimpleHTTPRequestHandler):
        def __init__(self, *args, **kwargs):
            super().__init__(*args, directory=str(DIST), **kwargs)

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Quiet)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    yield f"http://127.0.0.1:{server.server_address[1]}/"
    server.shutdown()


@pytest.fixture(scope="module")
def browser():
    with playwright_sync.sync_playwright() as p:
        explicit = os.environ.get("OCA_CHROMIUM")
        options = {"args": ["--no-sandbox"]}
        if explicit:
            options["executable_path"] = explicit
        else:
            candidates = sorted(Path("/opt/pw-browsers").glob("chromium-*/chrome-linux/chrome"))
            if candidates and not Path(p.chromium.executable_path).exists():
                options["executable_path"] = str(candidates[-1])
        try:
            instance = p.chromium.launch(**options)
        except Exception as error:  # no browser installed
            pytest.skip(f"no Chromium to drive: {error}")
        yield instance
        instance.close()


def open_page(browser, url, width, height):
    page = browser.new_page(viewport={"width": width, "height": height})
    page.add_init_script(BRIDGE_STUB)
    page.goto(url)
    page.wait_for_selector("header.topbar")
    return page


def open_editor(browser, url, width, height):
    page = open_page(browser, url, width, height)
    page.get_by_role("button", name="Novo projeto").first.click()
    page.get_by_role("button", name="Criar projeto").click()
    page.wait_for_selector("[data-testid=screen-editor]", timeout=15000)
    page.wait_for_timeout(500)
    return page


def overflow(page):
    return page.evaluate(
        """() => {
            const past = [];
            for (const el of document.querySelectorAll('body *')) {
              const r = el.getBoundingClientRect();
              const hidden = getComputedStyle(el).visibility === 'hidden' || getComputedStyle(el).display === 'none';
              if (!hidden && r.width > 0 && r.right > window.innerWidth + 1) {
                past.push((el.dataset.testid || el.className || el.tagName).toString().slice(0, 50) + ' right=' + Math.round(r.right));
              }
            }
            return {
              pageWidth: document.documentElement.scrollWidth,
              windowWidth: window.innerWidth,
              pageHeight: document.documentElement.scrollHeight,
              windowHeight: window.innerHeight,
              past: past.slice(0, 6),
            };
        }"""
    )


@pytest.mark.parametrize("width,height", SIZES)
def test_the_editor_fits_the_window(browser, frontend, width, height):
    page = open_editor(browser, frontend, width, height)
    try:
        measured = overflow(page)
        assert measured["pageWidth"] <= measured["windowWidth"], measured
        assert measured["pageHeight"] <= measured["windowHeight"] + 1, measured
        assert measured["past"] == [], measured
    finally:
        page.close()


@pytest.mark.parametrize("width,height", SIZES)
def test_the_controls_that_matter_stay_on_screen(browser, frontend, width, height):
    page = open_editor(browser, frontend, width, height)
    try:
        for testid in ("export", "export-queue-toggle"):
            box = page.get_by_test_id(testid).bounding_box()
            assert box is not None, testid
            assert box["x"] >= 0 and box["x"] + box["width"] <= width, (testid, box)
        inspector = page.get_by_text("Inspetor", exact=True).first.bounding_box()
        assert inspector is not None and inspector["x"] + inspector["width"] <= width, inspector
        timeline = page.get_by_text("TIMELINE", exact=True).first.bounding_box()
        assert timeline is not None and timeline["y"] + timeline["height"] <= height, timeline
    finally:
        page.close()


@pytest.mark.parametrize("width,height", SIZES)
def test_the_home_screen_fits_the_window(browser, frontend, width, height):
    page = open_page(browser, frontend, width, height)
    try:
        measured = overflow(page)
        assert measured["pageWidth"] <= measured["windowWidth"], measured
        assert measured["past"] == [], measured
    finally:
        page.close()


def test_the_menus_do_not_overlap_the_project_names_at_the_minimum_width(browser, frontend):
    page = open_editor(browser, frontend, 960, 600)
    try:
        crumbs = page.locator("header.topbar [class*=crumbs]")
        if crumbs.count() and crumbs.first.is_visible():
            crumbs_box = crumbs.first.bounding_box()
            menu_box = page.get_by_role("button", name="Arquivo").first.bounding_box()
            assert crumbs_box["x"] + crumbs_box["width"] <= menu_box["x"] + 1, (crumbs_box, menu_box)
    finally:
        page.close()
