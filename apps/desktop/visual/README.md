# Visual and layout tests

The real frontend (Vite dev server) in headless Chromium, with the Tauri bridge replaced by
`backend.ts`: every command answers from `fixtures/session.json` and is recorded, so a test can
check what the UI asked the backend to do. Four kinds of check:

- **Screenshots** (`toHaveScreenshot`) of the export dialog in each state and of the clip menu,
  against the baselines in `__screenshots__/`. They catch any pixel change in those views.
- **Measures** (`layout.visual.ts`): computed styles that must not drift, such as the 1px focus
  ring that takes the place of a field's border, text fields looking like dropdowns, the dialog's
  width, the tooltip having no border.
- **Motion** (`motion.visual.ts`): fade in on open, fade out on cancel and Escape, the flight
  to the queue button on export, the tooltip fade, and that reduced motion turns all of it off.
- **Behavior in a real layout** (`export-dialog.visual.ts`, `clip-export.visual.ts`): which options
  are offered, what is disabled and when, what is sent to the backend.

## Running

```bash
cd apps/desktop
npm run test:visual            # compare with the baselines
npm run test:visual:update     # after an intended visual change: rewrite them, review the diff
```

`make test-visual` does the same. Playwright starts `vite` on port 5173 when it is not running.
It needs a Chromium: `PLAYWRIGHT_BROWSERS_PATH` pointing at a Playwright install, or
`CHROMIUM_PATH`. The baselines are rendered by Chromium on Linux, so run these tests there (or in
a container); fonts and anti-aliasing on another OS differ by enough to fail them.

When a screenshot test fails, `test-results/<test>/` holds the actual image and a diff.

## The fixture

`fixtures/session.json` is a session dumped from the Rust side, so it always has the shape the
backend really sends:

```bash
OCA_DUMP_SESSION=$PWD/apps/desktop/visual/fixtures/session.json OCA_DUMP_TRACKS=2 OCA_DUMP_CLIPS=6 \
  cargo test -p desktop dump_session_json -- --ignored
```

Regenerate it, then update the baselines, when a field of `SessionView` changes.
