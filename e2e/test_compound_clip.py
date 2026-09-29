# Regression coverage for create_compound_clip (apps/desktop/src-tauri/src/commands/
# timeline.rs, delegating to avcore::Project::create_compound_clip -- a port of
# crates/ui/src/app/timeline_ops.rs's create_compound_clip_from_selected_clip). Wrapping a clip
# should replace it in place with a nested-sequence clip of the same id/position/duration, and
# the nested sequence should actually preview/export (crates/core/src/nested_sequence.rs's
# resolved_asset_id fix, verified separately at the unit level).
from __future__ import annotations

from helpers import get_session, import_and_add_to_timeline, invoke


def _track(session, track_id):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return next(t for t in sequence["timeline"]["tracks"] if t["id"] == track_id)


def test_create_compound_clip_wraps_the_selected_clip_in_a_nested_sequence(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    session = get_session(oca_driver)
    sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
    track_id = sequence["timeline"]["tracks"][0]["id"]
    sequences_before = len(session["project"]["sequences"])

    clip_before = _track(session, track_id)["clips"][0]

    new_sequence_id = invoke(oca_driver, "create_compound_clip", {
        "clipId": clip_id,
        "selectedClipIds": [clip_id],
    })
    assert new_sequence_id is not None, "should create a nested sequence"

    session = get_session(oca_driver)
    assert len(session["project"]["sequences"]) == sequences_before + 1

    track = _track(session, track_id)
    assert len(track["clips"]) == 1
    wrapper = track["clips"][0]
    assert wrapper["id"] == clip_id, "wrapper reuses the original clip's own id"
    assert wrapper["nested_sequence_id"] == new_sequence_id
    assert wrapper["start_secs"] == clip_before["start_secs"]

    nested = next(s for s in session["project"]["sequences"] if s["id"] == new_sequence_id)
    inner_clips = nested["timeline"]["tracks"][0]["clips"]
    assert len(inner_clips) == 1
    assert inner_clips[0]["id"] == clip_id
    assert inner_clips[0]["start_secs"] == 0.0, "rebased to the nested track's own 0.0"

    assert session["canUndo"]
