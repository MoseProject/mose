# Retired pywinauto/UI Automation e2e tests

These 4 files drove `crates/ui`'s egui shell through Windows UI Automation (pywinauto). They
are excluded from collection (`e2e/pytest.ini`'s `--ignore=legacy_pywinauto`) and, unlike the
rest of this directory's original ten, they are **not** blocked on tooling -- the WebDriver
harness works and is proven against 10 real, passing tests in the parent directory
(`test_launch.py`, `test_navigation.py`, `test_import.py`, `test_undo_redo.py`,
`test_export.py`, `test_silence_review.py`, `test_clip_selection.py`, `test_layer_transform.py`,
and the skip-unless-model-present `test_auto_reframe.py`). Each of these 4 tests a feature or
an implementation detail the desktop shell genuinely doesn't have a counterpart for yet:

- **`test_speed_ramp.py`** -- applying a speed-ramp preset from the Clip menu. No
  `apply_speed_ramp`-shaped command exists in `apps/desktop/src-tauri/src/commands/`, and no
  menu wires to one; `types.ts`/`timelineGeometry.test.ts` reference the *type* but nothing
  exposes it in the desktop UI yet.
- **`test_properties_panel_disclosure.py`** -- collapsible/collapsed-by-default property
  sections (egui's `CollapsingHeader`). Desktop's `Inspector.tsx` renders every control flat,
  always expanded; gap 3's own notes list "per-effect enable indicators the properties panel's
  collapsible sections give egui" as still missing in desktop. There is nothing to click.
- **`test_timeline_viewport_culling.py`** -- verifies a clip drag survives crossing egui's
  per-frame viewport-culling boundary (`ui.interact()` no longer called once culled,
  specifically guarded against for the dragged clip's id). This is an egui immediate-mode
  implementation detail with no React equivalent: the desktop timeline canvas has its own
  horizontal culling (skips drawing off-screen clips) but drag continuation is a browser
  pointer-capture concern, a different bug class entirely. Needs a test designed against
  `apps/desktop/src/editor/Timeline.tsx`'s actual drag handling, not a port of this one.
- **`test_editor_layout_geometry.py`** -- regression-tests three specific historical egui/CSS-
  equivalent layout bugs (a `ui.horizontal` reporting max child height, `ScrollArea`'s
  `auto_shrink` defaults, a menu bar rendering as a separate row). All three are properties of
  egui's immediate-mode layout engine; desktop uses real CSS grid/flexbox (see
  `apps/desktop/src/theme.css`) and already has its own panel-layout persistence feature
  (gap 6). None of the three bugs this file guards against can occur in a CSS layout at all.

`test_layer_transform.py` (on-canvas position/resize drag, previously listed here) is now
ported and passing: `PreviewPanel.tsx` implements the missing feature this test needed, so it
moved to the parent directory instead of staying quarantined.

Porting any of the remaining 4 needs either new desktop feature work (speed_ramp,
properties_panel_disclosure) or a bespoke test written against the real React/canvas
implementation (viewport_culling, editor_layout_geometry) -- not a mechanical rewrite of what's
here.
