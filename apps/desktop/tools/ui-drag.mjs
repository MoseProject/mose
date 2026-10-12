// Time per step of dragging, trimming, scrubbing, hovering and shift-selecting on the timeline of
// the real frontend, against a mocked backend (see ui-perf.mjs for the session dump and setup).
//
//   node tools/ui-drag.mjs /tmp/session.json
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
import { readFileSync } from "node:fs";
const session = JSON.parse(readFileSync(process.argv[2], "utf8"));
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
  args: ["--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
const calls = {};
await page.exposeFunction("__call", (c) => {
  calls[c] = (calls[c] ?? 0) + 1;
});
await page.addInitScript((session) => {
  let cb = 1;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback() {
      return cb++;
    },
    unregisterCallback() {},
    convertFileSrc: (p) => p,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    invoke: async (cmd) => {
      window.__call(cmd);
      switch (cmd) {
        case "get_session":
          return session;
        case "create_project":
          return true;
        case "plugin:event|listen":
          return 1;
        case "get_prefs":
          return {
            locale: "pt_br",
            lufsProfile: 0,
            outputFolder: "",
            autosaveMinutes: 5,
            previewHardwareDecode: false,
            recentProjectPaths: [],
            savedLayerTemplates: [],
            whisperModelPath: "",
            reframeModelPath: "",
            backgroundRemovalModelPath: "",
            ttsModelPath: "",
            soundLibraryPath: "",
            errorReportingConsent: "disabled",
            lastReviewedCrashUnix: 0,
            telemetryEnabled: false,
            keybindings: {},
          };
        case "get_keybindings":
          return {};
        default:
          return /^list_|^get_recent/.test(cmd) ? [] : null;
      }
    },
  };
}, session);
await page.goto("http://localhost:5173/");
await page.waitForTimeout(1200);
await page
  .getByRole("button", { name: /Novo projeto/ })
  .first()
  .click();
await page.waitForTimeout(300);
await page.getByRole("button", { name: /Criar projeto/ }).click();
await page.waitForSelector("canvas");
await page.waitForTimeout(1500);
const box = await page.locator("canvas").first().boundingBox();
const measure = async (label, run) => {
  const times = [];
  await run(async () => {
    const s = Date.now();
    await page.evaluate(() => new Promise((r) => requestAnimationFrame(r)));
    times.push(Date.now() - s);
  });
  times.sort((a, b) => a - b);
  console.log(
    `${label.padEnd(28)} step p50 ${times[Math.floor(times.length / 2)]} ms  p95 ${times[Math.floor(times.length * 0.95)]} ms  max ${times[times.length - 1]} ms  (${times.length} steps)`,
  );
};
const y0 = box.y + 62 + 56 * 3;
await measure("drag a clip sideways", async (tick) => {
  await page.mouse.move(box.x + 300, y0);
  await page.mouse.down();
  for (let i = 0; i < 80; i++) {
    await page.mouse.move(box.x + 300 + i * 5, y0 + (i % 20 > 10 ? 20 : 0));
    await tick();
  }
  await page.mouse.up();
});
await measure("trim a clip's edge", async (tick) => {
  await page.mouse.move(box.x + 440, y0 - 56);
  await page.mouse.down();
  for (let i = 0; i < 60; i++) {
    await page.mouse.move(box.x + 440 + i * 3, y0 - 56);
    await tick();
  }
  await page.mouse.up();
});
await measure("scrub the ruler", async (tick) => {
  await page.mouse.move(box.x + 300, box.y + 12);
  await page.mouse.down();
  for (let i = 0; i < 80; i++) {
    await page.mouse.move(box.x + 300 + i * 10, box.y + 12);
    await tick();
  }
  await page.mouse.up();
});
await measure("hover across the clips", async (tick) => {
  for (let i = 0; i < 80; i++) {
    await page.mouse.move(box.x + 260 + i * 13, box.y + 70 + (i % 10) * 40);
    await tick();
  }
});
await measure("select with shift-click", async (tick) => {
  for (let i = 0; i < 20; i++) {
    await page.keyboard.down("Shift");
    await page.mouse.click(box.x + 300 + i * 40, box.y + 62 + (i % 8) * 56);
    await page.keyboard.up("Shift");
    await tick();
  }
});
console.log("backend calls during the run:", JSON.stringify(calls));
await browser.close();
