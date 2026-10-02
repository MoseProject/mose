# Regression coverage for insert_sequence_as_compound_clip (apps/desktop/src-tauri/src/
# commands/timeline.rs, delegating to avcore::Project::insert_sequence_as_compound_clip -- a
# port of crates/ui/src/app/timeline_ops.rs's insert_sequence_as_compound_clip). Inserting an
# existing sequence onto another timeline should append a nested-sequence clip spanning that
# sequence's own edited duration.
from __future__ import annotations

from helpers import get_session, import_and_add_to_timeline, invoke


def test_insert_sequence_as_compound_clip_appends_a_nested_clip(oca_driver):
    import_and_add_to_timeline(oca_driver)
    session = get_session(oca_driver)
    nested_sequence_id = session["project"]["sequences"][0]["id"]

    invoke(oca_driver, "add_sequence", {"name": "Second"})
    session = get_session(oca_driver)
    active_sequence_id = session["project"]["sequences"][session["project"]["active_sequence"]]["id"]
    assert active_sequence_id != nested_sequence_id, "add_sequence should switch to the new tab"

    inserted = invoke(oca_driver, "insert_sequence_as_compound_clip", {
        "sequenceId": nested_sequence_id,
    })
    assert inserted is True

    session = get_session(oca_driver)
    active = session["project"]["sequences"][session["project"]["active_sequence"]]
    video_track = next(t for t in active["timeline"]["tracks"] if t["kind"] == "Video")
    assert len(video_track["clips"]) == 1
    clip = video_track["clips"][0]
    assert clip["nested_sequence_id"] == nested_sequence_id
    assert clip["start_secs"] == 0.0

    assert session["canUndo"]


def test_insert_sequence_as_compound_clip_is_a_no_op_for_the_active_sequence(oca_driver):
    session = get_session(oca_driver)
    active_sequence_id = session["project"]["sequences"][session["project"]["active_sequence"]]["id"]

    inserted = invoke(oca_driver, "insert_sequence_as_compound_clip", {
        "sequenceId": active_sequence_id,
    })
    assert inserted is False
