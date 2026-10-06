// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ContextMenu } from "../../src/editor/chrome/ContextMenu";

afterEach(() => {
  cleanup();
});

const items = (onSelect = vi.fn()) => {
  return [
    { label: "Split", shortcut: "S", onSelect },
    { label: "Copy", separator: true, onSelect: vi.fn() },
    { label: "Delete", disabled: true, onSelect: vi.fn() },
  ];
};

describe("ContextMenu", () => {
  it("lists the items with their shortcuts and focuses the first enabled one", () => {
    render(
      <ContextMenu
        x={10}
        y={20}
        items={items()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByRole("menu")).toBeTruthy();
    expect(screen.getAllByRole("menuitem")).toHaveLength(3);
    expect(screen.getByText("S").tagName).toBe("KBD");
    expect(document.activeElement).toBe(screen.getByText("Split").closest("button"));
    expect((screen.getByText("Delete").closest("button") as HTMLButtonElement).disabled).toBe(true);
  });

  it("closes first, then runs the chosen item", () => {
    const order: string[] = [];
    const onClose = vi.fn(() => {
      return order.push("close");
    });
    const onSelect = vi.fn(() => {
      return order.push("select");
    });
    render(
      <ContextMenu
        x={0}
        y={0}
        items={items(onSelect)}
        onClose={onClose}
      />,
    );
    fireEvent.click(screen.getByText("Split"));
    expect(order).toEqual(["close", "select"]);
  });

  it("closes on Escape, on a press outside and on blur, but not on a press inside", () => {
    const onClose = vi.fn();
    render(
      <ContextMenu
        x={0}
        y={0}
        items={items()}
        onClose={onClose}
      />,
    );
    fireEvent.pointerDown(screen.getByRole("menu"));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body);
    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.blur(window);
    expect(onClose).toHaveBeenCalledTimes(3);
  });

  it("does not open the browser's own menu over itself", () => {
    render(
      <ContextMenu
        x={0}
        y={0}
        items={items()}
        onClose={vi.fn()}
      />,
    );
    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    screen.getByRole("menu").dispatchEvent(event);
    expect(event.defaultPrevented).toBe(true);
  });
});
