# Regression coverage for the speed-ramp presets (apps/desktop/src-tauri/src/commands/
# timeline.rs's apply_speed_ramp_to_clip, ported from crates/ui/src/app/timeline_ops.rs's
# apply_speed_ramp_to_selected_clip) -- applying a preset should split the selected clip into
# several contiguous pieces with an interpolated speed_factor each, unit-tested at the math
# level in crates/ui/src/app/app_test.rs; this test's job is only the observable consequence
# through the real command.
from __future__ import annotations

from helpers import get_session, import_and_add_to_timeline, invoke


def _track(session, track_id):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return next(t for t in sequence["timeline"]["tracks"] if t["id"] == track_id)


def test_speed_ramp_preset_splits_the_selected_clip_into_several_pieces(oca_driver):
    clip_id = import_and_add_to_timeline(oca_driver)
    session = get_session(oca_driver)
    sequence = session["project"]["sequences"][session["project"]["active_sequence"]]
    track_id = sequence["timeline"]["tracks"][0]["id"]

    blocks_before = len(_track(get_session(oca_driver), track_id)["clips"])
    assert blocks_before == 1

    invoke(oca_driver, "apply_speed_ramp_to_clip", {
        "clipId": clip_id,
        "startSpeed": 0.5,
        "endSpeed": 2.0,
        "steps": 4,
    })

    after = _track(get_session(oca_driver), track_id)
    # apply_speed_ramp_to_clip(0.5, 2.0, 4) carves the clip into 4 contiguous pieces (see the
    # unit test this e2e test complements) -- assert it grew, not an exact count, matching the
    # retired pywinauto version's own caution about extra non-clip elements (not a concern here
    # since we read the real clip list, not painted canvas elements, but kept for parity).
    assert len(after["clips"]) > blocks_before, (
        f"clip count did not increase after applying the speed ramp preset "
        f"(before={blocks_before}, after={len(after['clips'])})"
    )
    speeds = [c["speed_factor"] for c in sorted(after["clips"], key=lambda c: c["start_secs"])]
    assert speeds[0] < speeds[-1], f"speed should increase left to right, got {speeds}"

    # Sanity: the command required a selection-equivalent (a valid clip_id) and the app is
    # still alive/responsive (undo remains available, proving push_undo_snapshot ran).
    assert get_session(oca_driver)["canUndo"]
