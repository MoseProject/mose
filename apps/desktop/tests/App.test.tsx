// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../src/i18n";
import { answerConfirm, answerExtra, confirmStore, toastStore } from "../src/notifications";
import { exportsStore, sessionStore, uiStore } from "../src/store";
import { makeSession } from "./support/testSupport";
import type { SessionView } from "../src/types";

vi.mock("../src/screens/Home", () => {
  return {
    Home: () => {
      return <div data-testid="screen-home" />;
    },
  };
});
vi.mock("../src/screens/Prefs", () => {
  return {
    Prefs: () => {
      return <div data-testid="screen-prefs" />;
    },
  };
});
vi.mock("../src/screens/WatchFolder", () => {
  return {
    WatchFolder: ({ open }: { open?: boolean }) => {
      return open ? <div data-testid="screen-watch" /> : null;
    },
  };
});
vi.mock("../src/screens/Editor", () => {
  return {
    Editor: () => {
      return <div data-testid="screen-editor" />;
    },
  };
});
vi.mock("../src/screens/CrashReviewModal", () => {
  return {
    CrashReviewModal: () => {
      return <div data-testid="crash-modal" />;
    },
  };
});
vi.mock("../src/editor/export/ExportDialog", () => {
  return {
    ExportDialog: () => {
      return <div data-testid="export-dialog" />;
    },
  };
});
let closeHandler: (request: { dirty: boolean; exporting: boolean }) => void = () => {};
const stopListeningToClose = vi.fn();

vi.mock("../src/ipc", () => {
  return {
    api: {
      undo: vi.fn().mockResolvedValue(undefined),
      redo: vi.fn().mockResolvedValue(undefined),
      saveProject: vi.fn().mockResolvedValue(undefined),
      quitApp: vi.fn().mockResolvedValue(undefined),
      controlReady: vi.fn().mockResolvedValue(undefined),
      startExport: vi.fn().mockResolvedValue(1),
      exportCanvas: vi.fn().mockRejectedValue("empty"),
      getPrefs: vi.fn().mockResolvedValue({ outputFolder: "" }),
    },
    onCloseRequested: vi.fn(
      (handler: (request: { dirty: boolean; exporting: boolean }) => void) => {
        closeHandler = handler;
        return Promise.resolve(stopListeningToClose);
      },
    ),
    // The control channel's listener; nothing is sent to it in these tests.
    onControlRequest: vi.fn().mockResolvedValue(() => {}),
  };
});
vi.mock("../src/store", async (importOriginal) => {
  return {
    ...(await importOriginal<typeof import("../src/store")>()),
    connect: vi.fn().mockResolvedValue(undefined),
  };
});

const { App } = await import("../src/App");
const { api } = await import("../src/ipc");
const { connect } = await import("../src/store");

const session = (overrides: Record<string, unknown> = {}): SessionView => {
  const view = makeSession([{ id: 1, name: "Main" }]);
  Object.assign(view.project, { name: "My project" });
  Object.assign(view, overrides);
  return view;
};

const goTo = (screenName: "home" | "editor") => {
  return act(() => {
    return uiStore.set({ ...uiStore.get(), screen: screenName });
  });
};

beforeEach(() => {
  vi.clearAllMocks();
  sessionStore.set(null);
  toastStore.set([]);
  uiStore.set({
    screen: "home",
    selectedClipIds: [],
    tool: "select",
    pxPerSec: 40,
    error: null,
    drawingShapePoints: null,
    exportQueueOpen: false,
    exportDialogOpen: false,
    exportClipIds: null,
    openDialogs: 0,
    markIn: null,
    markOut: null,
    snapEnabled: true,
    shortcutsOpen: false,
    paletteOpen: false,
    settingsOpen: false,
    watchOpen: false,
    monitorZoom: "fit",
    monitorGuides: false,
    scopesOpen: false,
    scopeKind: "waveform",
  });
});
afterEach(() => {
  return cleanup();
});

