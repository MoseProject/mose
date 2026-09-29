# Regression coverage for timeline_panel's viewport culling
# (crates/ui/src/screens/editor/timeline_panel/mod.rs) — a clip whose left edge is already past
# the visible right edge of the timeline is now skipped entirely (no paint, no ui.interact call)
# to avoid the documented O(all clips)-per-frame cost on long timelines (TIMELINE_PERFORMANCE.md).
#
# The risk that change introduced: dragging a clip such that it crosses that boundary mid-
# gesture must not silently abandon the drag — culling would otherwise stop calling
# ui.interact() for that clip's id every frame, which is what keeps egui's own drag-tracking
# alive for it. Guarded in the Rust code by never culling a clip that's the target of an
# in-progress drag (checked against its body/trim-start/trim-end widget ids via
# ui.ctx().dragged_id()). This test drags a selected clip out past the timeline panel's right
# edge and back to a resting position still inside it, in one continuous gesture, and checks the
# final position reflects the full round-trip delta rather than getting stuck wherever the
# boundary was first crossed.
#
# Not verified against a live app in the environment this was written in (no display/Windows
# available) — the drag continuing to track once the pointer leaves the app's own window bounds
# relies on standard Windows mouse-capture behavior during a button-down drag (winit/egui's
# usual behavior, same assumption test_layer_transform.py's drags already make implicitly by
# never needing to stay inside the window), but that specific "leaves and returns" path hasn't
# been exercised before. Run this on a real Windows dev machine to confirm before trusting it.
from __future__ import annotations

import time

from pywinauto import mouse

from test_layer_transform import (
    FIXTURE_VIDEO,
    _import_and_add_to_timeline,
    _v1_label_rect,
    _zoom_timeline_in,
)


def _find_clip_body(oca_window, v1_mid_y):
    candidates = [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_mid_y) <= 20
    ]
    if not candidates:
        return None
    # Same reasoning as test_layer_transform._select_the_clip: the clip body's interact rect
    # covers the clip's full width, wider than the narrow left/right trim-edge strips drawn (and
    # interacted with) on top of it.
    return max(candidates, key=lambda d: d.rectangle().width())


def test_dragging_a_clip_past_the_timeline_edge_and_back_preserves_the_full_move(oca_window):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    v1_rect = _v1_label_rect(oca_window)
    _zoom_timeline_in(oca_window, v1_rect)
    v1_rect = _v1_label_rect(oca_window)
    v1_mid_y = v1_rect.mid_point().y

    clip_before = _find_clip_body(oca_window, v1_mid_y)
    assert clip_before is not None, "no timeline clip block found after zooming in"
    clip_before_rect = clip_before.rectangle()
    start_x = clip_before_rect.mid_point().x
    start_y = clip_before_rect.mid_point().y

    window_rect = oca_window.rectangle()
    # Deliberately past the whole window's own right edge (not just the timeline panel's,
    # which sits somewhere left of it) -- guarantees the drag crosses timeline_panel's culling
    # boundary partway through, then returns to a spot still comfortably inside the visible
    # timeline before release.
    far_x = window_rect.right + 80
    end_x = start_x + 120

    mouse.move(coords=(start_x, start_y))
    time.sleep(0.3)
    mouse.press(button="left", coords=(start_x, start_y))
    time.sleep(0.3)
    for x in (start_x + 100, far_x, end_x):
        mouse.move(coords=(x, start_y))
        time.sleep(0.4)
    mouse.release(button="left", coords=(end_x, start_y))
    time.sleep(1)

    clip_after = _find_clip_body(oca_window, v1_mid_y)
    assert clip_after is not None, (
        "timeline clip disappeared after dragging past the window edge and back — either the "
        "drag was abandoned mid-gesture (culling regression) or the clip is now positioned "
        "somewhere this search doesn't look"
    )
    actual_dx = clip_after.rectangle().left - clip_before_rect.left
    intended_dx = end_x - start_x
    assert actual_dx > intended_dx * 0.5, (
        f"clip moved {actual_dx}px, expected roughly {intended_dx}px — the drag may have been "
        "abandoned when it crossed the timeline's viewport culling boundary instead of "
        "continuing to track back to the release point"
    )
    assert oca_window.exists()  # still alive: no panic
