// Accessibility checks of the editor: controls without an accessible name, text under 4.5:1 contrast and Tab stops without a visible focus ring.
//
//   node tools/ui-a11y.mjs /tmp/session.json   (setup as in ui-perf.mjs)
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
import { readFileSync } from "node:fs";
const session = JSON.parse(readFileSync(process.argv[2], "utf8"));
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
  args: ["--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 860 } });
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
    invoke: async (cmd) =>
      cmd === "get_session"
        ? session
        : cmd === "create_project"
          ? true
          : cmd === "get_prefs"
            ? {
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
              }
            : cmd === "get_keybindings"
              ? {}
              : /^list_|^get_recent/.test(cmd)
                ? []
                : null,
  };
}, session);
await page.goto("http://localhost:5173/");
await page.waitForTimeout(1000);
await page
  .getByRole("button", { name: /Novo projeto/ })
  .first()
  .click();
await page.waitForTimeout(300);
await page.getByRole("button", { name: /Criar projeto/ }).click();
await page.waitForSelector("canvas");
await page.waitForTimeout(1000);
const box = await page.locator("canvas").first().boundingBox();
await page.mouse.click(box.x + 300, box.y + 62);
await page.waitForTimeout(500);
const report = await page.evaluate(() => {
  const name = (el) =>
    (el.getAttribute("aria-label") || el.getAttribute("title") || el.textContent || "").trim() ||
    (el.labels && [...el.labels].map((l) => l.textContent.trim()).join(" ")) ||
    el.getAttribute("aria-labelledby");
  const visible = (el) => {
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0 && getComputedStyle(el).visibility !== "hidden";
  };
  const unnamed = [
    ...document.querySelectorAll(
      "button, [role=button], input, select, textarea, [role=tab], [role=slider]",
    ),
  ]
    .filter(visible)
    .filter((el) => !name(el))
    .map(
      (el) =>
        `${el.tagName.toLowerCase()}${el.type ? "[" + el.type + "]" : ""}.${String(el.className).split(" ")[0]} in ${el.closest("section,aside,header,nav,div[role]")?.className?.toString().split(" ")[0] ?? "?"}`,
    );
  // Contrast of the text colours against the surfaces they sit on.
  const lum = (rgb) => {
    const [r, g, b] = rgb.map((v) => {
      v /= 255;
      return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
    });
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  };
  const parse = (c) =>
    c
      .match(/[\d.]+/g)
      .slice(0, 3)
      .map(Number);
  const bgOf = (el) => {
    for (let e = el; e; e = e.parentElement) {
      const c = getComputedStyle(e).backgroundColor;
      const m = c.match(/[\d.]+/g);
      if (m && (m.length < 4 || Number(m[3]) > 0.9)) return parse(c);
    }
    return [8, 14, 18];
  };
  const ratio = (a, b) => {
    const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
    return (x + 0.05) / (y + 0.05);
  };
  const low = [];
  for (const el of document.querySelectorAll("body *")) {
    if (!visible(el) || !el.childNodes.length) continue;
    const own = [...el.childNodes].some((n) => n.nodeType === 3 && n.textContent.trim());
    if (!own) continue;
    const cs = getComputedStyle(el);
    const fg = parse(cs.color);
    const r = ratio(fg, bgOf(el));
    const size = parseFloat(cs.fontSize);
    if (r < 4.5 && !(size >= 18.66 || (size >= 14 && Number(cs.fontWeight) >= 700)))
      low.push([r.toFixed(2), cs.color, size + "px", el.textContent.trim().slice(0, 28)]);
  }
  const worst = [...new Map(low.map((l) => [l[3], l])).values()]
    .sort((a, b) => a[0] - b[0])
    .slice(0, 12);
  return {
    unnamed,
    lowCount: low.length,
    worst,
    focusables: document.querySelectorAll(
      "button, input, select, textarea, [tabindex]:not([tabindex='-1']), a[href]",
    ).length,
  };
});
console.log(
  "controls without an accessible name:",
  report.unnamed.length ? "\n  " + report.unnamed.join("\n  ") : "none",
);
console.log(`text under 4.5:1 contrast: ${report.lowCount} elements; worst:`);
for (const w of report.worst) console.log("  ", w.join(" | "));
// Keyboard: Tab through 40 stops and see where focus is visible.
const stops = [];
await page.mouse.click(5, 500);
for (let i = 0; i < 45; i++) {
  await page.keyboard.press("Tab");
  stops.push(
    await page.evaluate(() => {
      const el = document.activeElement;
      if (!el || el === document.body) return "body";
      const cs = getComputedStyle(el);
      const outlined = cs.outlineStyle !== "none" && parseFloat(cs.outlineWidth) > 0;
      const shadow = cs.boxShadow !== "none";
      return `${el.tagName.toLowerCase()} "${(el.getAttribute("aria-label") || el.textContent || el.getAttribute("title") || "").trim().slice(0, 22)}" ${outlined || shadow ? "ok" : "NO-FOCUS-RING"}`;
    }),
  );
}
const noRing = stops.filter((s) => s.includes("NO-FOCUS-RING"));
console.log(`Tab stops: ${stops.length}, without a visible focus ring: ${noRing.length}`);
noRing.slice(0, 8).forEach((s) => console.log("  ", s));
await browser.close();
