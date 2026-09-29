# Regression coverage for click-to-place-vertex custom shape drawing
# (apps/desktop/src-tauri/src/commands/overlays.rs's add_custom_shape_clip, ported from
# ui::App::finish_drawing_custom_shape) -- clicking three points on the preview canvas then
# pressing Enter should create a ShapeClip whose shape_kind is a Polygon, on an auto-created
# Shape track.
from __future__ import annotations

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys

from helpers import get_session, goto_editor
from conftest import poll_until


def test_drawing_three_points_and_pressing_enter_creates_a_polygon_shape(oca_driver):
    goto_editor(oca_driver)
    oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="draw-custom-shape"]').click()

    monitor = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="preview-monitor"]')
    size = monitor.size
    center_dx, center_dy = -size["width"] // 2, -size["height"] // 2
    offsets = [(20, -40), (-30, 40), (30, 40)]
    actions = ActionChains(oca_driver)
    for dx, dy in offsets:
        actions.move_to_element_with_offset(monitor, center_dx + size["width"] // 2 + dx, center_dy + size["height"] // 2 + dy)
        actions.click()
    actions.perform()

    body = oca_driver.find_element(By.TAG_NAME, "body")
    body.send_keys(Keys.ENTER)

    def shape_created():
        session = get_session(oca_driver)
        sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
        for track in sequence["timeline"]["tracks"]:
            for shape in track["shape_clips"]:
                if isinstance(shape["shape_kind"], dict) and "Polygon" in shape["shape_kind"]:
                    return shape
        return None

    shape = poll_until(shape_created, timeout_secs=3.0)
    assert shape, "no Polygon shape_clip appeared after drawing 3 points and pressing Enter"
    assert len(shape["shape_kind"]["Polygon"]) == 3
