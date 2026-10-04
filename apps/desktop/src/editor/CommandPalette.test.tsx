// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";

vi.mock("../ipc", () => {
  const names = [
    "undo",
    "redo",
    "splitAtPlayhead",
    "addMarker",
    "saveProject",
    "importMediaDialog",
  ];
  return {
    api: Object.fromEntries(
      names.map((name) => {
        return [name, vi.fn().mockResolvedValue(undefined)];
      }),
    ),
  };
});

const { CommandPalette } = await import("./CommandPalette");
const { api } = await import("../ipc");
const { sessionStore, uiStore } = await import("../store");
const { keymapStore } = await import("../keymap");

const search = () => {
  return screen.getByLabelText(t("paletteSearch"));
};

beforeEach(() => {
  vi.clearAllMocks();
  keymapStore.set({});
  sessionStore.set(null);
  uiStore.set({ ...uiStore.get(), paletteOpen: true, openDialogs: 0, selectedClipIds: [] });
});
afterEach(() => {
  cleanup();
});

describe("CommandPalette", () => {
  it("lists commands with their shortcuts and leaves out shortcut-only ones", () => {
    render(<CommandPalette />);
    expect(screen.getByRole("dialog", { name: t("cmdPalette") })).toBeTruthy();
    expect(screen.getByText(t("undo"))).toBeTruthy();
    expect(screen.getAllByText("Ctrl+Z").length).toBeGreaterThan(0);
    expect(screen.queryByText(t("cmdBack10s"))).toBeNull();
  });

  it("narrows the list as you type, ignoring case", () => {
    render(<CommandPalette />);
    fireEvent.change(search(), { target: { value: t("undo").toUpperCase() } });
    expect(screen.getAllByRole("option").length).toBeLessThan(5);
    expect(screen.getByText(t("undo"))).toBeTruthy();
    fireEvent.change(search(), { target: { value: "zzzz" } });
    expect(screen.getByText(t("paletteNone"))).toBeTruthy();
  });

  it("moves with the arrow keys and runs the highlighted command on Enter", () => {
    render(<CommandPalette />);
    fireEvent.change(search(), { target: { value: t("undo") } });
    fireEvent.keyDown(search(), { key: "ArrowDown" });
    fireEvent.keyDown(search(), { key: "ArrowUp" });
    expect(screen.getAllByRole("option")[0].getAttribute("aria-selected")).toBe("true");
    fireEvent.keyDown(search(), { key: "Enter" });
    expect(api.undo).toHaveBeenCalled();
    expect(uiStore.get().paletteOpen).toBe(false);
  });

  it("runs a command when it is clicked", () => {
    render(<CommandPalette />);
    fireEvent.click(screen.getByText(t("redo")));
    expect(api.redo).toHaveBeenCalled();
  });

  it("does not run a command that is disabled", () => {
    render(<CommandPalette />);
    fireEvent.change(search(), { target: { value: t("delete") } });
    const option = screen.getAllByRole("option").find((item) => {
      return item.textContent?.startsWith(t("delete")) ?? false;
    })!;
    expect(option.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(option);
    expect(uiStore.get().paletteOpen).toBe(true);
  });

  it("shows the bindings the user chose", () => {
    keymapStore.set({ undo: ["Alt+U"] });
    render(<CommandPalette />);
    expect(screen.getByText("Alt+U")).toBeTruthy();
  });
});
