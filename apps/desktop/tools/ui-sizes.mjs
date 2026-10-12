// Screenshots at 1280x720, 1920x1080 and 2560x1440, and a list of elements whose text is clipped.
//
//   node tools/ui-sizes.mjs /tmp/session.json   (setup as in ui-perf.mjs)
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
import { readFileSync, mkdirSync } from "node:fs";
const session = JSON.parse(readFileSync(process.argv[2], "utf8"));
const label = process.argv[3] ?? "full";
const locale = process.argv[4] ?? "pt_br";
mkdirSync("shots/sizes", { recursive: true });
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
  args: ["--no-sandbox"],
});
for (const [w, h] of [
  [1280, 720],
  [1920, 1080],
  [2560, 1440],
]) {
  const page = await browser.newPage({ viewport: { width: w, height: h } });
  await page.addInitScript(
    ({ session, locale }) => {
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
                    locale,
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
    },
    { session, locale },
  );
  await page.goto("http://localhost:5173/");
  await page.waitForTimeout(1000);
  await page
    .getByRole("button", { name: /Novo projeto|New project/ })
    .first()
    .click();
  await page.waitForTimeout(300);
  await page.getByRole("button", { name: /Criar projeto|Create project/ }).click();
  await page.waitForSelector("canvas");
  await page.waitForTimeout(1200);
  await page.screenshot({ path: `shots/sizes/${label}-${w}.png` });
  // Anything wider than its box (clipped text) in the visible UI.
  const overflow = await page.evaluate(() =>
    [...document.querySelectorAll("button, label, span, p, h1, h2, h3, summary")]
      .filter(
        (el) =>
          el.scrollWidth > el.clientWidth + 1 &&
          el.clientWidth > 0 &&
          getComputedStyle(el).overflow !== "visible" &&
          getComputedStyle(el).textOverflow !== "ellipsis",
      )
      .slice(0, 8)
      .map(
        (el) =>
          `${el.tagName}.${(el.className + "").slice(0, 30)} "${el.textContent.trim().slice(0, 30)}" ${el.scrollWidth}>${el.clientWidth}`,
      ),
  );
  console.log(
    `${w}x${h}: clipped elements: ${overflow.length ? "\n  " + overflow.join("\n  ") : "none"}`,
  );
  await page.close();
}
await browser.close();
