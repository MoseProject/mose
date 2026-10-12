// Screenshots of every screen and the main panels of the real frontend (Vite dev server) against a
// mocked Tauri backend, to look for layout bugs the unit tests cannot see.
//
//   node tools/ui-shots.mjs /tmp/session.json [pt-BR|en]     (see ui-perf.mjs for the session dump)
//
// Writes shots/<locale>/*.png under the current directory. PLAYWRIGHT_MODULE and CHROMIUM_PATH as
// in ui-perf.mjs.
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE ?? "playwright");
import { readFileSync } from "node:fs";
const session = JSON.parse(readFileSync(process.argv[2], "utf8"));
const locale = process.argv[3] ?? "pt-BR";
const dir = `shots/${locale}`;
import { mkdirSync } from "node:fs";
mkdirSync(dir, { recursive: true });
const browser = await chromium.launch({
  executablePath: process.env.CHROMIUM_PATH,
  args: ["--no-sandbox"],
});
const page = await browser.newPage({ viewport: { width: 1440, height: 860 } });
const errors = [];
page.on("pageerror", (e) => errors.push("pageerror: " + e.message));
page.on("console", (m) => {
  if (m.type() === "error" && !/Failed to load resource/.test(m.text()))
    errors.push("console: " + m.text().slice(0, 160));
});
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
              locale,
              lufsProfile: -14,
              outputFolder: "C:/Videos",
              autosaveMinutes: 5,
              previewHardwareDecode: false,
              recentProjectPaths: ["C:/proj/jogo1.ocproj", "C:/proj/jogo2.ocproj"],
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
          case "get_recent_projects":
            return [
              {
                path: "C:/proj/jogo1.ocproj",
                name: "Jogo 1",
                last_edited: { HoursAgo: 3 },
                summary: "12 clipes",
                exists: true,
              },
              {
                path: "C:/proj/jogo2.ocproj",
                name: "Jogo 2",
                last_edited: { HoursAgo: 30 },
                summary: "",
                exists: true,
              },
            ];
          case "get_keybindings":
            return {};
          case "get_gpu_encoder_preference":
            return "auto";
          case "get_normalize_audio":
            return true;
          case "list_exports":
            return [
              {
                id: 1,
                title: "Jogo 1",
                output_path: "C:/Videos/jogo1.mp4",
                status: { Rendering: { percent: 42 } },
                percent: 42,
                elapsed_secs: 12.5,
                report: null,
              },
              {
                id: 2,
                title: "Jogo 2",
                output_path: "C:/Videos/jogo2.mp4",
                status: "Completed",
                percent: 100,
                elapsed_secs: 30,
                report: { encoder: "copy", took_secs: 3.4, video_secs: 21.9 },
              },
            ];
          default:
            return /^list_/.test(cmd) ? [] : null;
        }
      },
    };
  },
  { session, locale },
);
const shot = async (name) => {
  await page.waitForTimeout(350);
  await page.screenshot({ path: `${dir}/${name}.png` });
};
await page.goto("http://localhost:5173/");
await page.waitForTimeout(1500);
await shot("01-home");
await page
  .getByRole("button", { name: /Ajustes|Preferences/ })
  .first()
  .click()
  .catch(() => {});
await shot("02-prefs");
await page.evaluate(() => {
  const el = document.querySelector("[data-testid=screen-prefs]");
  if (el) el.scrollTop = el.scrollHeight;
});
await shot("02b-prefs-bottom");
await page.keyboard.press("Escape");
await page
  .getByRole("button", { name: /Início|Home/ })
  .first()
  .click()
  .catch(() => {});
await page.waitForTimeout(300);
await page
  .getByRole("button", { name: /Monitorar pasta|Watch folder/ })
  .first()
  .click()
  .catch(() => {});
await shot("03-watch-folder");
await page.keyboard.press("Escape");
await page
  .getByRole("button", { name: /Início|Home/ })
  .first()
  .click()
  .catch(() => {});
await page.waitForTimeout(300);
await page
  .getByRole("button", { name: /Novo projeto|New project/ })
  .first()
  .click();
await page.waitForTimeout(300);
await shot("04-new-project-dialog");
await page.getByRole("button", { name: /Criar projeto|Create project/ }).click();
await page.waitForTimeout(1800);
await shot("05-editor");
const box = await page.locator("canvas").first().boundingBox();
const row = (i) => box.y + 52 + 56 * i - 24;
await page.mouse.click(box.x + 300, row(0));
await shot("06-inspector-clip");
for (const tab of [/efeitos|effects/i, /áudio|audio/i]) {
  await page
    .getByRole("tab", { name: tab })
    .last()
    .click()
    .catch(() => {});
  await shot(
    "07-inspector-" +
      String(tab)
        .replace(/[^a-z]/gi, "")
        .slice(0, 8),
  );
}
await page.mouse.click(box.x + 300, row(4));
await shot("08-inspector-text");
await page.mouse.click(box.x + 300, row(5));
await shot("09-inspector-shape");
await page.mouse.click(box.x + 300, row(1), { button: "right" });
await shot("10-clip-menu");
await page.keyboard.press("Escape");
await page.mouse.click(60, box.y + 60, { button: "right" });
await shot("11-track-menu");
await page.keyboard.press("Escape");
for (const tab of [/efeitos|effects/i, /favoritos|favorites/i, /recentes|recent/i]) {
  await page
    .getByRole("tab", { name: tab })
    .first()
    .click()
    .catch(() => {});
  await shot(
    "12-library-" +
      String(tab)
        .replace(/[^a-z]/gi, "")
        .slice(0, 8),
  );
}
await page.keyboard.press("Control+k");
await shot("13-palette");
await page.keyboard.press("Escape");
await page
  .getByRole("button", { name: /Exportar|Export/ })
  .first()
  .click();
await shot("14-export-dialog");
await page.keyboard.press("Escape");
console.log(locale, "errors:", errors.length ? errors.join("\n") : "none");
await browser.close();
