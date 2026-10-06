// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      setKeybindings: vi.fn((table: Record<string, string[]>) => {
        return Promise.resolve(table);
      }),
      getKeybindings: vi.fn().mockResolvedValue({}),
    },
  };
});

const { ShortcutsDialog } = await import("../../src/editor/chrome/ShortcutsDialog");
const { ConfirmHost } = await import("../../src/editor/chrome/Notifications");
const { api } = await import("../../src/ipc");
const { keymapStore } = await import("../../src/keymap");
const { uiStore } = await import("../../src/store");

const row = (id: string) => {
  return document.querySelector('[data-command="' + id + '"]') as HTMLElement;
};
const record = (id: string) => {
  fireEvent.click(within(row(id)).getByLabelText(new RegExp("^" + t("keymapAdd"))));
};
const press = (init: KeyboardEventInit) => {
  act(() => {
    document.dispatchEvent(
      new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init }),
    );
  });
};

beforeEach(() => {
  vi.clearAllMocks();
  keymapStore.set({});
  uiStore.set({ ...uiStore.get(), shortcutsOpen: true, openDialogs: 0 });
});
afterEach(() => {
  cleanup();
});

describe("shortcut sheet", () => {
  it("lists commands by group with their bindings", () => {
    render(<ShortcutsDialog />);
    expect(screen.getByRole("dialog", { name: t("cmdShortcuts") })).toBeTruthy();
    expect(within(row("undo")).getByText("Ctrl+Z")).toBeTruthy();
    expect(within(row("redo")).getAllByRole("generic").length).toBeGreaterThan(0);
    expect(screen.getByText(t("groupPlayback"))).toBeTruthy();
  });

  it("filters by command name or by key, ignoring accents and case", () => {
    render(<ShortcutsDialog />);
    const search = screen.getByLabelText(t("shortcutsSearch"));
    fireEvent.change(search, { target: { value: t("undo").toUpperCase() } });
    expect(row("undo")).toBeTruthy();
    expect(row("playPause")).toBeNull();
    fireEvent.change(search, { target: { value: "ctrl+shift+z" } });
    expect(row("redo")).toBeTruthy();
    expect(row("undo")).toBeNull();
    fireEvent.change(search, { target: { value: "zzzz" } });
    expect(screen.getByText(t("shortcutsNone"))).toBeTruthy();
  });

  it("marks a command the user has changed", () => {
    keymapStore.set({ undo: ["Alt+U"] });
    render(<ShortcutsDialog />);
    expect(row("undo").className).toContain("customized");
    expect(within(row("undo")).getByText("Alt+U")).toBeTruthy();
    expect(row("redo").className).not.toContain("customized");
  });
});

describe("recording a binding", () => {
  it("adds a free binding as soon as the keys are pressed and saves it", async () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    expect(screen.getByText(t("keymapPress"))).toBeTruthy();
    press({ key: "q", altKey: true });
    await waitFor(() => {
      expect(api.setKeybindings).toHaveBeenCalledWith({ addMarker: ["M", "Alt+Q"] });
    });
    expect(within(row("addMarker")).getByText("Alt+Q")).toBeTruthy();
    expect(screen.queryByText(t("keymapPress"))).toBeNull();
  });

  it("waits through bare modifiers and cancels on Escape without closing the dialog", () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    press({ key: "Control", ctrlKey: true });
    expect(screen.getByText(t("keymapPress"))).toBeTruthy();
    press({ key: "Escape" });
    expect(screen.queryByText(t("keymapPress"))).toBeNull();
    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(api.setKeybindings).not.toHaveBeenCalled();
  });

  it("refuses a reserved key and says why", () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    press({ key: "w", ctrlKey: true });
    expect(screen.getByRole("alert").textContent).toContain(t("keymapReserved"));
    expect(api.setKeybindings).not.toHaveBeenCalled();
  });

  it("refuses a duplicate on the same command", () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    press({ key: "m" });
    expect(screen.getByRole("alert").textContent).toContain(t("keymapDuplicate"));
  });

  it("asks before taking a binding from another command, and replaces on request", async () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    press({ key: "z", ctrlKey: true });
    const notice = screen.getByRole("alert");
    expect(notice.textContent).toContain("Ctrl+Z");
    expect(notice.textContent).toContain(t("undo"));
    expect(api.setKeybindings).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText(t("keymapReplace")));
    await waitFor(() => {
      expect(api.setKeybindings).toHaveBeenCalledWith({ addMarker: ["M", "Ctrl+Z"], undo: [] });
    });
    expect(within(row("undo")).getByText(t("keymapUnbound"))).toBeTruthy();
  });

  it("leaves everything as it was when the conflict is cancelled", () => {
    render(<ShortcutsDialog />);
    record("addMarker");
    press({ key: "z", ctrlKey: true });
    fireEvent.click(within(screen.getByRole("alert")).getByText(t("cancel")));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(api.setKeybindings).not.toHaveBeenCalled();
  });
});

describe("removing and restoring", () => {
  it("removes one binding of a command", async () => {
    render(<ShortcutsDialog />);
    fireEvent.click(within(row("redo")).getByLabelText(t("keymapRemove") + ": Ctrl+Y"));
    await waitFor(() => {
      expect(api.setKeybindings).toHaveBeenCalledWith({ redo: ["Ctrl+Shift+Z"] });
    });
  });

  it("restores a single command to its default", async () => {
    keymapStore.set({ undo: ["Alt+U"], redo: [] });
    render(<ShortcutsDialog />);
    fireEvent.click(within(row("undo")).getByLabelText(new RegExp("^" + t("keymapReset"))));
    await waitFor(() => {
      expect(api.setKeybindings).toHaveBeenCalledWith({ redo: [] });
    });
  });

  it("restores everything only after a confirmation", async () => {
    keymapStore.set({ undo: ["Alt+U"] });
    render(
      <>
        <ShortcutsDialog />
        <ConfirmHost />
      </>,
    );
    fireEvent.click(screen.getByText(t("keymapResetAll"), { selector: "button" }));
    expect(await screen.findByText(t("keymapResetAllMessage"))).toBeTruthy();
    expect(api.setKeybindings).not.toHaveBeenCalled();
    fireEvent.click(screen.getAllByText(t("keymapResetAll"), { selector: "button" }).at(-1)!);
    await waitFor(() => {
      expect(api.setKeybindings).toHaveBeenCalledWith({});
    });
  });

  it("disables the global restore when nothing is customized", () => {
    render(<ShortcutsDialog />);
    expect((screen.getByText(t("keymapResetAll")) as HTMLButtonElement).disabled).toBe(true);
  });
});
