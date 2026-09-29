# Regression coverage for auto-reframe (crates/core/src/auto_reframe.rs +
# crates/ui/src/app/auto_reframe.rs, request.md's "Reenquadramento automático") — the
# properties panel's "Reenquadramento automático" button should run face detection against the
# selected clip and apply a crop without crashing the app, end to end through the real ONNX
# Runtime (via the `ort` crate) rather than just the pure-geometry unit tests.
#
# The fixture video (crates/avbridge/tests/fixtures/video.mp4, 320x240 synthetic test pattern)
# has no real face in it, so the expected outcome is the documented fallback: a centered crop
# plus the AutoReframeNoSubjectFound toast (i18n.rs) — that toast appearing is exactly the
# end-to-end signal this test wants, since it can only fire after the model successfully loaded
# and ran a real inference pass.
#
# Requires the UltraFace ONNX model from a release bundle (or an explicit OCA_RESOURCE_DIR) —
# skipped when running against a source-tree build whose resources have not been assembled.
from __future__ import annotations

import os
import time
from pathlib import Path

import pytest

from conftest import poll_for_descendants
from test_layer_transform import (
    FIXTURE_VIDEO,
    _import_and_add_to_timeline,
    _select_the_clip,
)

RESOURCE_DIR = Path(os.environ.get("OCA_RESOURCE_DIR", "target/debug/resources"))
REFRAME_MODEL_PATH = RESOURCE_DIR / "models" / "version-RFB-320_simplified.onnx"


def _set_reframe_model_path(oca_window):
    oca_window.child_window(title="Ajustes", control_type="Button").click_input()
    time.sleep(0.5)
    # output_folder, whisper_model_path, sound_library_path, reframe_model_path — in that
    # fixed source order (prefs.rs), and no other Edit controls precede them on this screen.
    edits = oca_window.descendants(control_type="Edit")
    assert len(edits) >= 4, f"expected at least 4 Edit controls on Ajustes, found {len(edits)}"
    reframe_edit = edits[3]
    reframe_edit.set_edit_text(str(REFRAME_MODEL_PATH))
    time.sleep(0.2)


@pytest.mark.skipif(
    not REFRAME_MODEL_PATH.exists(),
    reason=f"auto-reframe model not bundled at {REFRAME_MODEL_PATH}",
)
def test_auto_reframe_button_runs_detection_without_crashing(oca_window):
    _set_reframe_model_path(oca_window)

    oca_window.child_window(title="Editor", control_type="Button").click_input()
    _import_and_add_to_timeline(oca_window)
    _select_the_clip(oca_window)

    reframe_button = oca_window.child_window(
        title="Reenquadramento automático", control_type="Button"
    )
    assert reframe_button.exists(), "auto-reframe button not found in the properties panel"
    reframe_button.click_input()

    # Real ONNX inference (model load + a 320x240 decode + face-detection forward pass) — more
    # generous than a pure-UI assertion's timeout.
    matches = poll_for_descendants(
        oca_window,
        "Nenhum rosto detectado — recorte centralizado aplicado.",
        timeout_secs=20.0,
    )
    assert matches, (
        "auto-reframe's 'no subject found' toast never appeared — either detection didn't "
        "run, or it crashed silently"
    )
    assert oca_window.exists()  # still alive: no panic
