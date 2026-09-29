# Smoke coverage for the undo/redo wiring (spec/ROADMAP.md P0 item 1, spec/architecture/
# undo-redo.md) -- drives the real invoke("add_track")/invoke("undo")/invoke("redo") commands
# rather than App methods directly, so it exercises the actual command dispatch and
# canUndo/canRedo flags SessionView reports, not just the underlying core::undo::UndoStack
# (already unit tested in crates/core/tests/undo_test.rs).
#
# Uses "add a video track" as the mutating action under test, same choice the retired
# pywinauto version made: it is a single, unambiguous undo/redo step with an easy-to-check
# side effect (the tracks array's length), unlike a timeline clip drag.
from __future__ import annotations

from helpers import get_session, invoke


def _track_count(session):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return len(sequence["timeline"]["tracks"])


def test_undo_redo_round_trips_an_added_video_track(oca_driver):
    before = get_session(oca_driver)
    assert not before["canUndo"]
    tracks_before = _track_count(before)

    invoke(oca_driver, "add_track", {"kind": "Video"})
    after_add = get_session(oca_driver)
    assert _track_count(after_add) == tracks_before + 1
    assert after_add["canUndo"]
    assert not after_add["canRedo"]

    invoke(oca_driver, "undo")
    after_undo = get_session(oca_driver)
    assert _track_count(after_undo) == tracks_before
    assert not after_undo["canUndo"]
    assert after_undo["canRedo"]

    invoke(oca_driver, "redo")
    after_redo = get_session(oca_driver)
    assert _track_count(after_redo) == tracks_before + 1
    assert after_redo["canUndo"]
    assert not after_redo["canRedo"]
