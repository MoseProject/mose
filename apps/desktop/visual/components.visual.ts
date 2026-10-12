import { expect, test, type Page } from "@playwright/test";
import { exportDialog, mockBackend, openEditor } from "./backend";

async function openDialog(page: Page) {
  await mockBackend(page);
  await openEditor(page);
  await page.getByTestId("export").click();
  const dialog = exportDialog(page);
  await expect(dialog.getByTestId("export-summary")).toBeVisible();
  return dialog;
}

// One screenshot per component, so a change to its size, border, radius, color or spacing shows
// as a change to the picture, rather than being checked property by property.

test.describe("form components", () => {
  test("name and folder rows: fields and button side by side", async ({ page }) => {
    const dialog = await openDialog(page);
    const rows = dialog.locator(".dialog-field").filter({ has: page.locator("input[type=text]") });
    await expect(rows.first()).toHaveScreenshot("field-rows-name.png");
    await expect(rows.nth(1)).toHaveScreenshot("field-rows-folder.png");
  });

  test("dropdown row", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(
      dialog
        .locator(".dialog-field")
        .filter({ has: page.locator("select") })
        .first(),
    ).toHaveScreenshot("dropdown-row.png");
  });

  test("a text field in focus", async ({ page }) => {
    const dialog = await openDialog(page);
    // Reached with the keyboard, the way the focus ring shows on every kind of field.
    await dialog.getByLabel("Salvar em").focus();
    await page.keyboard.press("Shift+Tab");
    await expect(dialog.getByLabel("Nome do arquivo")).toBeFocused();
    await expect(
      dialog
        .locator(".dialog-field")
        .filter({ has: page.locator("input[type=text]") })
        .first(),
    ).toHaveScreenshot("text-field-focus.png");
  });

  test("a dropdown in focus", async ({ page }) => {
    const dialog = await openDialog(page);
    const row = dialog
      .locator(".dialog-field")
      .filter({ has: page.locator("select") })
      .first();
    await row.locator("select").focus();
    await page.keyboard.press("Shift+Tab");
    await page.keyboard.press("Tab");
    await expect(row.locator("select")).toBeFocused();
    await expect(row).toHaveScreenshot("dropdown-focus.png");
  });

  test("a button in focus", async ({ page }) => {
    const dialog = await openDialog(page);
    const browse = dialog.getByRole("button", { name: "Procurar…" });
    await dialog.getByLabel("Salvar em").focus();
    await page.keyboard.press("Tab");
    await expect(browse).toBeFocused();
    await expect(browse.locator("xpath=..")).toHaveScreenshot("button-focus.png");
  });

  test("the actions", async ({ page }) => {
    const dialog = await openDialog(page);
    await expect(dialog.locator(".modal-actions")).toHaveScreenshot("actions.png");
  });
});

test.describe("tooltip", () => {
  test("the icon beside the resolution, at rest and with its text", async ({ page }) => {
    const dialog = await openDialog(page);
    await dialog.locator("label", { hasText: "Resolução" }).locator("select").selectOption("1440");
    const row = dialog.locator("label", { hasText: "Resolução" });
    await expect(row).toHaveScreenshot("tooltip-rest.png");
    await dialog.locator("button[aria-describedby]").hover();
    await expect(dialog.locator("[role=tooltip]")).toHaveCSS("opacity", "1");
    await expect(dialog).toHaveScreenshot("tooltip-open.png");
  });
});
