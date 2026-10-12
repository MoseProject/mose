import { expect, test, type Page } from "@playwright/test";
import { backendCalls, mockBackend, openEditor, exportDialog } from "./backend";

/** Ctrl+click the first three clips of the first track, then right-click the middle one. */
async function selectThreeAndOpenMenu(page: Page) {
  await mockBackend(page);
  await openEditor(page);
  const canvas = page.getByTestId("timeline-canvas");
  const box = (await canvas.boundingBox())!;
  const row = box.y + 62;
  const clips = [box.x + 100, box.x + 300, box.x + 500];
  await page.mouse.click(clips[0], row);
  await page.keyboard.down("Control");
  await page.mouse.click(clips[1], row);
  await page.mouse.click(clips[2], row);
  await page.keyboard.up("Control");
  await page.mouse.click(clips[1], row, { button: "right" });
  return page.locator(".context-menu");
}

test("the clip menu offers to export the selected clips", async ({ page }) => {
  const menu = await selectThreeAndOpenMenu(page);
  await expect(menu.getByText("Exportar 3 clipes…")).toBeVisible();
  await expect(menu).toHaveScreenshot("menu-several.png");
});

test("one clip is offered as a single export", async ({ page }) => {
  await mockBackend(page);
  await openEditor(page);
  const box = (await page.getByTestId("timeline-canvas").boundingBox())!;
  await page.mouse.click(box.x + 300, box.y + 62, { button: "right" });
  await expect(page.locator(".context-menu").getByText("Exportar clipe…")).toBeVisible();
});

test("the dialog says how many videos it will make and names them by prefix", async ({ page }) => {
  const menu = await selectThreeAndOpenMenu(page);
  await menu.getByText("Exportar 3 clipes…").click();
  const dialog = exportDialog(page);
  await expect(dialog.getByRole("heading", { name: "Exportar 3 vídeos" })).toBeVisible();
  await expect(dialog.getByLabel("Prefixo do nome")).toHaveValue("");
  await expect(dialog.getByText(".mp4")).toHaveCount(0);
  await expect(dialog.getByTestId("export-summary")).toContainText("total");
  await expect(dialog).toHaveScreenshot("dialog-several.png");
});

test("exporting sends every selected clip, once", async ({ page }) => {
  const menu = await selectThreeAndOpenMenu(page);
  await menu.getByText("Exportar 3 clipes…").click();
  const dialog = exportDialog(page);
  await dialog.getByLabel("Prefixo do nome").fill("Cut");
  await dialog.getByRole("button", { name: "Exportar" }).click();
  await expect(exportDialog(page)).toHaveCount(0);
  const [start] = await backendCalls(page, "start_export_clips");
  expect(start.args).toMatchObject({ folder: "D:/Videos/Exports", prefix: "Cut", rename: false });
  expect((start.args as { clipIds: number[] }).clipIds).toHaveLength(3);
  expect(await backendCalls(page, "start_export")).toHaveLength(0);
});

test("a name that exists is flagged with the count", async ({ page }) => {
  await mockBackend(page, { existing: ["Cut 1", "Cut 2"] });
  await openEditor(page);
  const box = (await page.getByTestId("timeline-canvas").boundingBox())!;
  const row = box.y + 62;
  await page.mouse.click(box.x + 100, row);
  await page.keyboard.down("Control");
  await page.mouse.click(box.x + 300, row);
  await page.keyboard.up("Control");
  await page.mouse.click(box.x + 300, row, { button: "right" });
  await page.locator(".context-menu").getByText("Exportar 2 clipes…").click();
  const dialog = exportDialog(page);
  await dialog.getByLabel("Prefixo do nome").fill("Cut");
  await expect(dialog.getByRole("status")).toContainText("2 desses arquivos");
});