describe("app shell", () => {
  it("connects to the backend once on mount", () => {
    render(<App />);
    expect(connect).toHaveBeenCalledTimes(1);
  });

  it("reports a failed connection in the error banner", async () => {
    vi.mocked(connect).mockRejectedValueOnce(new Error("backend down"));
    render(<App />);
    await act(async () => {});
    expect(screen.getByRole("alert").textContent).toContain("backend down");
  });

  it("renders the crash review modal on every screen", () => {
    render(<App />);
    expect(screen.getByTestId("crash-modal")).toBeTruthy();
  });

  describe("screens", () => {
    it("shows home without a session even when another screen is selected", () => {
      uiStore.set({ ...uiStore.get(), screen: "editor" });
      render(<App />);
      expect(screen.getByTestId("screen-home")).toBeTruthy();
    });

    it("shows the editor and home for an open session", async () => {
      sessionStore.set(session());
      render(<App />);
      goTo("editor");
      expect(await screen.findByTestId("screen-editor")).toBeTruthy();
      goTo("home");
      expect(screen.getByTestId("screen-home")).toBeTruthy();
    });

    it("opens settings as a dialog over whatever screen is showing", async () => {
      render(<App />);
      expect(screen.queryByTestId("screen-prefs")).toBeNull();
      act(() => {
        return uiStore.set({ ...uiStore.get(), settingsOpen: true });
      });
      expect(await screen.findByRole("dialog", { name: t("preferences") })).toBeTruthy();
      expect(await screen.findByTestId("screen-prefs")).toBeTruthy();
    });

    it("shows the watch-folder dialog without a session", async () => {
      uiStore.set({ ...uiStore.get(), watchOpen: true });
      render(<App />);
      expect(await screen.findByTestId("screen-watch")).toBeTruthy();
    });

    it("navigates from the View menu", () => {
      sessionStore.set(session());
      render(<App />);
      fireEvent.click(screen.getByTestId("nav-prefs"));
      expect(uiStore.get().settingsOpen).toBe(true);
      fireEvent.click(screen.getByTestId("nav-watch"));
      expect(uiStore.get().watchOpen).toBe(true);
      fireEvent.click(screen.getByTestId("nav-home"));
      expect(uiStore.get().screen).toBe("home");
      expect(screen.getByTestId("nav-home").getAttribute("aria-current")).toBe("page");
    });

    it("opens the shortcut sheet and the command palette from the View menu", async () => {
      sessionStore.set(session());
      render(<App />);
      fireEvent.click(screen.getByText(t("cmdShortcuts"), { selector: "button" }));
      expect(await screen.findByRole("dialog", { name: t("cmdShortcuts") })).toBeTruthy();
      cleanup();
      uiStore.set({ ...uiStore.get(), shortcutsOpen: false, paletteOpen: true });
      render(<App />);
      expect(await screen.findByRole("dialog", { name: t("cmdPalette") })).toBeTruthy();
    });
  });

  describe("project bar", () => {
    it("is hidden without a session", () => {
      render(<App />);
      expect(screen.queryByTestId("save-project")).toBeNull();
      expect(screen.queryByTestId("export")).toBeNull();
    });

    it("shows the project name and the unsaved marker only when dirty", () => {
      sessionStore.set(session({ dirty: false }));
      uiStore.set({ ...uiStore.get(), screen: "editor" });
      render(<App />);
      expect(screen.getByText(/My project/)).toBeTruthy();
      expect(screen.queryByLabelText(t("unsaved"))).toBeNull();
      act(() => {
        return sessionStore.set(session({ dirty: true }));
      });
      expect(screen.getByLabelText(t("unsaved"))).toBeTruthy();
    });

    it("enables undo and redo only when there is something to undo or redo", () => {
      sessionStore.set(session({ canUndo: false, canRedo: false }));
      uiStore.set({ ...uiStore.get(), screen: "editor" });
      render(<App />);
      expect((screen.getByTestId("undo") as HTMLButtonElement).disabled).toBe(true);
      expect((screen.getByTestId("redo") as HTMLButtonElement).disabled).toBe(true);
      act(() => {
        return sessionStore.set(session({ canUndo: true, canRedo: true }));
      });
      expect((screen.getByTestId("undo") as HTMLButtonElement).disabled).toBe(false);
      expect((screen.getByTestId("redo") as HTMLButtonElement).disabled).toBe(false);
    });

    it("undoes, redoes and saves", () => {
      sessionStore.set(session({ canUndo: true, canRedo: true }));
      uiStore.set({ ...uiStore.get(), screen: "editor" });
      render(<App />);
      fireEvent.click(screen.getByTestId("undo"));
      fireEvent.click(screen.getByTestId("redo"));
      fireEvent.click(screen.getByTestId("save-project"));
      expect(api.undo).toHaveBeenCalled();
      expect(api.redo).toHaveBeenCalled();
      expect(api.saveProject).toHaveBeenCalledWith(false);
    });

    it("opens the export options dialog instead of exporting at once", async () => {
      sessionStore.set(session());
      uiStore.set({ ...uiStore.get(), screen: "editor" });
      render(<App />);
      fireEvent.click(screen.getByTestId("export"));
      expect(uiStore.get().exportDialogOpen).toBe(true);
      expect(await screen.findByTestId("export-dialog")).toBeTruthy();
      expect(api.startExport).not.toHaveBeenCalled();
    });
  });

  describe("error toasts", () => {
    it("turns an error into a dismissable toast and clears the pending error", () => {
      uiStore.set({ ...uiStore.get(), error: "disk full" });
      render(<App />);
      const toast = screen.getByRole("alert");
      expect(toast.textContent).toContain(t("errorPrefix") + ": disk full");
      expect(uiStore.get().error).toBeNull();
      fireEvent.click(screen.getByLabelText(t("close")));
      expect(screen.queryByRole("alert")).toBeNull();
    });

    it("keeps a second error instead of replacing the first", () => {
      render(<App />);
      act(() => {
        return uiStore.set({ ...uiStore.get(), error: "first" });
      });
      act(() => {
        return uiStore.set({ ...uiStore.get(), error: "second" });
      });
      expect(screen.getAllByRole("alert")).toHaveLength(2);
    });

    it("is absent when there is no error", () => {
      render(<App />);
      expect(screen.queryByRole("alert")).toBeNull();
    });
  });
});

