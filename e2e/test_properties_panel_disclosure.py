# Regression coverage for the inspector's collapsible property sections
# (apps/desktop/src/editor/Inspector.tsx's Section component, ported from
# crates/ui/src/components/property.rs's property_section) -- a fresh clip added straight from
# the media library has every property at its default, so its Gain section should start
# collapsed, expand on click, and collapse again on a second click.
from __future__ import annotations

from selenium.webdriver.common.by import By

from helpers import import_and_add_to_timeline, select_the_clip


def test_gain_section_starts_collapsed_and_toggles_on_click(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    select_the_clip(oca_driver, clip_id)

    section = oca_driver.find_element(By.CSS_SELECTOR, '[data-testid="section-gain"]')
    assert not section.get_attribute("open"), (
        "Gain section should start collapsed for a freshly added, untouched clip (gain_db == 0)"
    )

    summary = section.find_element(By.CSS_SELECTOR, "summary")
    # A native <summary> toggles on click; WebDriver refuses to click it when the inspector is
    # scrolled, so the click is dispatched in the page.
    oca_driver.execute_script("arguments[0].click()", summary)
    assert section.get_attribute("open"), "clicking the Gain section header didn't expand it"

    oca_driver.execute_script("arguments[0].click()", summary)
    assert not section.get_attribute("open"), (
        "clicking the Gain section header a second time didn't collapse it back"
    )
