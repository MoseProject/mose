# The preferences screen offers the Bush preview engine only in builds with the `bush-preview`
# feature, and the choice is saved for the next start. Restores the original value afterwards so
# running the suite does not change the real prefs file.
from __future__ import annotations

from pathlib import Path

import pytest
from selenium.webdriver.common.by import By
from selenium.webdriver.support.ui import Select

from conftest import poll_until
from helpers import invoke

OUT_DIR = Path(__file__).resolve().parent / "out"


def test_the_preview_engine_choice_is_offered_and_saved(oca_driver):
    engines = invoke(oca_driver, "preview_engines")
    if "bush" not in engines:
        pytest.skip("this build has no bush-preview feature")

    original = invoke(oca_driver, "get_prefs")["previewEngine"]
    try:
        oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="nav-prefs"]').click()
        assert poll_until(
            lambda: oca_driver.find_elements(By.CSS_SELECTOR, '[data-testid="preview-engine"]'),
            timeout_secs=5.0,
        ), "the engine picker never appeared on the preferences screen"
        picker = Select(oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="preview-engine"]'))
        picker.select_by_value("bush")
        assert poll_until(
            lambda: invoke(oca_driver, "get_prefs")["previewEngine"] == "bush", timeout_secs=5.0
        ), "choosing Bush was not saved to the preferences"
        OUT_DIR.mkdir(exist_ok=True)
        oca_driver.save_screenshot(str(OUT_DIR / "prefs_preview_engine.png"))
    finally:
        prefs = invoke(oca_driver, "get_prefs")
        prefs["previewEngine"] = original
        invoke(oca_driver, "set_prefs", {"prefs": prefs})
