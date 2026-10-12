import { expect, test, type Page } from "@playwright/test";
import { backendCalls, mockBackend, openEditor, type MockOptions, exportDialog } from "./backend";

async function openDialog(page: Page, options: MockOptions = {}) {
  await mockBackend(page, options);
  await openEditor(page);
  await page.getByTestId("export").click();
  const dialog = exportDialog(page);
  await dialog.waitFor();
  // The stored folder and the size summary arrive after the dialog mounts.
  await expect(dialog.getByRole("button", { name: "Exportar" })).toBeEnabled();
  return dialog;
}

const select = (page: Page, label: string) => {
  return exportDialog(page).locator("label", { hasText: label }).locator("select");
};

test.describe("export dialog: how it looks", () => {
  test("default state", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(dialog.getByTestId("export-summary")).toBeVisible();
    await expect(dialog).toHaveScreenshot("default.png");
  });

  test("a bitrate of its own", async ({ page }) => {
    const dialog = await openDialog(page);
    await select(page, "Qualidade").selectOption("custom");
    await expect(dialog).toHaveScreenshot("custom-quality.png");
  });

  test("an upscale, with its tooltip", async ({ page }) => {
    const dialog = await openDialog(page);
    await select(page, "Resolução").selectOption("1440");
    await dialog.locator("button[aria-describedby]").hover();
    await expect(dialog.locator("[role=tooltip]")).toBeVisible();
    await expect(dialog).toHaveScreenshot("upscale-tooltip.png");
  });

  test("a loudness target of its own", async ({ page }) => {
    const dialog = await openDialog(page);
    await select(page, "Loudness alvo").selectOption("custom");
    await expect(dialog).toHaveScreenshot("custom-loudness.png");
  });

  test("a file that already exists", async ({ page }) => {
    const dialog = await openDialog(page, { existing: ["seq"] });
    await expect(dialog.getByRole("status")).toBeVisible();
    await expect(dialog).toHaveScreenshot("existing-file.png");
  });

  test("a source bitrate too low for the higher quality steps", async ({ page }) => {
    const dialog = await openDialog(page, {
      canvas: { width: 1920, height: 1080, bitrateMbps: 6, durationSecs: 95 },
    });
    await expect(dialog.getByText(/indisponíveis/)).toBeVisible();
    await expect(dialog).toHaveScreenshot("low-source-bitrate.png");
  });

  test("no folder chosen yet", async ({ page }) => {
    await mockBackend(page, { outputFolder: "" });
    await openEditor(page);
    await page.getByTestId("export").click();
    const dialog = exportDialog(page);
    await expect(dialog.getByRole("button", { name: "Exportar" })).toBeDisabled();
    await expect(dialog).toHaveScreenshot("no-folder.png");
  });

  test("an empty timeline", async ({ page }) => {
    await mockBackend(page, { canvas: null });
    await openEditor(page);
    await page.getByTestId("export").click();
    const dialog = exportDialog(page);
    await expect(dialog.getByRole("alert")).toBeVisible();
    await expect(dialog).toHaveScreenshot("empty-timeline.png");
  });
});

test.describe("export dialog: what it offers", () => {
  test("has the options that belong to an export, and no others", async ({ page }) => {
    const dialog = await openDialog(page);
    const labels = await dialog.locator("select").evaluateAll((nodes) => {
      return nodes.map((node) => {
        return node.closest("label")?.textContent?.split("\n")[0].trim() ?? "";
      });
    });
    // Format, quality, size and loudness: no encoder choice (a machine setting) and no aspect
    // ratio (the project's own).
    expect(labels.length).toBe(4);
    await expect(dialog.getByText(/GPU|Codificador|Proporção/)).toHaveCount(0);
    await expect(dialog.locator("input[type=text]")).toHaveCount(2);
  });

  test("keeps Export off until it can work", async ({ page }) => {
    await mockBackend(page, { outputFolder: "" });
    await openEditor(page);
    await page.getByTestId("export").click();
    const dialog = exportDialog(page);
    const button = dialog.getByRole("button", { name: "Exportar" });
    await expect(button).toBeDisabled();
    await dialog.getByLabel("Salvar em").fill("D:/out");
    await expect(button).toBeEnabled();
    await dialog.getByLabel("Nome do arquivo").fill("   ");
    await expect(button).toBeDisabled();
  });

  test("disables the quality steps above the source bitrate", async ({ page }) => {
    await openDialog(page, {
      canvas: { width: 1920, height: 1080, bitrateMbps: 6, durationSecs: 95 },
    });
    const disabled = await select(page, "Qualidade")
      .locator("option")
      .evaluateAll((options) => {
        return (options as HTMLOptionElement[])
          .filter((option) => {
            return option.disabled;
          })
          .map((option) => {
            return option.value;
          });
      });
    expect(disabled).toEqual(["high", "medium"]);
  });

  test("offers larger sizes as an upscale, with the original recommended", async ({ page }) => {
    await openDialog(page);
    const texts = await select(page, "Resolução").locator("option").allTextContents();
    expect(texts[0]).toBe("Original (1920x1080) · recomendado");
    expect(texts).toContain("1440p · ampliar");
    expect(texts).toContain("720p");
  });

  test("shows the custom loudness field only for the custom choice", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(dialog.getByRole("spinbutton")).toHaveCount(0);
    await select(page, "Loudness alvo").selectOption("custom");
    await expect(dialog.getByRole("spinbutton")).toHaveCount(1);
    await select(page, "Loudness alvo").selectOption("-16");
    await expect(dialog.getByRole("spinbutton")).toHaveCount(0);
  });

  test("sends what was chosen and queues the job", async ({ page }) => {
    const dialog = await openDialog(page);
    await dialog.getByLabel("Nome do arquivo").fill("final");
    await select(page, "Qualidade").selectOption("low");
    await select(page, "Resolução").selectOption("720");
    await dialog.getByRole("button", { name: "Exportar" }).click();
    await expect(exportDialog(page)).toHaveCount(0);
    const [start] = await backendCalls(page, "start_export");
    expect(start.args).toEqual({
      path: "D:/Videos/Exports/final.mp4",
      rename: false,
      options: { bitrateMbps: 4, maxHeight: 720 },
    });
  });
});
