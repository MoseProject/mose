# Regression coverage for three real Editor-layout bugs found (and fixed) by hand this session,
# via screenshots, before this file existed:
#
# 1. `properties_panel` (crates/ui/src/screens/editor/properties_panel/mod.rs) had no
#    `egui::ScrollArea` around its (potentially very long, uncollapsible) content stack. A
#    `ui.horizontal` reports the *max* height its children occupy back to its own parent
#    `ui.vertical` — so once a real selected clip's property sections overflowed past the
#    panel's own `body_height`, the whole body row's reported height inflated by however far
#    that overflow ran, pushing everything drawn afterward in `editor/mod.rs::show()` (the
#    resizable divider, then `timeline_panel::timeline_panel`) far below the visible window.
#    The timeline didn't get painted over — it got pushed off-screen. Fixed by wrapping the
#    panel's content in `egui::ScrollArea::vertical()`.
# 2. That same `ScrollArea` initially collapsed the panel to near-zero width: `ScrollArea`
#    defaults `auto_shrink` to shrink-to-fit on *both* axes, so without
#    `.auto_shrink([false, false])` its reported width collapses to whatever its narrowest
#    child naturally wants, instead of filling the `width` the caller allocated via
#    `ui.set_width(width)` one level up.
# 3. The Editor's `menu_bar` (screens/editor/menu_bar.rs) used to render as its own full-width
#    row below the custom titlebar (`screens/breadcrumb.rs`) — visually two separate bars,
#    unlike the design target's single fused top strip. Fixed by calling `menu_bar::menu_bar`
#    from inside `breadcrumb::show`'s own horizontal row instead of from
#    `editor/mod.rs::show()`.
#
# Each test below targets one of these by bounding-rectangle geometry (pywinauto UIA against the
# real compiled app, same `oca_window` fixture and `descendants(...)`/`.rectangle()` idiom
# `test_timeline_viewport_culling.py`/`test_properties_panel_disclosure.py` already use) so a
# future regression is caught by `pytest`, not by a human screenshotting the app again.
from __future__ import annotations

from test_layer_transform import (
    FIXTURE_VIDEO,
    _import_and_add_to_timeline,
    _select_the_clip,
)

# Rendered text, pt_br (this app's default locale) — same hardcode-the-rendered-string
# convention `test_properties_panel_disclosure.py`'s own GAIN_SECTION_LABEL comment documents.
TIMELINE_LABEL = "TIMELINE"  # Text::Timeline.tr(pt_br), plain ui.label (not uppercased)
PROGRAM_MONITOR_LABEL = "MONITOR DE PROGRAMA"  # Text::ProgramMonitor.tr(pt_br), via section_label (uppercased)
CODEC_ROW_LABEL = "Codec"  # Text::PropCodec.tr(pt_br), via prop_row
FILE_MENU_LABEL = "Arquivo"  # Text::MenuFile.tr(pt_br)
MIN_TIMELINE_HEIGHT_PX = 20
MIN_PROPERTIES_PANEL_WIDTH_PX = 150  # matches editor/mod.rs's own `min_col` floor


def _single(oca_window, title, control_type):
    matches = oca_window.descendants(title=title, control_type=control_type)
    assert matches, f"no {control_type} control titled {title!r} found"
    return matches[0].rectangle()


def test_timeline_stays_visible_and_onscreen_with_a_real_clip_selected(oca_window):
    """Guards regression #1 above: with a real selected clip (the case that actually overflows
    properties_panel's content), the timeline's own header label must still exist, sit below the
    Program Monitor, and stay within the window's visible bounds — not pushed off past its
    bottom edge."""
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    _select_the_clip(oca_window)

    window_rect = oca_window.rectangle()
    monitor_rect = _single(oca_window, PROGRAM_MONITOR_LABEL, "Text")
    timeline_rect = _single(oca_window, TIMELINE_LABEL, "Text")

    assert timeline_rect.height() > 0
    assert timeline_rect.top > monitor_rect.bottom, (
        f"timeline header (top={timeline_rect.top}) isn't below Program Monitor "
        f"(bottom={monitor_rect.bottom}) — it may have been pushed out of its normal spot"
    )
    assert timeline_rect.bottom <= window_rect.bottom, (
        f"timeline header (bottom={timeline_rect.bottom}) is past the window's own bottom edge "
        f"({window_rect.bottom}) — it's been pushed off-screen, the exact symptom of the "
        "unscrolled properties_panel overflow bug this test guards against"
    )


def test_properties_panel_keeps_its_configured_width_with_a_clip_selected(oca_window):
    """Guards regression #2 above: the Codec row's label must sit with real horizontal room to
    its right (the panel's actual configured width), not glued against the window's right edge
    the way it was when ScrollArea's auto_shrink collapsed the panel to near-zero width."""
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    _import_and_add_to_timeline(oca_window)
    _select_the_clip(oca_window)

    window_rect = oca_window.rectangle()
    codec_label_rect = _single(oca_window, CODEC_ROW_LABEL, "Text")

    room_to_the_right = window_rect.right - codec_label_rect.left
    assert room_to_the_right >= MIN_PROPERTIES_PANEL_WIDTH_PX, (
        f"only {room_to_the_right}px between the Codec row and the window's right edge — "
        f"expected at least {MIN_PROPERTIES_PANEL_WIDTH_PX}px (editor/mod.rs's own min_col "
        "floor); the properties panel may have collapsed to near-zero width"
    )


LOCALE_CODE_LABEL = "PT-BR"  # Locale::PtBr.short_code() — a static breadcrumb-row-only control


def test_top_bar_menu_is_fused_with_the_breadcrumb(oca_window):
    """Guards regression #3 above: the File menu button must sit in the same horizontal strip as
    the breadcrumb's own locale-code label (a static control only ever drawn inside
    `breadcrumb::show`'s row), not below it as a separate row."""
    # menu_bar only renders when app.screen == Screen::Editor (breadcrumb.rs) — no media import
    # needed for this check, just switch screens via the nav rail.
    oca_window.child_window(title="Editor", control_type="Button").click_input()

    breadcrumb_rect = _single(oca_window, LOCALE_CODE_LABEL, "Text")
    file_menu_rect = _single(oca_window, FILE_MENU_LABEL, "Button")

    assert (
        file_menu_rect.top < breadcrumb_rect.bottom
        and file_menu_rect.bottom > breadcrumb_rect.top
    ), (
        f"File menu button (y={file_menu_rect.top}-{file_menu_rect.bottom}) doesn't share a "
        f"horizontal band with the breadcrumb row's locale label (y={breadcrumb_rect.top}-"
        f"{breadcrumb_rect.bottom}) — the menu bar may have split back into its own row below "
        "the titlebar"
    )

