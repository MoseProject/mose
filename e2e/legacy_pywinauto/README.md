# Retired pywinauto/UI Automation e2e tests

These 10 files drove `crates/ui`'s egui shell through Windows UI Automation (pywinauto). They
are excluded from collection (`e2e/pytest.ini`'s `--ignore=legacy_pywinauto`) since:

- `pywinauto`/`pywin32` are no longer e2e dependencies (`e2e/requirements.txt` now installs
  `selenium` for the WebDriver/tauri-driver harness -- see `e2e/conftest.py`).
- egui had no DOM; these tests found controls by accessible name/role or, for hand-painted
  controls (`test_layer_transform.py`'s shared helpers), by computing pixel rectangles from
  egui's own immediate-mode layout. Neither approach carries over: the desktop shell is a real
  React DOM (CSS selectors, `data-testid` attributes) with a canvas-based timeline whose pixel
  math lives in `apps/desktop/src/timelineGeometry.ts`, not in these files' constants.
- Porting them needs a live oca-desktop.exe run to derive and verify real selectors/coordinates
  against the new UI, which isn't possible from this environment: `target/debug/oca-desktop.exe`
  currently builds cleanly but aborts on launch with a "Microsoft Visual C++ Runtime Library"
  dialog before any log line is written (no `crash_<unix>.txt`, no fresh `oca.log` entry) --
  likely a CRT-linkage conflict given `cargo build`'s `LNK4098 defaultlib conflicts` warnings for
  `desktop`, not something safe to guess-fix without a debugger attached.

`test_launch.py` and `test_navigation.py` in the parent directory are already ported to
Selenium/tauri-driver against real `data-testid` selectors and are a template for the rest.
Porting order should probably follow this list's dependency chain: `test_layer_transform.py`
first (the shared `_import_and_add_to_timeline`/`_select_the_clip`/`_zoom_timeline_in` helpers
five other files import), then `test_auto_reframe.py`, `test_editor_layout_geometry.py`,
`test_properties_panel_disclosure.py`, `test_speed_ramp.py`, `test_timeline_viewport_culling.py`
(all import from it), then the independent `test_export.py`, `test_import.py`,
`test_silence_review.py`, `test_undo_redo.py`.
