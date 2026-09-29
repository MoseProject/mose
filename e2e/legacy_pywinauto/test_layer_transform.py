# Regression coverage for the preview panel's drag-to-position layer transform
# (crates/ui/src/screens/editor/mod.rs's layer_transform_preview) — dragging the selected
# clip's canvas-space box should write a single ClipInstance::position_keyframes entry, which
# in turn moves the drawn box by the same amount on screen.
#
# The clip block and the preview's layer box are both custom-painted (no accesskit name — see
# conftest.py's module docstring), so this drives them by their reported bounding rectangles
# rather than by accessible name, same fallback the codebase already documents for timeline
# clips. Two non-obvious requirements this test relies on, found empirically:
#
# 1. The fixture video is 1 second long, and the timeline's default zoom (4 px/sec) draws it as
#    a ~4px-wide block — too small to click reliably (its trim-edge strips alone cover the
#    whole width). Ctrl+scroll-zooming in first (App::timeline_px_per_sec, clamped to 60 px/sec)
#    gives it a real body region distinct from the edges.
# 2. A synthetic drag needs real elapsed time between each mouse-move: oca only requests a
#    repaint on genuine input, but pywinauto's default press/move/release helpers (and
#    BaseWrapper.drag_mouse_input's built-in 5-step version) fire faster than consecutive
#    winit events reliably turn into separate egui frames, so the whole gesture can collapse
#    into a single button-down-then-up with no observed movement in between. Spacing moves by
#    >=0.3s each avoids that.
from __future__ import annotations

import time
from pathlib import Path

import win32api
import win32con
from pywinauto import mouse

from conftest import poll_for_descendants

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_VIDEO = REPO_ROOT / "crates" / "avbridge" / "tests" / "fixtures" / "video.mp4"
OPEN_DIALOG_FILENAME_CONTROL_ID = 1148


def _import_and_add_to_timeline(oca_window):
    from pywinauto import Desktop

    oca_window.child_window(title="Mídia", control_type="Button").click_input()

    # Retried, not single-shot: the "Importar arquivos" click occasionally doesn't land on a
    # settled frame (same category of OS-level synthetic-input timing noise
    # `_drag_layer_once`'s own retry loop below documents, ~1 in 4 observed empirically for
    # this specific click too) and the native Open dialog never spawns at all that attempt —
    # waiting longer doesn't help when the click was simply lost, only clicking again does.
    dialog = Desktop(backend="uia").window(class_name="#32770")
    for attempt in range(3):
        oca_window.child_window(
            title="⭱ Importar arquivos", control_type="Button"
        ).click_input()
        try:
            dialog.wait("visible", timeout=5)
            break
        except Exception:
            if attempt == 2:
                raise
    filename_edit = dialog.child_window(
        control_id=OPEN_DIALOG_FILENAME_CONTROL_ID, class_name="Edit"
    )
    filename_edit.set_edit_text(str(FIXTURE_VIDEO))
    filename_edit.type_keys("{ENTER}")
    assert poll_for_descendants(oca_window, FIXTURE_VIDEO.name, timeout_secs=5.0)

    oca_window.child_window(title="Editor", control_type="Button").click_input()
    time.sleep(1)
    asset_label = oca_window.child_window(
        title=FIXTURE_VIDEO.name, control_type="Text"
    )
    asset_label.double_click_input()
    time.sleep(1)


def _zoom_timeline_in(oca_window, v1_label_rect):
    # Ctrl+scroll over the track area, per timeline_panel.rs's zoom handling
    # (App::timeline_px_per_sec, MAX_PX_PER_SEC = 60.0) — enough ticks to hit the clamp
    # regardless of the scroll wheel's configured step size.
    zoom_x = v1_label_rect.right + 300
    zoom_y = v1_label_rect.mid_point().y
    win32api.SetCursorPos((zoom_x, zoom_y))
    time.sleep(0.2)
    win32api.keybd_event(win32con.VK_CONTROL, 0, 0, 0)
    try:
        for _ in range(40):
            win32api.mouse_event(win32con.MOUSEEVENTF_WHEEL, 0, 0, 120, 0)
            time.sleep(0.02)
    finally:
        win32api.keybd_event(win32con.VK_CONTROL, 0, win32con.KEYEVENTF_KEYUP, 0)
    time.sleep(0.5)


def _v1_label_rect(oca_window):
    matches = poll_for_descendants(oca_window, "V1", timeout_secs=5.0)
    assert matches, "V1 track label never appeared on the timeline"
    return matches[0].rectangle()


def _select_the_clip(oca_window):
    _zoom_timeline_in(oca_window, _v1_label_rect(oca_window))

    v1_rect = _v1_label_rect(oca_window)
    candidates = [
        d
        for d in oca_window.descendants(control_type="Custom")
        if abs(d.rectangle().mid_point().y - v1_rect.mid_point().y) <= 20
        and d.rectangle().left > v1_rect.right
    ]
    assert candidates, "no timeline clip block found after zooming in"
    # The clip's body_response covers the clip's full rect (drawn/interacted before the
    # narrower left/right trim-edge strips in timeline_panel.rs) — the widest candidate.
    body = max(candidates, key=lambda d: d.rectangle().width())
    body.click_input()
    time.sleep(0.5)


# The fixture is 320x240 (4:3) — the layer box preserves the video's own aspect ratio (see
# layer_transform_preview's tex_aspect), so filtering candidates close to 4/3 tells it apart
# from unrelated Custom elements in the preview area (panel dividers, scrollbars) far more
# reliably than picking whichever happens to have the largest area that frame.
_FIXTURE_ASPECT = 320 / 240


