# Desktop frontend

The React webview frontend lives in `src/` and uses Vite for the Tauri development and production
builds. Storybook provides an isolated browser workspace for the reusable editor controls and
components.

```bash
npm install
npm run storybook         # interactive component workshop at http://localhost:6006
npm run build-storybook   # static Storybook output in storybook-static/
```

Add component stories under `stories/` with the `*.stories.tsx` suffix. Storybook loads the base
CSS Module and the modules for each component area through the `cx` helper, so stories use the same
palette, typography, and scoped styles as the desktop editor. Components that call the native Tauri
bridge need a browser-safe mock or an injectable adapter before they can be interacted with in a
story.
