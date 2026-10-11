# The crash-recovery journal against the real app: a project that was never saved is kept on
# disk while it has unsaved changes, survives the process being killed, and is not touched by
# the next launch until the user picks it. The process is really killed (SIGKILL / taskkill).
from __future__ import annotations

from conftest import App, poll_until

PROJECT_MAGIC = b"OCPJ"
PROJECT_FORMAT_VERSION = 4


def snapshots(app):
    return sorted(app.recovery_root.glob("*/snapshot.ocproj")) if app.recovery_root.exists() else []


def test_a_clean_app_keeps_nothing(app):
    import time

    time.sleep(3)
    assert snapshots(app) == []


def test_a_never_saved_project_is_kept_on_disk_while_it_has_unsaved_changes(app):
    app.call("commands.run", {"id": "addMarker"})
    # The snapshot follows the edit about a second later.
    assert poll_until(lambda: snapshots(app), timeout_secs=30), app.log()
    data = snapshots(app)[0].read_bytes()
    assert data[:4] == PROJECT_MAGIC
    assert data[4] == PROJECT_FORMAT_VERSION


def test_a_later_edit_rewrites_the_same_snapshot_instead_of_adding_another(app):
    app.call("commands.run", {"id": "addMarker"})
    assert poll_until(lambda: snapshots(app), timeout_secs=30)
    app.call("commands.run", {"id": "addMarker"})
    first = snapshots(app)[0]
    before = first.stat().st_mtime_ns
    # A second edit rewrites the same snapshot rather than adding another.
    assert poll_until(lambda: first.stat().st_mtime_ns != before, timeout_secs=30)
    assert len(snapshots(app)) == 1


def test_the_snapshot_survives_the_process_being_killed(launch, home):
    first = launch()
    first.call("commands.run", {"id": "addMarker"})
    assert poll_until(lambda: snapshots(first), timeout_secs=30), first.log()
    left_behind = snapshots(first)[0]
    first.kill()
    assert left_behind.exists()

    second = launch(home)
    # The new launch has its own session directory and does not consume the old one.
    assert left_behind.exists()
    sessions = [path for path in second.recovery_root.iterdir() if path.is_dir()]
    assert len(sessions) >= 2
    assert second.session()["project"]["name"] == "Untitled"
    assert second.session()["dirty"] is False


def test_the_next_launch_removes_abandoned_empty_sessions(launch, home, app_exe):
    stale = App(app_exe, home).recovery_root / "1-1"
    stale.mkdir(parents=True)
    launch(home)
    # The home screen asks for the leftovers on mount, which clears directories with no snapshot.
    assert poll_until(lambda: not stale.exists(), timeout_secs=30)
