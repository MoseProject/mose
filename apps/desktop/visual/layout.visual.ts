import { expect, test, type Locator } from "@playwright/test";
import { mockBackend, openEditor, exportDialog } from "./backend";

const style = (locator: Locator, props: string[]) => {
  return locator.evaluate((node, props) => {
    const computed = getComputedStyle(node);
    return Object.fromEntries(
      props.map((prop) => {
        return [prop, computed.getPropertyValue(prop)];
      }),
    );
  }, props);
};

test.describe("export dialog: measures", () => {
  test.beforeEach(async ({ page }) => {
    await mockBackend(page);
    await openEditor(page);
    await page.getByTestId("export").click();
    await exportDialog(page).waitFor();
    await expect(exportDialog(page).getByTestId("export-summary")).toBeVisible();
  });

  test("stays narrow and never scrolls sideways", async ({ page }) => {
    const panel = exportDialog(page);
    const box = await panel.boundingBox();
    expect(box?.width).toBeLessThanOrEqual(420);
    const overflow = await panel.evaluate((node) => {
      return node.scrollWidth - node.clientWidth;
    });
    expect(overflow).toBe(0);
  });

  test("text fields look like the dropdowns they sit with", async ({ page }) => {
    const dialog = exportDialog(page);
    const props = ["border-top-width", "border-top-left-radius", "background-color", "font-size"];
    const text = await style(dialog.locator("input[type=text]").first(), props);
    const dropdown = await style(dialog.locator("select").first(), props);
    expect(text).toEqual(dropdown);
    expect(text["border-top-width"]).toBe("1px");
  });

  test("the focus ring is 1px, takes the place of the border and follows its shape", async ({
    page,
  }) => {
    const dialog = exportDialog(page);
    for (const field of [
      dialog.locator("input[type=text]").first(),
      dialog.locator("select").first(),
    ]) {
      // Reached with the keyboard, which is what makes the ring show on a dropdown.
      await field.focus();
      await page.keyboard.press("Shift+Tab");
      await page.keyboard.press("Tab");
      await expect(field).toBeFocused();
      const ring = await style(field, [
        "outline-width",
        "outline-style",
        "outline-offset",
        "border-top-left-radius",
      ]);
      expect(ring["outline-width"]).toBe("1px");
      expect(ring["outline-style"]).toBe("solid");
      expect(ring["outline-offset"]).toBe("-1px");
    }
  });

  test("the tooltip has no border and no button chrome", async ({ page }) => {
    const dialog = exportDialog(page);
    await dialog.locator("label", { hasText: "Resolução" }).locator("select").selectOption("1440");
    const button = dialog.locator("button[aria-describedby]");
    const tip = dialog.locator("[role=tooltip]");
    expect(await style(button, ["border-top-width", "background-color"])).toEqual({
      "border-top-width": "0px",
      "background-color": "rgba(0, 0, 0, 0)",
    });
    expect(await style(tip, ["border-top-width", "opacity", "visibility"])).toEqual({
      "border-top-width": "0px",
      opacity: "0",
      visibility: "hidden",
    });
    await button.hover();
    await expect(tip).toHaveCSS("opacity", "1");
  });
});
