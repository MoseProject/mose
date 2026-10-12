import { defineConfig } from "@playwright/test";

// Visual and layout tests: the real frontend (Vite) in headless Chromium against a mocked Tauri
// backend (`visual/backend.ts`). `npm run test:visual` compares screenshots with the baselines in
// `visual/__screenshots__`; `npm run test:visual:update` rewrites them after an intended change.
// Baselines are rendered by Chromium on Linux: run them there (or in a container), not on another OS.
export default defineConfig({
  testDir: "visual",
  testMatch: "**/*.visual.ts",
  snapshotPathTemplate: "{testDir}/__screenshots__/{testFileName}/{arg}{ext}",
  fullyParallel: true,
  retries: 0,
  reporter: [["list"]],
  expect: {
    toHaveScreenshot: { animations: "disabled", maxDiffPixelRatio: 0.002 },
  },
  use: {
    baseURL: "http://localhost:5173",
    viewport: { width: 1600, height: 900 },
    deviceScaleFactor: 1,
    locale: "pt-BR",
    colorScheme: "dark",
    launchOptions: {
      executablePath: process.env.CHROMIUM_PATH || undefined,
      args: ["--no-sandbox"],
    },
    // Screenshots are taken without motion; `mockBackend` lets a test ask for it.
  },
  webServer: {
    command: "npx vite --port 5173 --strictPort",
    url: "http://localhost:5173",
    reuseExistingServer: true,
    timeout: 60_000,
  },
});
