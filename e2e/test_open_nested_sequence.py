# Regression coverage for double-clicking a compound clip to jump into its nested sequence's
# tab (apps/desktop/src/editor/Timeline.tsx's onDoubleClick, a port of
# ui::App::open_nested_sequence onto the canvas-based timeline via the already-existing
# select_sequence command).
from __future__ import annotations

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By

from conftest import poll_until
from helpers import (
    PX_PER_SEC,
    RULER_HEIGHT,
    get_session,
    goto_editor,
    import_and_add_to_timeline,
    invoke,
)


def test_double_clicking_a_compound_clip_switches_to_its_nested_sequence_tab(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    goto_editor(oca_driver)
    new_sequence_id = invoke(oca_driver, "create_compound_clip", {
        "clipId": clip_id,
        "selectedClipIds": [clip_id],
    })
    assert new_sequence_id is not None

    session = get_session(oca_driver)
    wrapper = session["project"]["sequences"][session["project"]["active_sequence"]][
        "timeline"
    ]["tracks"][0]["clips"][0]
    duration_secs = wrapper["source_out_secs"] - wrapper["source_in_secs"]
    x = int((wrapper["start_secs"] + duration_secs / 2) * PX_PER_SEC)
    y = RULER_HEIGHT + 28  # middle of track 0 (TRACK_HEIGHT // 2)

    canvas = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="timeline-canvas"]')
    size = canvas.size
    center_dx, center_dy = x - size["width"] // 2, y - size["height"] // 2
    ActionChains(oca_driver).move_to_element_with_offset(
        canvas, center_dx, center_dy
    ).double_click().perform()

    def switched():
        session = get_session(oca_driver)
        active = session["project"]["sequences"][session["project"]["active_sequence"]]
        return active if active["id"] == new_sequence_id else None

    active = poll_until(switched, timeout_secs=3.0)
    assert active, "double-clicking the compound clip should switch to its nested sequence tab"
