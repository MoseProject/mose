# Regression coverage for the export queue (apps/desktop/src-tauri/src/export.rs,
# commands/export.rs) -- starting a job and watching it move from Queued through Rendering to
# Done, driven through the real start_export/list_exports commands rather than calling
# ExportQueue methods directly. Bypasses the native Save dialog via start_export's explicit
# path argument (see helpers.py's module docstring on why -- WebDriver can't reach it anyway).
#
# Uses the same 1-second fixture video the other e2e tests use, so the actual FFmpeg render
# behind the job finishes in well under the test's timeout -- this is not a mocked/faked
# completion, it is a real (tiny) render.
from __future__ import annotations

import time

from helpers import FIXTURE_VIDEO, import_and_add_to_timeline, invoke


def test_export_job_runs_from_queued_to_done(oca_driver, tmp_path):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"
    output_path = tmp_path / "e2e_export_output.mp4"

    import_and_add_to_timeline(oca_driver)

    job_id = invoke(oca_driver, "start_export", {"path": str(output_path), "rename": False})

    def job_status():
        jobs = invoke(oca_driver, "list_exports")
        return next((j for j in jobs if j["id"] == job_id), None)

    # Bounded poll for completion instead of a fixed sleep -- the render itself is real
    # (ExportQueue dispatches an actual FFmpeg worker thread), just against a 1-second source
    # so it finishes quickly.
    deadline = time.monotonic() + 30.0
    status = None
    while time.monotonic() < deadline:
        job = job_status()
        assert job is not None, f"export job {job_id} disappeared from list_exports"
        status = job["status"]
        if status == "Done" or (isinstance(status, dict) and "Failed" in status):
            break
        time.sleep(0.3)

    assert status == "Done", f"export job did not reach Done within 30s (last status: {status})"
    assert output_path.exists(), "export job reported Done but produced no output file"
    assert output_path.stat().st_size > 0, "export job produced an empty output file"
