# Regression coverage for the probe-then-enrich import flow (see CLAUDE.md's status
# paragraph): an imported asset must show up in the library right after the cheap probe, not
# only once loudness measurement + proxy generation (both full decode passes) finish too.
from __future__ import annotations

from helpers import FIXTURE_VIDEO, get_session, invoke
from conftest import poll_until


def test_importing_a_file_adds_it_to_the_library_quickly(oca_driver):
    assert FIXTURE_VIDEO.exists(), f"fixture missing: {FIXTURE_VIDEO}"

    invoke(oca_driver, "import_media", {"paths": [str(FIXTURE_VIDEO)]})

    # Probing alone (no loudness/proxy passes) is metadata-only and should be near-instant --
    # generous timeout to absorb CI jitter without masking a regression back to "wait for the
    # full decode passes first".
    def imported():
        session = get_session(oca_driver)
        return any(a["file_name"] == FIXTURE_VIDEO.name for a in session["project"]["media_library"])

    assert poll_until(imported, timeout_secs=5.0), (
        f"{FIXTURE_VIDEO.name!r} did not appear in media_library within 5s of import_media"
    )
