# Regression coverage for basic timeline clip selection -- clicking a clip's canvas-space body
# should select it and show its properties in the Inspector panel. The timeline is a
# hand-painted <canvas> with no per-clip DOM element (see helpers.py's select_the_clip), so this
# is the one thing worth checking on its own: every other command-driven test bypasses the
# canvas entirely and never exercises this click path.
from __future__ import annotations

from helpers import FIXTURE_VIDEO, import_and_add_to_timeline, select_the_clip


def test_clicking_a_clip_selects_it_and_shows_it_in_the_inspector(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    select_the_clip(oca_driver, clip_id)  # asserts internally that the inspector shows it