describe("export queue popover", () => {
  it("stays closed until toggled, and shows the active job count", async () => {
    sessionStore.set(session());
    uiStore.set({ ...uiStore.get(), screen: "editor" });
    exportsStore.set([
      {
        id: 1,
        title: "a.mp4",
        outputPath: "a",
        status: "Queued",
        percent: 0,
        elapsedSecs: null,
      },
    ]);
    render(<App />);
    expect(screen.queryByText(t("noExports"))).toBeNull();
    const toggle = screen.getByTestId("export-queue-toggle");
    expect(toggle.textContent).toContain("1");
    fireEvent.click(toggle);
    expect(uiStore.get().exportQueueOpen).toBe(true);
    expect(await screen.findByRole("dialog", { name: t("exportQueue") })).toBeTruthy();
    exportsStore.set([]);
  });
});

describe("closing the window", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(api.saveProject).mockResolvedValue(true as never);
    confirmStore.set(null);
  });

  it("offers save and quit, quit without saving or staying when changes would be lost", async () => {
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: false });
    });
    const request = confirmStore.get()!;
    expect(request.title).toBe(t("confirmQuitTitle"));
    expect(request.message).toBe(t("confirmQuitDirty"));
    expect(request.confirmLabel).toBe(t("confirmQuitSave"));
    expect(request.extraLabel).toBe(t("confirmQuitAction"));
    expect(api.quitApp).not.toHaveBeenCalled();
  });

  it("saves first and then quits", async () => {
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: false });
    });
    await act(async () => {
      answerConfirm(true);
    });
    expect(api.saveProject).toHaveBeenCalledWith(false);
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });

  it("stays open when the save dialog is cancelled", async () => {
    vi.mocked(api.saveProject).mockResolvedValue(false as never);
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: false });
    });
    await act(async () => {
      answerConfirm(true);
    });
    expect(api.saveProject).toHaveBeenCalled();
    expect(api.quitApp).not.toHaveBeenCalled();
  });

  it("stays open when the save fails", async () => {
    vi.mocked(api.saveProject).mockRejectedValue("disk full");
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: false });
    });
    await act(async () => {
      answerConfirm(true);
    });
    expect(api.quitApp).not.toHaveBeenCalled();
  });

  it("quits without saving when asked to", async () => {
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: false });
    });
    await act(async () => {
      answerExtra();
    });
    expect(api.saveProject).not.toHaveBeenCalled();
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });

  it("stays open and saves nothing when the question is dismissed", async () => {
    render(<App />);
    act(() => {
      closeHandler({ dirty: true, exporting: true });
    });
    expect(confirmStore.get()!.message).toBe(
      t("confirmQuitDirty") + " " + t("confirmQuitExporting"),
    );
    await act(async () => {
      answerConfirm(false);
    });
    expect(api.saveProject).not.toHaveBeenCalled();
    expect(api.quitApp).not.toHaveBeenCalled();
  });

  it("only asks to quit anyway when nothing is unsaved but an export is running", async () => {
    render(<App />);
    act(() => {
      closeHandler({ dirty: false, exporting: true });
    });
    const request = confirmStore.get()!;
    expect(request.extraLabel).toBeUndefined();
    expect(request.message).toBe(t("confirmQuitExporting"));
    await act(async () => {
      answerConfirm(true);
    });
    expect(api.saveProject).not.toHaveBeenCalled();
    expect(api.quitApp).toHaveBeenCalledTimes(1);
  });

  it("stops listening when the app goes away", async () => {
    const { unmount } = render(<App />);
    unmount();
    await Promise.resolve();
    expect(stopListeningToClose).toHaveBeenCalled();
  });
});
