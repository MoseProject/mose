# Regression coverage for the export queue (crates/ui/src/screens/queue.rs, crates/ui/src/app/
# export.rs) -- adding a job from the Fila screen and watching it move from Queued through
# Rendering to Done, driven entirely through the live UI (menus, the native Save dialog, the
# job list's status tag) rather than calling App::queue_export directly.
#
# Uses the same 1-second fixture video the other e2e tests already use (video_layer_transform.py
# etc.) so the actual FFmpeg render behind the job finishes in well under the test's timeout --
# this is not a mocked/faked completion, it is a real (tiny) render, which is what keeps this
# fast without weakening the assertion the task called for ("observe it complete").
from __future__ import annotations

import time
from pathlib import Path

from pywinauto import Desktop

from conftest import poll_for_descendants
from test_layer_transform import _import_and_add_to_timeline

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_VIDEO = REPO_ROOT / "crates" / "avbridge" / "tests" / "fixtures" / "video.mp4"
SAVE_DIALOG_FILENAME_CONTROL_ID = 1148

ADD_EXPORT_LABEL = "＋ Adicionar exportação"
NAV_QUEUE_LABEL = "Fila"
STATUS_QUEUED = "Na fila"
STATUS_RENDERING = "Renderizando"
STATUS_DONE = "Concluído"


def _go_to_queue_screen(oca_window):
    oca_window.child_window(title=NAV_QUEUE_LABEL, control_type="Button").click_input()
    assert poll_for_descendants(oca_window, "Fila de exportação", timeout_secs=3.0)


def _add_export_job(oca_window, output_path: Path):
    """Clicks "+ Adicionar exportação" and fills in the native Save dialog with
    `output_path`. Retried like `_import_and_add_to_timeline`'s own Open-dialog click -- the
    same category of OS-level synthetic-input timing noise applies to any click that's
    supposed to spawn a native common-item dialog."""
    dialog = Desktop(backend="uia").window(class_name="#32770")
    for attempt in range(3):
        oca_window.child_window(
            title=ADD_EXPORT_LABEL, control_type="Button"
        ).click_input()
        try:
            dialog.wait("visible", timeout=5)
            break
        except Exception:
            if attempt == 2:
                raise
    filename_edit = dialog.child_window(
        control_id=SAVE_DIALOG_FILENAME_CONTROL_ID, class_name="Edit"
    )
    filename_edit.set_edit_text(str(output_path))
    filename_edit.type_keys("{ENTER}")


def test_export_job_runs_from_queued_to_done(oca_window, tmp_path):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"
    output_path = tmp_path / "e2e_export_output.mp4"

    _import_and_add_to_timeline(oca_window)

    _go_to_queue_screen(oca_window)
    _add_export_job(oca_window, output_path)

    # The job list only shows a status tag once the job is on screen -- poll rather than a
    # single snapshot, same reasoning as every other UI-Automation assertion in this suite
    # (the app only repaints every 200ms while idle).
    assert poll_for_descendants(
        oca_window, STATUS_QUEUED, timeout_secs=5.0
    ) or poll_for_descendants(oca_window, STATUS_RENDERING, timeout_secs=5.0), (
        "export job never appeared in the queue after filling in the Save dialog"
    )

    # Bounded poll for completion instead of a fixed sleep -- the render itself is real
    # (App::pump_export_queue dispatches an actual FFmpeg worker thread), just against a
    # 1-second source so it finishes quickly.
    deadline = time.monotonic() + 30.0
    done = False
    while time.monotonic() < deadline:
        if oca_window.descendants(title=STATUS_DONE, control_type="Text"):
            done = True
            break
        failed = oca_window.descendants(title="Falhou", control_type="Text")
        if failed:
            detail = oca_window.descendants(control_type="Text")
            detail_texts = [d.window_text() for d in detail]
            raise AssertionError(
                f"export job reported Failed instead of Done; visible texts: {detail_texts}"
            )
        time.sleep(0.3)

    assert done, "export job did not reach 'Concluído' (Done) status within 30s"
    assert output_path.exists(), "export job reported Done but produced no output file"
    assert output_path.stat().st_size > 0, "export job produced an empty output file"
