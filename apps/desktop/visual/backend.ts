import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";

/** A session dumped from the Rust side: `OCA_DUMP_SESSION=... OCA_DUMP_TRACKS=2 OCA_DUMP_CLIPS=6
 *  cargo test -p desktop dump_session_json -- --ignored` (see `visual/README.md`). */
export const SESSION: Record<string, unknown> = JSON.parse(
  readFileSync(fileURLToPath(new URL("./fixtures/session.json", import.meta.url)), "utf8"),
);

export interface MockOptions {
  /** Output folder the preferences hold. */
  outputFolder?: string;
  /** What `export_canvas` answers; `null` makes it fail, like an empty timeline. */
  canvas?: { width: number; height: number; bitrateMbps: number; durationSecs: number } | null;
  /** File names (without extension) that already exist in the output folder. */
  existing?: string[];
  /** Normalization stored in the app settings. */
  normalizeAudio?: boolean;
  /** Replaces the project's session. */
  session?: Record<string, unknown>;
  /** Lets animations run. Off by default, so screenshots and measures see the settled page. */
  motion?: boolean;
}

interface Call {
  command: string;
  args: unknown;
}

/** Installs a stand-in for the Tauri bridge before the page loads: every command answers from the
 *  fixture and is recorded, so a test can look at what the UI asked the backend to do. */
export async function mockBackend(page: Page, options: MockOptions = {}) {
  // Asked for here, per page: the config's `reducedMotion` did not reach the page.
  await page.emulateMedia({ reducedMotion: options.motion ? "no-preference" : "reduce" });
  const canvas =
    options.canvas === undefined
      ? { width: 1920, height: 1080, bitrateMbps: 16, durationSecs: 95 }
      : options.canvas;
  await page.addInitScript(
    ({ session, options, canvas }) => {
      const calls: Call[] = [];
      (window as unknown as { __calls: Call[] }).__calls = calls;
      let callback = 1;
      const prefs = {
        locale: "pt-BR",
        lufsProfile: -14,
        outputFolder: options.outputFolder ?? "D:/Videos/Exports",
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
      const target = (folder: string, name: string) => {
        return {
          path: `${folder}/${name}.mp4`,
          exists: (options.existing ?? []).includes(name),
        };
      };
      Object.assign(window, {
        __TAURI_EVENT_PLUGIN_INTERNALS__: { unregisterListener() {} },
        __TAURI_INTERNALS__: {
          transformCallback: () => {
            return callback++;
          },
          unregisterCallback() {},
          convertFileSrc: (path: string) => {
            return path;
          },
          metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
          invoke: async (command: string, args: Record<string, unknown>) => {
            calls.push({ command, args });
            switch (command) {
              case "get_session":
                return session;
              case "create_project":
                return true;
              case "plugin:event|listen":
                return 1;
              case "get_prefs":
                return prefs;
              case "get_normalize_audio":
                return options.normalizeAudio ?? true;
              case "get_gpu_encoder_preference":
                return "auto";
              case "get_keybindings":
                return {};
              case "export_canvas":
                if (canvas === null) {
                  throw "nothing to export";
                }
                return canvas;
              case "export_target":
                return target(String(args.folder), String(args.fileName));
              case "export_clips_targets":
                return (args.clipIds as number[]).map((id) => {
                  return target(String(args.folder), `${args.prefix} ${id}`);
                });
              case "pick_path":
                return "E:/Picked";
              case "start_export":
                return 1;
              case "start_export_clips":
                return (args.clipIds as number[]).map((_, index) => {
                  return index + 1;
                });
              default:
                return /^list_|^get_recent/.test(command) ? [] : null;
            }
          },
        },
      });
    },
    { session: options.session ?? SESSION, options, canvas },
  );
}

/** What the page sent to the backend, in order. */
export async function backendCalls(page: Page, command?: string): Promise<Call[]> {
  const calls = await page.evaluate(() => {
    return (window as unknown as { __calls: Call[] }).__calls;
  });
  return command
    ? calls.filter((call) => {
        return call.command === command;
      })
    : calls;
}

/** Opens the editor on the fixture project, from the home screen. */
export async function openEditor(page: Page) {
  await page.goto("/");
  await page
    .getByRole("button", { name: /Novo projeto/ })
    .first()
    .click();
  await page.getByRole("button", { name: /Criar projeto/ }).click();
  await page.getByTestId("timeline-canvas").waitFor();
}

/** The export dialog, not the queue panel that opens after it (that one is a dialog too). */
export function exportDialog(page: Page) {
  return page.getByRole("dialog", { name: /^Exportar (sequência|\d+ vídeos)$/ });
}
