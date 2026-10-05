# Shared helpers for driving the desktop shell's real Tauri IPC bridge directly from e2e tests,
# bypassing native OS dialogs (open/save file pickers) entirely -- WebDriver has no way to
# automate those anyway, since they live outside the WebView2 DOM. `window.__TAURI_INTERNALS__`
# is always present in the webview regardless of `app.withGlobalTauri` (that setting only
# controls the friendlier `window.__TAURI__.core.invoke` wrapper); every command `apps/desktop/
# src/ipc.ts` exposes is reachable this way with the same camelCase-arg-to-snake_case-param
# mapping ipc.ts itself relies on.
from __future__ import annotations

from pathlib import Path

from selenium.webdriver.common.by import By

from conftest import poll_until

REPO_ROOT = Path(__file__).resolve().parents[1]
FIXTURE_VIDEO = REPO_ROOT / "crates" / "avbridge" / "tests" / "fixtures" / "video.mp4"

# apps/desktop/src/store.ts's DEFAULT_UI.pxPerSec and timelineGeometry.ts's constants -- the
# canvas has no per-clip DOM elements to select, so a click needs the same pixel math the
# timeline itself uses.
PX_PER_SEC = 40
RULER_HEIGHT = 24
TRACK_HEIGHT = 56


def invoke(driver, command, args=None):
    """Calls a Tauri command by name, exactly as apps/desktop/src/ipc.ts's `api` object does,
    and returns its resolved value (or raises on rejection)."""
    status, value = driver.execute_async_script(
        "const [cmd, args, cb] = arguments;"
        "window.__TAURI_INTERNALS__.invoke(cmd, args || {})"
        ".then(r => cb(['ok', r])).catch(e => cb(['err', String(e)]));",
        command,
        args,
    )
    if status == "err":
        raise RuntimeError(f"invoke({command!r}) failed: {value}")
    return value


def get_session(driver):
    return invoke(driver, "get_session")


def import_and_add_to_timeline(driver, video_path=FIXTURE_VIDEO, track_kind="Video"):
    """Imports `video_path` (bypassing the native Open dialog via the `import_media` command
    directly) and adds the resulting asset to a new track. Returns the added clip's id."""
    invoke(driver, "import_media", {"paths": [str(video_path)]})

    def find_asset():
        session = get_session(driver)
        return next(
            (a for a in session["project"]["media_library"] if a["file_name"] == video_path.name),
            None,
        )

    asset = poll_until(find_asset, timeout_secs=10.0)
    assert asset, f"{video_path.name!r} never appeared in media_library after import_media"

    invoke(driver, "add_track", {"kind": track_kind})
    session = get_session(driver)
    track = next(t for t in _active_timeline(session)["tracks"] if t["kind"] == track_kind)
    invoke(driver, "add_asset_to_timeline", {"assetId": asset["id"], "trackId": track["id"]})

    session = get_session(driver)
    track = next(t for t in _active_timeline(session)["tracks"] if t["id"] == track["id"])
    clips = track["clips"]
    assert clips, "add_asset_to_timeline reported success but the track has no clips"
    return clips[0]["id"]


def _active_timeline(session):
    project = session["project"]
    sequence = project["sequences"][project["active_sequence"]]
    return sequence["timeline"]


def goto_editor(driver):
    """The Editor screen (and its timeline canvas) only mounts once the frontend's own screen
    state is "editor" -- a backend session existing (there always is one, from launch) isn't
    enough, since App.tsx renders Home until the user opens/creates a project. Clicking Home's
    "back to editor" button is the one screen-switch that works regardless of whether a project
    was ever explicitly created (a session already exists from app startup either way)."""
    if driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-editor"]'):
        return
    # Launch lands on Home, which has no topbar Home button to click; "back to editor" is offered
    # only for a session that is saved or has unsaved changes, so a clean one is edited and the
    # edit undone (the session stays marked as changed).
    if not driver.find_elements(By.CSS_SELECTOR, '[data-testid="back-to-editor"]'):
        invoke(driver, "add_track", {"kind": "Audio"})
        invoke(driver, "undo")
        poll_until(
            lambda: driver.find_elements(By.CSS_SELECTOR, '[data-testid="back-to-editor"]'),
            timeout_secs=3.0,
        )
    driver.find_element(By.CSS_SELECTOR, '[data-testid="back-to-editor"]').click()
    found = poll_until(
        lambda: driver.find_elements(By.CSS_SELECTOR, '[data-testid="screen-editor"]'),
        timeout_secs=3.0,
    )
    assert found, "editor screen never appeared after clicking back-to-editor"


def select_the_clip(driver, clip_id, track_index=0):
    """Selects `clip_id` by clicking its canvas-space body -- the timeline is a hand-painted
    <canvas> with no per-clip DOM element, so this needs the same start_secs/duration_secs ->
    pixel math timelineGeometry.ts uses, not a selector. Assumes the default zoom/scroll (a
    fresh session never changes pxPerSec/scrollSecs) and that `clip_id` starts at t=0, true for
    every clip this helper module itself adds."""
    goto_editor(driver)
    session = get_session(driver)
    clip = next(
        c for t in _active_timeline(session)["tracks"] for c in t["clips"] if c["id"] == clip_id
    )
    duration_secs = clip["source_out_secs"] - clip["source_in_secs"]
    x = int((clip["start_secs"] + duration_secs / 2) * PX_PER_SEC)
    y = RULER_HEIGHT + track_index * TRACK_HEIGHT + TRACK_HEIGHT // 2

    canvas = driver.find_element(By.CSS_SELECTOR, '[data-testid="timeline-canvas"]')
    from selenium.webdriver.common.action_chains import ActionChains

    # W3C Actions (what a real WebDriver session uses) place (0, 0) at the element's *center*,
    # not its top-left corner -- timelineGeometry.ts's math above is all top-left-relative.
    size = canvas.size
    center_dx, center_dy = x - size["width"] // 2, y - size["height"] // 2
    ActionChains(driver).move_to_element_with_offset(canvas, center_dx, center_dy).click().perform()

    found = poll_until(
        lambda: driver.find_elements(By.CSS_SELECTOR, '[data-testid="inspector-clip"]'),
        timeout_secs=3.0,
    )
    assert found, f"selecting clip {clip_id} at canvas ({x}, {y}) didn't show the inspector"
