# Smoke coverage for the toolbar/Ctrl+Z/Ctrl+Y undo-redo wiring (spec/ROADMAP.md P0 item 1,
# spec/architecture/undo-redo.md) — drives the real compiled ui.exe rather than App methods
# directly, so it exercises the actual key-binding dispatch in screens/editor/mod.rs and the
# toolbar button enabled-state wiring, not just the underlying core::undo::UndoStack (already
# unit tested in crates/core/tests/undo_test.rs and crates/ui/src/app/app_test.rs).
#
# Uses "add video track" as the mutating action under test rather than a timeline clip drag:
# the new track's name ("V1") is a plain accessible Text control (see conftest.py's module
# docstring on why timeline clips themselves aren't), so its presence/absence is a reliable,
# coordinate-free signal that undo/redo actually round-tripped through the live app.
#
# "Add video track" lives inside the top bar's "Sequência" (Sequence) menu_button (see
# screens/editor/menu_bar.rs's sequence_menu, added by e8f7eb8 "add the Editor's .../Sequence/...
# menu bar") -- an egui menu_button's children only exist in the accessibility tree while the
# menu popup is open, so the menu has to be clicked open first rather than reaching the item
# directly.
from __future__ import annotations

import time

from conftest import poll_for_descendants

ADD_VIDEO_TRACK_LABEL = "＋ Adicionar faixa de vídeo"
SEQUENCE_MENU_LABEL = "Sequência"


def _toolbar_button(oca_window, symbol):
    return oca_window.child_window(title=symbol, control_type="Button")


def _click_add_video_track(oca_window):
    oca_window.child_window(
        title=SEQUENCE_MENU_LABEL, control_type="Button"
    ).click_input()
    oca_window.child_window(
        title=ADD_VIDEO_TRACK_LABEL, control_type="Button"
    ).click_input()


def test_undo_redo_round_trips_an_added_video_track(oca_window):
    oca_window.child_window(title="Editor", control_type="Button").click_input()

    assert not poll_for_descendants(oca_window, "V1", timeout_secs=2.0)
    assert not _toolbar_button(oca_window, "Desfazer").is_enabled()

    _click_add_video_track(oca_window)
    assert poll_for_descendants(oca_window, "V1", timeout_secs=3.0)
    assert _toolbar_button(oca_window, "Desfazer").is_enabled()
    assert not _toolbar_button(oca_window, "Refazer").is_enabled()

    oca_window.type_keys("^z")
    # poll_for_descendants only polls for *presence*; absence needs its own settle+recheck.
    time.sleep(0.5)
    assert not oca_window.descendants(title="V1", control_type="Text")
    assert not _toolbar_button(oca_window, "Desfazer").is_enabled()
    assert _toolbar_button(oca_window, "Refazer").is_enabled()

    oca_window.type_keys("^y")
    assert poll_for_descendants(oca_window, "V1", timeout_secs=3.0)
    assert _toolbar_button(oca_window, "Desfazer").is_enabled()
    assert not _toolbar_button(oca_window, "Refazer").is_enabled()
