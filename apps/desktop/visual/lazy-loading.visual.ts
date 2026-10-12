import { expect, test, type Page } from "@playwright/test";
import { mockBackend, openEditor } from "./backend";

/** The source modules the page has asked the dev server for, by file name. */
function recordModules(page: Page) {
  const seen = new Set<string>();
  page.on("request", (request) => {
    const match = /\/src\/(.+?)\.(?:tsx?|ts)(?:\?|$)/.exec(request.url());
    if (match) {
      seen.add(match[1]);
    }
  });
  return {
    has: (name: string) => {
      return [...seen].some((path) => {
        return path.endsWith(name);
      });
    },
  };
}

const ON_DEMAND = [
  "inspector/TextInspector",
  "inspector/ShapeInspector",
  "inspector/AiActionsSection",
  "LayerTemplates",
  "TranscriptPanel",
  "SilenceReviewPanel",
  "library/SoundLibraryButton",
  "library/YoutubeDownloadButton",
  "library/TtsButton",
  "library/EffectsBrowser",
  "export/ExportDialog",
  "screens/Prefs",
  "chrome/ShortcutsDialog",
  "screens/WatchFolder",
];

test.describe("what is fetched, and when", () => {
  test("an open project fetches none of the code that is not on screen", async ({ page }) => {
    const modules = recordModules(page);
    await mockBackend(page);
    await openEditor(page);
    // The editor itself is there; nothing that waits for a click, a tab or a menu is.
    expect(modules.has("screens/Editor")).toBe(true);
    for (const name of ON_DEMAND) {
      expect(modules.has(name), name).toBe(false);
    }
  });

  test("only the language in use is fetched", async ({ page }) => {
    const modules = recordModules(page);
    await mockBackend(page);
    await openEditor(page);
    expect(modules.has("locales/pt-BR")).toBe(true);
    expect(modules.has("locales/en")).toBe(false);
  });

  test("the Effects tab brings the AI actions when it is shown, the Inspector tab keeps its own", async ({
    page,
  }) => {
    const modules = recordModules(page);
    await mockBackend(page);
    await openEditor(page);
    const box = (await page.getByTestId("timeline-canvas").boundingBox())!;
    await page.mouse.click(box.x + 300, box.y + 62);
    await expect(page.getByTestId("section-mask")).toBeVisible();
    expect(modules.has("inspector/AiActionsSection")).toBe(false);
    await page
      .getByRole("tab", { name: /Efeitos/i })
      .last()
      .click();
    await expect(page.getByText("Ações de IA")).toBeVisible();
    await expect
      .poll(() => {
        return modules.has("inspector/AiActionsSection");
      })
      .toBe(true);
  });

  test("a menu brings its dialogs when it is first opened", async ({ page }) => {
    const modules = recordModules(page);
    await mockBackend(page);
    await openEditor(page);
    expect(modules.has("TranscriptPanel")).toBe(false);
    await page.getByRole("button", { name: "Analisar" }).click();
    await expect(page.getByRole("button", { name: /Transcrever/i }).first()).toBeAttached();
    expect(modules.has("TranscriptPanel")).toBe(true);
    expect(modules.has("SilenceReviewPanel")).toBe(true);
    expect(modules.has("LayerTemplates")).toBe(false);
  });

  test("the export dialog is fetched as the pointer reaches its button", async ({ page }) => {
    const modules = recordModules(page);
    await mockBackend(page);
    await openEditor(page);
    expect(modules.has("export/ExportDialog")).toBe(false);
    await page.getByTestId("export").hover();
    await expect
      .poll(() => {
        return modules.has("export/ExportDialog");
      })
      .toBe(true);
  });

  test("a media thumbnail is requested only when its card nears the view", async ({ page }) => {
    await mockBackend(page);
    await openEditor(page);
    const thumbs = page.locator("img.thumb, img[class*=thumb]");
    const count = await thumbs.count();
    expect(count).toBeGreaterThan(0);
    const loading = await thumbs.evaluateAll((nodes) => {
      return nodes.map((node) => {
        return (node as HTMLImageElement).loading;
      });
    });
    expect(new Set(loading)).toEqual(new Set(["lazy"]));
  });
});
