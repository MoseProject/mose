# Regression coverage for the clip properties panel's collapsible sections
# (components::property_section, crates/ui/src/components/property.rs +
# crates/ui/src/screens/editor/properties_panel/mod.rs) — previously every one of the panel's
# ~30 property sections (Gain, Crop, Vignette, Color Adjust, ...) was always fully expanded
# regardless of whether the selected clip actually used that effect. property_section now wraps
# its content in an egui::CollapsingHeader, closed by default unless the caller reports the
# property already holds a non-default value.
#
# This test only covers the "closed by default, click to expand/collapse" mechanics — a fresh
# clip added straight from the media library has every property at its default, so its Gain
# section should start collapsed, expand on click (revealing its slider), and collapse again on
# a second click. It does *not* cover the "already-set property starts open" half (that needs a
# clip whose properties were set before this panel is ever shown for it, e.g. a saved project
# fixture with an edited clip — no such fixture exists under e2e/ yet), which stays covered by
# the plain Rust logic (is_cropped()/has_vignette()/etc. predicates feeding `default_open`) and
# manual verification instead.
#
# Slider counts are used rather than checking for the crop DragValue/color-picker controls
# directly, since egui::Slider is a real accesskit-exposed widget (see conftest.py's module
# docstring) and every property_section this test cares about is delta-tested against its own
# open/closed slider count rather than an absolute one — the Editor screen's timeline zoom
# slider (always present) would otherwise throw off a plain "0 sliders visible" assertion.
from __future__ import annotations

import time

from pywinauto import mouse

from test_layer_transform import FIXTURE_VIDEO, _import_and_add_to_timeline, _select_the_clip

GAIN_SECTION_LABEL = "GANHO DO BLOCO"  # Text::PropGain.tr(pt_br).to_uppercase()
AUDIO_TAB_LABEL = "Áudio"  # Text::PropertiesTabAudio.tr(pt_br)


def _slider_count(oca_window):
    return len(oca_window.descendants(control_type="Slider"))


def test_gain_section_starts_collapsed_and_toggles_on_click(oca_window):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    _select_the_clip(oca_window)

    # The Gain section lives under the properties panel's "Áudio" tab (properties_panel/
    # mod.rs's properties_tab_bar) — the panel defaults to PropertiesTab::Inspector, which
    # doesn't render it at all, so it's never visible without switching tabs first.
    oca_window.child_window(title=AUDIO_TAB_LABEL, control_type="Button").click_input()
    time.sleep(0.3)

    # The panel's asset-metadata block + "Ao exportar" loudness note (above the tab bar) push
    # the tab content itself below the fold at this window size — accesskit still reports the
    # Gain header's (unreachable) logical rectangle even while it's scrolled out of view, so a
    # real click_input() at that rect silently lands on whatever's actually on-screen instead
    # (confirmed empirically: without this scroll, clicking the found header never changes the
    # slider count, with it it reliably does). Scroll the properties panel (right side of the
    # window) down before touching anything in it.
    panel_rect = oca_window.rectangle()
    mouse.scroll(
        coords=(panel_rect.right - 150, (panel_rect.top + panel_rect.bottom) // 2),
        wheel_dist=-10,
    )
    time.sleep(0.3)

    gain_header = oca_window.child_window(
        title=GAIN_SECTION_LABEL, control_type="Button"
    )
    assert gain_header.exists(), (
        "Gain section header not found in the properties panel — either the clip wasn't "
        "selected, or CollapsingHeader's accesskit exposure isn't control_type=Button "
        "(adjust this selector if so)"
    )

    before = _slider_count(oca_window)

    gain_header.click_input()
    time.sleep(0.5)
    after_open = _slider_count(oca_window)
    assert after_open > before, (
        f"clicking the Gain section header didn't reveal its slider ({before} -> "
        f"{after_open}) — it may already have been open (default_open should be false for "
        "a freshly added, untouched clip) or the click didn't land on the header"
    )

    gain_header.click_input()
    time.sleep(0.5)
    after_close = _slider_count(oca_window)
    assert after_close == before, (
        f"clicking the Gain section header a second time didn't collapse it back "
        f"({before} before, {after_close} after re-closing)"
    )
