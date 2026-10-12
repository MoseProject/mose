// Drives the real frontend (Vite dev server) in headless Chromium against a mocked Tauri backend
// and reports how long scrolling and zooming a big timeline take, plus where the time goes.
//
//   cd apps/desktop && npx vite --port 5173 &
//   OCA_DUMP_SESSION=/tmp/session.json OCA_DUMP_TRACKS=30 OCA_DUMP_CLIPS=100 \
//     cargo test -p desktop dump_session_json -- --ignored
//   node tools/ui-perf.mjs /tmp/session.json
//
// Needs `playwright` resolvable (PLAYWRIGHT_MODULE overrides the import path) and a Chromium
// (CHROMIUM_PATH, default: the one Playwright installed). The Vite dev build runs React in its
// development mode, so absolute numbers are higher than the shipped app's; compare before/after.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
import { readFileSync } from "node:fs";
const session = JSON.parse(readFileSync(process.argv[2], "utf8"));
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
  args: ["--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
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
      switch (cmd) {
        case "get_session":
          return session;
        case "create_project":
          return true;
        case "plugin:event|listen":
          return 1;
        case "get_prefs":
          return {
            locale: "pt-BR",
            lufsProfile: -14,
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
const t0 = Date.now();
await page.goto("http://localhost:5173/");
await page.waitForTimeout(1500);
await page
  .getByRole("button", { name: /Novo projeto/ })
  .first()
  .click();
await page.waitForTimeout(400);
await page.getByRole("button", { name: /Criar projeto/ }).click();
await page.waitForTimeout(2500);
const clips = session.project.sequences[0].timeline.tracks.reduce((n, t) => n + t.clips.length, 0);
console.log("clips:", clips, "tracks:", session.project.sequences[0].timeline.tracks.length);
const box = await page.locator("canvas").first().boundingBox();
await page.mouse.move(box.x + 600, box.y + 100);
const cdp = await page.context().newCDPSession(page);
await cdp.send("Profiler.enable");
await cdp.send("Profiler.setSamplingInterval", { interval: 200 });
await cdp.send("Profiler.start");
// Frame cost while scrolling and zooming: the time of each requestAnimationFrame tick.
const result = await page.evaluate(async () => {
  const canvas = document.querySelector("canvas");
  const rect = canvas.getBoundingClientRect();
  const frames = [];
  const measure = async (label, make, count) => {
    const times = [];
    for (let i = 0; i < count; i++) {
      const start = performance.now();
      canvas.dispatchEvent(make(i));
      await new Promise((r) => requestAnimationFrame(() => r()));
      await new Promise((r) => requestAnimationFrame(() => r()));
      times.push(performance.now() - start);
    }
    times.sort((a, b) => a - b);
    return {
      label,
      p50: times[Math.floor(times.length / 2)],
      p95: times[Math.floor(times.length * 0.95)],
      max: times[times.length - 1],
    };
  };
  const wheel = (opts) => (i) =>
    new WheelEvent("wheel", {
      bubbles: true,
      cancelable: true,
      clientX: rect.x + 600,
      clientY: rect.y + 100,
      ...opts(i),
    });
  frames.push(
    await measure(
      "scroll (shift+wheel)",
      wheel((i) => ({ deltaY: 120, shiftKey: true })),
      60,
    ),
  );
  frames.push(
    await measure(
      "zoom (ctrl+wheel)",
      wheel((i) => ({ deltaY: i % 2 ? -100 : 100, ctrlKey: true })),
      60,
    ),
  );
  frames.push(
    await measure(
      "vertical wheel",
      wheel(() => ({ deltaY: 40 })),
      40,
    ),
  );
  return frames;
});
for (const r of result)
  console.log(
    `${r.label.padEnd(24)} p50 ${r.p50.toFixed(1)} ms  p95 ${r.p95.toFixed(1)} ms  max ${r.max.toFixed(1)} ms`,
  );
const { profile } = await cdp.send("Profiler.stop");
const self = new Map();
const byId = new Map(profile.nodes.map((n) => [n.id, n]));
const dt = profile.timeDeltas;
profile.samples.forEach((id, i) => {
  const n = byId.get(id);
  const key = `${n.callFrame.functionName || "(anon)"} ${n.callFrame.url.split("/").slice(-1)[0].split("?")[0]}:${n.callFrame.lineNumber}`;
  self.set(key, (self.get(key) ?? 0) + (dt[i] ?? 0));
});
const total = [...self.values()].reduce((a, b) => a + b, 0);
const children = new Map(profile.nodes.map((n) => [n.id, n.children ?? []]));
const selfById = new Map();
profile.samples.forEach((id, i) => selfById.set(id, (selfById.get(id) ?? 0) + (dt[i] ?? 0)));
const incl = new Map();
const walk = (id, stack) => {
  const n = byId.get(id);
  const name = n.callFrame.functionName;
  let total = selfById.get(id) ?? 0;
  for (const c of children.get(id)) total += walk(c, stack);
  if (/^[A-Z]/.test(name) && !name.startsWith("React"))
    incl.set(name, (incl.get(name) ?? 0) + total);
  return total;
};
walk(profile.nodes[0].id, []);
console.log("--- inclusive time by component, top 14");
[...incl.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 14)
  .forEach(([k, v]) => console.log((v / 1000).toFixed(0).padStart(6), "ms", k));
console.log("--- self time, top 14 (of", (total / 1000).toFixed(0), "ms)");
[...self.entries()]
  .sort((a, b) => b[1] - a[1])
  .slice(0, 14)
  .forEach(([k, v]) => console.log((v / 1000).toFixed(0).padStart(6), "ms", k));
await browser.close();
