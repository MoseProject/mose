// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { uiStore } from "../store";
import { Dialog } from "./Dialog";

beforeEach(() => {
  uiStore.set({ ...uiStore.get(), openDialogs: 0 });
});
afterEach(() => {
  cleanup();
});

function open(onClose = vi.fn(), dismissible?: boolean) {
  return render(
    <div>
      <button>opener</button>
      <Dialog
        title="Options"
        onClose={onClose}
        dismissible={dismissible}
        actions={<button>done</button>}
      >
        <input aria-label="name" />
      </Dialog>
    </div>,
  );
}

describe("Dialog", () => {
  it("is a labelled modal dialog", () => {
    open();
    const dialog = screen.getByRole("dialog", { name: "Options" });
    expect(dialog.getAttribute("aria-modal")).toBe("true");
  });

  it("focuses its first control and registers itself while open", () => {
    const { unmount } = open();
    expect(document.activeElement).toBe(screen.getByLabelText("name"));
    expect(uiStore.get().openDialogs).toBe(1);
    unmount();
    expect(uiStore.get().openDialogs).toBe(0);
  });

  it("closes on Escape and on a backdrop click, but not on a click inside", () => {
    const onClose = vi.fn();
    const { container } = open(onClose);
    fireEvent.click(screen.getByRole("dialog"));
    expect(onClose).not.toHaveBeenCalled();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(onClose).toHaveBeenCalledTimes(2);
  });

  it("ignores Escape and the backdrop when it must not be dismissed", () => {
    const onClose = vi.fn();
    const { container } = open(onClose, false);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(onClose).not.toHaveBeenCalled();
  });

  it("keeps Tab inside the dialog", () => {
    open();
    const input = screen.getByLabelText("name");
    const done = screen.getByText("done");
    done.focus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab" });
    expect(document.activeElement).toBe(input);
    input.focus();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(done);
  });

  it("gives focus back to the control that opened it", () => {
    const opener = document.createElement("button");
    document.body.appendChild(opener);
    opener.focus();
    const { unmount } = render(
      <Dialog
        title="T"
        onClose={() => {}}
      >
        <input />
      </Dialog>,
    );
    expect(document.activeElement).not.toBe(opener);
    unmount();
    expect(document.activeElement).toBe(opener);
    opener.remove();
  });
});
