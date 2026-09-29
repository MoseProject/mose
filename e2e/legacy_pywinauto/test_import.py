# Regression coverage for the probe-then-enrich import flow (see CLAUDE.md's status
# paragraph): an imported asset must show up in the library right after the cheap probe, not
# only once loudness measurement + proxy generation (both full decode passes) finish too.
from __future__ import annotations

from pathlib import Path

from pywinauto import Desktop

from conftest import poll_for_descendants

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_VIDEO = REPO_ROOT / "crates" / "avbridge" / "tests" / "fixtures" / "video.mp4"

# Control ID of the file-name edit box in the modern (Vista-style) common Open dialog — stable
# across Windows versions and OS locales, unlike the dialog/button text.
OPEN_DIALOG_FILENAME_CONTROL_ID = 1148


def test_importing_a_file_adds_it_to_the_library_quickly(oca_window):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    oca_window.child_window(title="Mídia", control_type="Button").click_input()
    oca_window.child_window(title="⭱ Importar arquivos", control_type="Button").click_input()

    dialog = Desktop(backend="uia").window(class_name="#32770")
    dialog.wait("visible", timeout=10)
    filename_edit = dialog.child_window(
        control_id=OPEN_DIALOG_FILENAME_CONTROL_ID, class_name="Edit"
    )
    filename_edit.set_edit_text(str(FIXTURE_VIDEO))
    filename_edit.type_keys("{ENTER}")

    # Probing alone (no loudness/proxy passes) is metadata-only and should be near-instant —
    # generous timeout to absorb CI/dialog-teardown jitter without masking a regression back to
    # "wait for the full decode passes first".
    matches = poll_for_descendants(oca_window, FIXTURE_VIDEO.name, timeout_secs=5.0)
    assert matches, f"{FIXTURE_VIDEO.name!r} did not appear in the library within 5s of import"
