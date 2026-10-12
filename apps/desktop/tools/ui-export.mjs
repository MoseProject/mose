import { chromium } from "/opt/node22/lib/node_modules/playwright/index.mjs";
import fs from "node:fs";
import { readFileSync } from "node:fs";
const sessionFile = process.argv[2];
const session = JSON.parse(readFileSync(sessionFile, "utf8"));
const calls = [];
const browser = await chromium.launch({ executablePath: "/opt/pw-browsers/chromium-1194/chrome-linux/chrome", args: ["--no-sandbox"] });
const page = await browser.newPage({ viewport: { width: 1600, height: 900 } });
await page.exposeFunction("__call", (c, a) => { calls.push([c, a]); });
await page.addInitScript((session) => {
  let cb = 1;
  window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
  window.__TAURI_INTERNALS__ = {
    transformCallback() { return cb++; }, unregisterCallback() {}, convertFileSrc: (p) => p,
    metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    invoke: async (cmd, args) => {
      window.__call(cmd, args);
      switch (cmd) {
        case "get_session": return session;
        case "create_project": return true;
        case "plugin:event|listen": return 1;
        case "get_prefs": return { locale: "pt-BR", lufsProfile: -14, outputFolder: "D:/Videos/Exports", autosaveMinutes: 5, previewHardwareDecode: false, recentProjectPaths: [], savedLayerTemplates: [], whisperModelPath: "", reframeModelPath: "", backgroundRemovalModelPath: "", ttsModelPath: "", soundLibraryPath: "", errorReportingConsent: "disabled", lastReviewedCrashUnix: 0, telemetryEnabled: false, keybindings: {} };
        case "get_gpu_encoder_preference": return "auto";
        case "get_normalize_audio": return true;
        case "export_canvas": return { width: 1920, height: 1080, bitrateMbps: 16, durationSecs: 95 };
        case "export_target": return { path: args.folder + "/" + args.fileName + ".mp4", exists: false };
        case "get_keybindings": return {};
        default: return /^list_|^get_recent/.test(cmd) ? [] : null;
      }
    },
  };
}, session);
await page.goto("http://localhost:5173/");
await page.waitForTimeout(1500);
await page.getByRole("button", { name: /Novo projeto/ }).first().click();
await page.waitForTimeout(400);
await page.getByRole("button", { name: /Criar projeto/ }).click();
await page.waitForTimeout(2000);
// Export dialog
await page.getByRole("button", { name: "Exportar" }).first().click();
await page.waitForTimeout(800);
await page.screenshot({ path: "export-dialog.png" });
await page.getByRole("radio", { name: "Média" }).click();
await page.getByRole("radio", { name: "720p" }).click();
await page.waitForTimeout(300);
await page.screenshot({ path: "export-dialog2.png" });
await page.getByRole("radio", { name: "Personalizada" }).click();
await page.waitForTimeout(300);
await page.screenshot({ path: "export-dialog3.png" });
await page.getByRole("button", { name: "Exportar" }).last().click();
await page.waitForTimeout(600);
console.log(JSON.stringify(calls.filter(([c]) => /export|prefs/.test(c)).map(([c,a])=>[c,a])));
await browser.close();
