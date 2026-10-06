// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../src/i18n";
import { keymapStore } from "../src/keymap";
import { exportsStore, previewStore, uiStore } from "../src/store";
import { makeSession } from "./support/testSupport";

vi.mock("../src/ipc", () => {
  const names = [
    "undo",
    "redo",
    "saveProject",
    "importMediaDialog",
    "addSequence",
    "addTrack",
    "splitAtPlayhead",
    "mergeClipsIntoComposite",
    "createCompoundClip",
    "addMarker",
    "copyClip",
  ];
  return {
    api: Object.fromEntries(
      names.map((name) => {
        return [name, vi.fn().mockResolvedValue(undefined)];
      }),
    ),
  };
});

const { TopBar } = await import("../src/TopBar");
const { api } = await import("../src/ipc");

const session = (overrides: Record<string, unknown> = {}) => {
  const view = makeSession([{ id: 1, name: "Cut A" }]);
  Object.assign(view.project, {
    name: "Doc",
    media_library: [{ id: 1, kind: "Video", resolution: [3840, 2160], fps: 23.976 }],
  });
  return Object.assign(view, { canUndo: true, canRedo: false, ...overrides });
};

beforeEach(() => {
  vi.clearAllMocks();
  keymapStore.set({});
  exportsStore.set([]);
  previewStore.set({ ...previewStore.get(), playheadSecs: 0 });
  uiStore.set({
    ...uiStore.get(),
    selectedClipIds: [],
    exportDialogOpen: false,
    settingsOpen: false,
    watchOpen: false,
  });
});
afterEach(() => {
  cleanup();
});

describe("TopBar without a project", () => {
  it("shows only the View menu and the settings button", () => {
    render(<TopBar session={null} />);
    expect(screen.getByText(t("menuView"))).toBeTruthy();
    expect(screen.queryByText(t("menuFile"))).toBeNull();
    expect(screen.queryByTestId("export")).toBeNull();
    expect(screen.getByLabelText(t("preferences"))).toBeTruthy();
  });
});

describe("TopBar with a project", () => {
  it("names the project and sequence and marks unsaved work", () => {
    render(<TopBar session={session({ dirty: true })} />);
    expect(screen.getByText(new RegExp(t("projectLabel") + ": Doc"))).toBeTruthy();
    expect(screen.getByText(new RegExp(t("sequence") + ": Cut A"))).toBeTruthy();
    expect(screen.getByLabelText(t("unsaved"))).toBeTruthy();
  });

  it("shows the save state: saving while edits wait, saved once written, unsaved without a file", () => {
    const { rerender } = render(
      <TopBar session={{ ...session({ dirty: true }), filePath: "C:/p/doc.ocproj" }} />,
    );
    expect(screen.getByText(t("savingProject"))).toBeTruthy();
    expect(screen.queryByLabelText(t("unsaved"))).toBeNull();

    rerender(<TopBar session={{ ...session({ dirty: false }), filePath: "C:/p/doc.ocproj" }} />);
    expect(screen.getByText(t("projectSaved"))).toBeTruthy();

    rerender(<TopBar session={session({ dirty: false })} />);
    expect(screen.queryByText(t("projectSaved"))).toBeNull();
    rerender(<TopBar session={session({ dirty: true })} />);
    expect(screen.getByLabelText(t("unsaved"))).toBeTruthy();
  });

  it("reads the timecode, frame rate and resolution of the source", () => {
    previewStore.set({ ...previewStore.get(), playheadSecs: 134 + 8 / 24 });
    render(<TopBar session={session()} />);
    const readout = screen.getByLabelText(t("timecode"));
    expect(readout.textContent).toContain("00:02:14:");
    expect(readout.textContent).toContain("24 FPS");
    expect(readout.textContent).toContain("3840 × 2160");
  });

  it("builds menu items from the command table, with their shortcuts", () => {
    render(<TopBar session={session()} />);
    const undo = screen.getByText(t("undo"), { selector: ".menu-pop button" });
    expect(undo.getAttribute("data-shortcut")).toBe("Ctrl+Z");
    fireEvent.click(undo);
    expect(api.undo).toHaveBeenCalled();
  });

  it("disables menu items that need a selection until there is one", () => {
    const { rerender } = render(<TopBar session={session()} />);
    const copy = () => {
      return screen.getByText(t("menuCopy"), { selector: ".menu-pop button" }) as HTMLButtonElement;
    };
    expect(copy().disabled).toBe(true);
    uiStore.set({ ...uiStore.get(), selectedClipIds: [4] });
    rerender(<TopBar session={session()} />);
    expect(copy().disabled).toBe(false);
  });

  it("follows the user's shortcut changes", () => {
    keymapStore.set({ undo: ["Alt+U"] });
    render(<TopBar session={session()} />);
    expect(
      screen.getByText(t("undo"), { selector: ".menu-pop button" }).getAttribute("data-shortcut"),
    ).toBe("Alt+U");
    expect(screen.getByTestId("undo").getAttribute("title")).toContain("Alt+U");
  });

  it("opens the export dialog from the button and the File menu", () => {
    render(<TopBar session={session()} />);
    fireEvent.click(screen.getByTestId("export"));
    expect(uiStore.get().exportDialogOpen).toBe(true);
    uiStore.set({ ...uiStore.get(), exportDialogOpen: false });
    fireEvent.click(screen.getByText(t("export"), { selector: ".menu-pop button" }));
    expect(uiStore.get().exportDialogOpen).toBe(true);
  });

  it("opens settings and the watch folder as dialogs from the View menu", () => {
    render(<TopBar session={session()} />);
    fireEvent.click(screen.getByTestId("nav-prefs"));
    fireEvent.click(screen.getByTestId("nav-watch"));
    expect(uiStore.get().settingsOpen).toBe(true);
    expect(uiStore.get().watchOpen).toBe(true);
  });

  it("badges the export queue with the running jobs only", () => {
    exportsStore.set([
      { id: 1, title: "a", outputPath: "a", status: "Queued", percent: 0, elapsedSecs: null },
      { id: 2, title: "b", outputPath: "b", status: "Done", percent: 100, elapsedSecs: 3 },
    ]);
    render(<TopBar session={session()} />);
    expect(screen.getByTestId("export-queue-toggle").textContent).toBe("1");
  });

  it("adds a sequence named after the count and runs track and clip actions", () => {
    render(<TopBar session={session()} />);
    fireEvent.click(screen.getByText(t("addSequence"), { selector: ".menu-pop button" }));
    expect(api.addSequence).toHaveBeenCalledWith(t("sequence") + " 2");
    fireEvent.click(screen.getByText(t("addAudioTrack"), { selector: ".menu-pop button" }));
    expect(api.addTrack).toHaveBeenCalledWith("Audio");
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] });
    cleanup();
    render(<TopBar session={session()} />);
    fireEvent.click(screen.getByText(t("mergeComposite"), { selector: ".menu-pop button" }));
    expect(api.mergeClipsIntoComposite).toHaveBeenCalledWith([1, 2]);
    fireEvent.click(screen.getByText(t("compoundClip"), { selector: ".menu-pop button" }));
    expect(api.createCompoundClip).toHaveBeenCalledWith(1, [1, 2]);
  });
});
