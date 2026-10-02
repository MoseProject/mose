# Regression coverage for the Text tool's click-to-create (apps/desktop/src/editor/
# PreviewPanel.tsx's onCanvasClick text-tool branch, ported from ui::App::add_text_clip_at) --
# pressing T then clicking the preview should create a TextClip at that canvas-fraction position
# and select it, switching back to the Select tool.
from __future__ import annotations

from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.by import By
from selenium.webdriver.common.keys import Keys

from helpers import get_session, goto_editor
from conftest import poll_until


def test_pressing_t_then_clicking_the_preview_creates_a_text_clip(oca_driver):
    goto_editor(oca_driver)
    body = oca_driver.find_element(By.TAG_NAME, "body")
    body.send_keys("t")

    monitor = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="preview-monitor"]')
    ActionChains(oca_driver).move_to_element_with_offset(monitor, 30, -20).click().perform()

    def text_clip_created():
        session = get_session(oca_driver)
        sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
        for track in sequence["timeline"]["tracks"]:
            if track["text_clips"]:
                return track["text_clips"][0]
        return None

    clip = poll_until(text_clip_created, timeout_secs=3.0)
    assert clip, "no TextClip appeared after pressing T and clicking the preview"
    # Roughly right-of-center, above-center -- exact fraction depends on the letterboxed canvas
    # rect, just checks it moved off TextClip::new's own fixed (0.1, 0.85) default.
    assert clip["pos_x"] != 0.1 or clip["pos_y"] != 0.85
