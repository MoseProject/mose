# Regression coverage for the preview panel's drag-to-position/resize layer transform
# (apps/desktop/src/editor/PreviewPanel.tsx, ported from crates/ui/src/screens/editor/mod.rs's
# layer_transform_preview) -- dragging the selected clip's on-canvas box should write a single
# ClipInstance::position_keyframes entry (move) or layer_scale_x/y (resize), same effect the
# egui shell's equivalent drag has.
from __future__ import annotations

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By

from helpers import get_session, import_and_add_to_timeline, select_the_clip
from conftest import poll_until


def _clip(session, clip_id):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return next(
        c for t in sequence["timeline"]["tracks"] for c in t["clips"] if c["id"] == clip_id
    )


def test_dragging_the_preview_layer_moves_it(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    select_the_clip(oca_driver, clip_id)

    layer = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="preview-layer"]')
    ActionChains(oca_driver).move_to_element(layer).click_and_hold().move_by_offset(40, 25).release().perform()

    def moved():
        clip = _clip(get_session(oca_driver), clip_id)
        keyframes = clip["position_keyframes"]
        return keyframes if keyframes and (keyframes[0]["value"]["x"] != 0 or keyframes[0]["value"]["y"] != 0) else None

    keyframes = poll_until(moved, timeout_secs=3.0)
    assert keyframes, "dragging the preview layer never wrote a position_keyframes entry"
    assert keyframes[0]["value"]["x"] > 0, "layer should have moved right"
    assert keyframes[0]["value"]["y"] > 0, "layer should have moved down"


def test_dragging_the_resize_handle_grows_the_layer(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    select_the_clip(oca_driver, clip_id)

    before = _clip(get_session(oca_driver), clip_id)
    handle = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="preview-layer-resize-handle"]')
    ActionChains(oca_driver).move_to_element(handle).click_and_hold().move_by_offset(30, 20).release().perform()

    def grew():
        clip = _clip(get_session(oca_driver), clip_id)
        return clip if clip["layer_scale_x"] != before["layer_scale_x"] else None

    after = poll_until(grew, timeout_secs=3.0)
    assert after, "dragging the resize handle never changed layer_scale_x"
    assert after["layer_scale_x"] > before["layer_scale_x"]
    assert after["layer_scale_y"] > before["layer_scale_y"]
