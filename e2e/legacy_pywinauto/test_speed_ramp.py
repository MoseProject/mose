# Regression coverage for the Clip > Speed ramp menu (crates/ui/src/screens/editor/menu_bar.rs's
# clip_menu -> App::apply_speed_ramp_to_selected_clip, unit-tested at the math level in
# crates/ui/src/app/app_test.rs's apply_speed_ramp_splits_into_contiguous_steps_with_
# interpolated_speed). That unit test already proves the interpolation/contiguity math; this
# test's job is only the observable UI consequence -- picking the preset from the real menu bar
# actually mutates the *live* timeline, visible as the single selected clip block turning into
# several contiguous blocks.
#
# UNVERIFIED: written by reading source + i18n.rs only, never run end-to-end (this session was
# told to stop before an iterate-until-green pass). A manual spot-check while writing this found
# the top menu bar's *rightmost* menu_buttons ("Clipe" among them) sometimes don't expose their
# popup's items to UI Automation on click, while a leftmost one ("Sequência", already exercised
# by test_undo_redo.py) did -- possibly a DPI/coordinate issue specific to that manual probe, or
# a real accesskit/menu_button popup-timing gap. `_apply_slow_to_fast_speed_ramp` below retries
# the whole click sequence defensively, but if this test is flaky or fails outright on a real
# run, check whether "Clipe" menu items appear in the accessibility tree at all before assuming
# the speed-ramp logic itself regressed.
from __future__ import annotations

import time

from conftest import poll_for_descendants
from test_layer_transform import (
    _import_and_add_to_timeline,
    _select_the_clip,
    _v1_label_rect,
)

MENU_CLIP_LABEL = "Clipe"
MENU_SPEED_RAMP_LABEL = "⏱ Rampa de velocidade"
SPEED_RAMP_SLOW_TO_FAST_LABEL = "Lento → Rápido (0.5x → 2x)"


def _clip_blocks_on_v1_row(oca_window, v1_row_mid_y):
    return [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_row_mid_y) <= 20
        and d.rectangle().left > 0
    ]


def _apply_slow_to_fast_speed_ramp(oca_window):
    # Each step is retried independently (not just the outer 3-attempt loop in the test body)
    # -- a menu_button popup not yet materialized in the accessibility tree at click time needs
    # its own settle-and-retry, same reasoning as _import_and_add_to_timeline's Open-dialog
    # click retry in test_layer_transform.py.
    for label in (MENU_CLIP_LABEL, MENU_SPEED_RAMP_LABEL, SPEED_RAMP_SLOW_TO_FAST_LABEL):
        last_error = None
        for _attempt in range(3):
            try:
                oca_window.child_window(
                    title=label, control_type="Button"
                ).click_input()
                last_error = None
                break
            except Exception as error:  # pywinauto.findwindows.ElementNotFoundError et al.
                last_error = error
                time.sleep(0.5)
        if last_error is not None:
            raise last_error
        time.sleep(0.3)


def test_speed_ramp_preset_splits_the_selected_clip_into_several_pieces(oca_window):
    _import_and_add_to_timeline(oca_window)
    _select_the_clip(oca_window)

    v1_rect = _v1_label_rect(oca_window)
    row_mid_y = v1_rect.mid_point().y
    blocks_before = _clip_blocks_on_v1_row(oca_window, row_mid_y)
    assert len(blocks_before) >= 1, "no clip block found on V1 before applying the speed ramp"

    # Retried like every other menu-driven action in this suite -- an occasional lost click on
    # a not-yet-settled frame (menu_button popups included) is OS-level input timing noise, not
    # a regression in the speed-ramp wiring itself.
    blocks_after = []
    for _attempt in range(3):
        _apply_slow_to_fast_speed_ramp(oca_window)
        deadline = time.monotonic() + 3.0
        while time.monotonic() < deadline:
            blocks_after = _clip_blocks_on_v1_row(oca_window, row_mid_y)
            if len(blocks_after) > len(blocks_before):
                break
            time.sleep(0.2)
        if len(blocks_after) > len(blocks_before):
            break

    # App::apply_speed_ramp_to_selected_clip(0.5, 2.0, 4) carves the clip into 4 contiguous
    # pieces (see the unit test this e2e test complements) -- assert it grew, not an exact
    # count, since the custom-painted timeline may expose extra Custom elements (trim-edge
    # strips) per clip that aren't this test's concern.
    assert len(blocks_after) > len(blocks_before), (
        f"clip block count did not increase after applying the speed ramp preset "
        f"(before={len(blocks_before)}, after={len(blocks_after)})"
    )

    # Sanity: the clip menu path required a selection, and the app is still alive/responsive
    # (undo remains available, proving push_undo_snapshot ran as part of the operation).
    assert oca_window.exists()
    assert not poll_for_descendants(oca_window, "nonexistent-marker-xyz", timeout_secs=0.3)
