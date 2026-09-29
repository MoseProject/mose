# Retired pywinauto/UI Automation e2e tests

These 2 files drove `crates/ui`'s egui shell through Windows UI Automation (pywinauto). They
are excluded from collection (`e2e/pytest.ini`'s `--ignore=legacy_pywinauto`). Unlike this
directory's original ten, they are **not** blocked on tooling or on a missing desktop feature --
the WebDriver harness works and is proven against 12 real, passing tests in the parent directory.
Both remaining files instead test an egui immediate-mode implementation detail with no React/CSS
equivalent to port against:

- **`test_timeline_viewport_culling.py`** -- verifies a clip drag survives crossing egui's
  per-frame viewport-culling boundary (`ui.interact()` no longer called once culled,
  specifically guarded against for the dragged clip's id). The desktop timeline canvas has its
  own horizontal culling (skips drawing off-screen clips) but drag continuation is a browser
  pointer-capture concern, a different bug class entirely. Needs a test designed against
  `apps/desktop/src/editor/Timeline.tsx`'s actual drag handling, not a port of this one.
- **`test_editor_layout_geometry.py`** -- regression-tests three specific historical egui/CSS-
  equivalent layout bugs (a `ui.horizontal` reporting max child height, `ScrollArea`'s
  `auto_shrink` defaults, a menu bar rendering as a separate row). All three are properties of
  egui's immediate-mode layout engine; desktop uses real CSS grid/flexbox (see
  `apps/desktop/src/theme.css`) and already has its own panel-layout persistence feature
  (gap 6). None of the three bugs this file guards against can occur in a CSS layout at all.

`test_layer_transform.py`, `test_speed_ramp.py` and `test_properties_panel_disclosure.py`
(previously listed here) are now ported and passing: on-canvas drag/resize, the speed-ramp
presets, and the inspector's collapsible property sections all shipped as real desktop features,
so each moved to the parent directory instead of staying quarantined.

Porting either of the remaining 2 needs a bespoke test written against the real React/canvas
implementation, not a mechanical rewrite of what's here.
