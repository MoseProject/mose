# Regression coverage for auto-reframe (crates/core/src/auto_reframe.rs,
# apps/desktop/src-tauri/src/commands/auto_reframe.rs) -- running face detection against the
# selected clip and applying a crop, end to end through the real ONNX Runtime (via the `ort`
# crate) rather than just the pure-geometry unit tests.
#
# The fixture video (crates/avbridge/tests/fixtures/video.mp4, a 320x240 synthetic test
# pattern) has no real face in it, so the expected outcome is the documented fallback: a
# centered crop is written to the clip's crop_x/y/w/h regardless -- that changing is exactly
# the end-to-end signal this test wants, since it can only happen after the model successfully
# loaded and ran a real inference pass.
#
# Requires the UltraFace ONNX model from a release bundle (or an explicit OCA_RESOURCE_DIR) --
# skipped when running against a source-tree build whose resources have not been assembled.
from __future__ import annotations

import os
from pathlib import Path

import pytest

from helpers import FIXTURE_VIDEO, get_session, import_and_add_to_timeline, invoke
from conftest import poll_until

RESOURCE_DIR = Path(os.environ.get("OCA_RESOURCE_DIR", "target/debug/resources"))
REFRAME_MODEL_PATH = RESOURCE_DIR / "models" / "version-RFB-320_simplified.onnx"


def _clip(session, clip_id):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return next(
        c for t in sequence["timeline"]["tracks"] for c in t["clips"] if c["id"] == clip_id
    )


@pytest.mark.skipif(
    not REFRAME_MODEL_PATH.exists(),
    reason=f"auto-reframe model not bundled at {REFRAME_MODEL_PATH}",
)
def test_auto_reframe_button_runs_detection_without_crashing(oca_driver):
    prefs = invoke(oca_driver, "get_prefs")
    prefs["reframeModelPath"] = str(REFRAME_MODEL_PATH)
    invoke(oca_driver, "set_prefs", {"prefs": prefs})

    clip_id = import_and_add_to_timeline(oca_driver)
    before = _clip(get_session(oca_driver), clip_id)

    invoke(oca_driver, "auto_reframe_clip", {"clipId": clip_id})

    # Real ONNX inference (model load + a 320x240 decode + face-detection forward pass) -- more
    # generous than a pure-command-roundtrip timeout.
    def cropped():
        after = _clip(get_session(oca_driver), clip_id)
        return after if after["crop_w"] != before["crop_w"] or after["crop_x"] != before["crop_x"] else None

    after = poll_until(cropped, timeout_secs=20.0)
    assert after, "auto-reframe never changed the clip's crop -- detection didn't run, or it crashed silently"
    assert get_session(oca_driver) is not None  # still alive: no panic
