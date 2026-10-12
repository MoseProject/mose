import { expect, test, type Page } from "@playwright/test";
import { backendCalls, mockBackend, openEditor, exportDialog } from "./backend";

async function openDialog(page: Page, motion: boolean) {
  await mockBackend(page, { motion });
  await openEditor(page);
  await page.getByTestId("export").click();
  const dialog = exportDialog(page);
  await expect(dialog.getByRole("button", { name: "Exportar" })).toBeEnabled();
  // The sizes offered come with the canvas, a moment after the dialog opens.
  await expect(dialog.getByTestId("export-summary")).toBeVisible();
  return dialog;
}

test.describe("with motion", () => {
  test("a dialog fades in", async ({ page }) => {
    const dialog = await openDialog(page, true);
    const overlay = dialog.locator("xpath=..");
    for (const node of [overlay, dialog]) {
      const animation = await node.evaluate((element) => {
        const computed = getComputedStyle(element);
        return { name: computed.animationName, duration: computed.animationDuration };
      });
      expect(animation.name).not.toBe("none");
      expect(animation.duration).toBe("0.16s");
    }
  });

  test("the tooltip text fades in and out", async ({ page }) => {
    const dialog = await openDialog(page, true);
    await dialog.locator("label", { hasText: "Resolução" }).locator("select").selectOption("1440");
    const tip = dialog.locator("[role=tooltip]");
    await expect(tip).toHaveCSS("transition-property", /opacity/);
    await expect(tip).toHaveCSS("opacity", "0");
    await dialog.locator("button[aria-describedby]").hover();
    await expect(tip).toHaveCSS("opacity", "1");
    await page.mouse.move(5, 5);
    await expect(tip).toHaveCSS("opacity", "0");
  });

  test("Cancel fades the dialog out before it goes", async ({ page }) => {
    const dialog = await openDialog(page, true);
    await dialog.getByRole("button", { name: "Cancelar" }).click();
    await expect(dialog.locator("xpath=..")).toHaveClass(/leaving/);
    await expect(dialog.locator("xpath=..")).not.toHaveClass(/flying/);
    await expect(exportDialog(page)).toHaveCount(0);
  });

  test("Escape does the same", async ({ page }) => {
    const dialog = await openDialog(page, true);
    await page.keyboard.press("Escape");
    await expect(dialog.locator("xpath=..")).toHaveClass(/leaving/);
    await expect(exportDialog(page)).toHaveCount(0);
  });

  test("Export shrinks the dialog towards the queue button, then opens the queue", async ({
    page,
  }) => {
    const dialog = await openDialog(page, true);
    const target = await page.locator("[data-fly-target=export-queue]").boundingBox();
    const from = await dialog.boundingBox();
    await dialog.getByRole("button", { name: "Exportar" }).click();
    const overlay = dialog.locator("xpath=..");
    await expect(overlay).toHaveClass(/flying/);
    const transform = await dialog.evaluate((element) => {
      return (element as HTMLElement).style.transform;
    });
    const [, dx, dy, scale] = transform.match(
      /translate\(([-\d.]+)px, ([-\d.]+)px\) scale\(([\d.]+)\)/,
    )!;
    // The panel's centre ends on the button's centre, and it has become much smaller.
    expect(from!.x + from!.width / 2 + Number(dx)).toBeCloseTo(target!.x + target!.width / 2, 0);
    expect(from!.y + from!.height / 2 + Number(dy)).toBeCloseTo(target!.y + target!.height / 2, 0);
    expect(Number(scale)).toBeLessThanOrEqual(0.2);
    await expect(exportDialog(page)).toHaveCount(0);
    await expect(page.getByRole("dialog", { name: "Fila de exportação" })).toBeVisible();
    expect(await backendCalls(page, "start_export")).toHaveLength(1);
  });
});

test.describe("with reduced motion", () => {
  test("a dialog opens and closes without any motion", async ({ page }) => {
    const dialog = await openDialog(page, false);
    const overlay = dialog.locator("xpath=..");
    await expect(overlay).toHaveCSS("animation-name", "none");
    await dialog.getByRole("button", { name: "Cancelar" }).click();
    await expect(exportDialog(page)).toHaveCount(0, { timeout: 100 });
  });

  test("Export closes the dialog and still queues the job", async ({ page }) => {
    const dialog = await openDialog(page, false);
    await dialog.getByRole("button", { name: "Exportar" }).click();
    await expect(exportDialog(page)).toHaveCount(0, { timeout: 300 });
    expect(await backendCalls(page, "start_export")).toHaveLength(1);
  });
});
