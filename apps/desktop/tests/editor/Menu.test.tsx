// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Menu } from "../../src/editor/chrome/Menu";

afterEach(() => {
  cleanup();
});

const mount = (onPick = vi.fn(), variant?: "bar") => {
  render(
    <Menu
      label="Edit"
      variant={variant}
    >
      <button onClick={onPick}>Pick</button>
    </Menu>,
  );
  return screen.getByText("Edit", { selector: "button" });
};
const pop = () => {
  return document.querySelector(".menu-pop") as HTMLElement;
};

describe("Menu", () => {
  it("is closed until the trigger is pressed, and says so to assistive tech", () => {
    const trigger = mount();
    expect(trigger.getAttribute("aria-haspopup")).toBe("menu");
    expect(trigger.getAttribute("aria-expanded")).toBe("false");
    expect(pop().className).not.toContain("open");
    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");
    expect(pop().className).toContain("open");
  });

  it("closes after an item is chosen", () => {
    const onPick = vi.fn();
    fireEvent.click(mount(onPick));
    fireEvent.click(screen.getByText("Pick"));
    expect(onPick).toHaveBeenCalled();
    expect(pop().className).not.toContain("open");
  });

  it("closes on Escape and on a press outside, not on a press inside", () => {
    fireEvent.click(mount());
    fireEvent.pointerDown(pop());
    expect(pop().className).toContain("open");
    fireEvent.keyDown(document, { key: "Escape" });
    expect(pop().className).not.toContain("open");
    fireEvent.click(screen.getByText("Edit", { selector: "button" }));
    fireEvent.pointerDown(document.body);
    expect(pop().className).not.toContain("open");
  });

  it("uses the menu-bar look without a chevron for the bar variant", () => {
    const trigger = mount(vi.fn(), "bar");
    expect(trigger.className).toContain("menubar-item");
    expect(trigger.querySelector("svg")).toBeNull();
  });

  it("shows a chevron on the ordinary variant", () => {
    expect(mount().querySelector("svg")).toBeTruthy();
  });
});