def _find_layer_box(oca_window, window_mid_y):
    for _ in range(20):
        candidates = [
            d.rectangle()
            for d in oca_window.descendants(control_type="Custom")
            if d.rectangle().top < window_mid_y
            and d.rectangle().width() > 100
            and d.rectangle().height() > 100
            and abs((d.rectangle().width() / d.rectangle().height()) - _FIXTURE_ASPECT) < 0.05
        ]
        if candidates:
            return max(candidates, key=lambda r: r.width() * r.height())
        time.sleep(0.3)
    return None


def _drag_layer_once(oca_window, layer_before, window_mid_y):
    """One attempt at dragging `layer_before` by (150, 100)px. Returns the layer's rect
    afterward, or `None` if it couldn't be found again."""
    start_x, start_y = layer_before.mid_point().x, layer_before.mid_point().y
    drag_dx, drag_dy = 150, 100

    # A settled hover before the button goes down (separate move, then a real pause) makes the
    # first drag_delta() sample land inside the widget reliably — pressing and moving in the
    # same breath occasionally missed the very first frame's delta in testing.
    mouse.move(coords=(start_x, start_y))
    time.sleep(0.3)
    mouse.press(button="left", coords=(start_x, start_y))
    time.sleep(0.3)
    steps = 10
    for i in range(1, steps + 1):
        mouse.move(
            coords=(
                start_x + drag_dx * i // steps,
                start_y + drag_dy * i // steps,
            )
        )
        time.sleep(0.3)
    mouse.release(button="left", coords=(start_x + drag_dx, start_y + drag_dy))
    time.sleep(1)
    return _find_layer_box(oca_window, window_mid_y)


def test_dragging_the_preview_layer_moves_it(oca_window):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    time.sleep(1)
    _select_the_clip(oca_window)

    window_rect = oca_window.rectangle()
    window_mid_y = (window_rect.top + window_rect.bottom) // 2
    layer_before = _find_layer_box(oca_window, window_mid_y)
    assert layer_before is not None, "preview layer box never appeared after selecting the clip"

    # Retried, not single-shot: occasionally the very first synthetic mouse-down doesn't land
    # on a settled frame (observed ~1 in 4 in testing) and the drag never starts, leaving the
    # layer at its original position — a real regression would fail identically on every
    # attempt, so retrying only masks OS-level input timing noise, not an actual break in
    # ClipInstance::position_keyframes wiring.
    layer_after = None
    for _attempt in range(3):
        layer_after = _drag_layer_once(oca_window, layer_before, window_mid_y)
        if layer_after is not None and layer_after.left != layer_before.left:
            break

    assert layer_after is not None, "preview layer box disappeared after dragging"
    assert layer_after.width() == layer_before.width()
    assert layer_after.height() == layer_before.height()

    actual_dx = layer_after.left - layer_before.left
    actual_dy = layer_after.top - layer_before.top
    # Generous tolerance (not an exact-pixel match): the drag delta is converted to a
    # canvas-fraction offset and back, so rounding plus whatever fraction of the 150x100px
    # gesture landed on repainted frames both nudge the exact pixel count.
    assert actual_dx > 150 * 0.5, f"layer moved {actual_dx}px right, expected ~150"
    assert actual_dy > 100 * 0.5, f"layer moved {actual_dy}px down, expected ~100"


def _drag_resize_handle_once(oca_window, layer_before, window_mid_y):
    """One attempt at dragging the bottom-right resize handle by (90, 68)px — close to the
    fixture's own 4:3 aspect so `_find_layer_box`'s aspect filter still matches the box
    afterward. Returns the layer's rect afterward, or `None` if it couldn't be found again."""
    handle_x = layer_before.right
    handle_y = layer_before.bottom
    grow_dx, grow_dy = 90, 68

    mouse.move(coords=(handle_x, handle_y))
    time.sleep(0.3)
    mouse.press(button="left", coords=(handle_x, handle_y))
    time.sleep(0.3)
    steps = 10
    for i in range(1, steps + 1):
        mouse.move(
            coords=(
                handle_x + grow_dx * i // steps,
                handle_y + grow_dy * i // steps,
            )
        )
        time.sleep(0.3)
    mouse.release(button="left", coords=(handle_x + grow_dx, handle_y + grow_dy))
    time.sleep(1)
    return _find_layer_box(oca_window, window_mid_y)


def test_dragging_the_resize_handle_grows_the_layer(oca_window):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    time.sleep(1)
    _select_the_clip(oca_window)

    window_rect = oca_window.rectangle()
    window_mid_y = (window_rect.top + window_rect.bottom) // 2
    layer_before = _find_layer_box(oca_window, window_mid_y)
    assert layer_before is not None, "preview layer box never appeared after selecting the clip"

    # Same retry rationale as test_dragging_the_preview_layer_moves_it above — a real
    # regression in ClipInstance::layer_scale_x/_y would fail identically every attempt.
    layer_after = None
    for _attempt in range(3):
        layer_after = _drag_resize_handle_once(oca_window, layer_before, window_mid_y)
        if layer_after is not None and layer_after.width() != layer_before.width():
            break

    assert layer_after is not None, "preview layer box disappeared after resizing"
    grew_w = layer_after.width() - layer_before.width()
    grew_h = layer_after.height() - layer_before.height()
    assert grew_w > 90 * 0.5, f"layer grew {grew_w}px wider, expected ~90"
    assert grew_h > 68 * 0.5, f"layer grew {grew_h}px taller, expected ~68"
