// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "./i18n";
import { answerConfirm, answerExtra, confirmStore, toastStore } from "./notifications";
import { exportsStore, sessionStore, uiStore } from "./store";
import { makeSession } from "./testSupport";
import type { SessionView } from "./types";

vi.mock("./screens/Home", () => {
  return {
    Home: () => {
      return <div data-testid="screen-home" />;
    },
  };
});
vi.mock("./screens/Prefs", () => {
  return {
    Prefs: () => {
      return <div data-testid="screen-prefs" />;
    },
  };
});
vi.mock("./screens/WatchFolder", () => {
  return {
    WatchFolder: ({ open }: { open?: boolean }) => {
      return open ? <div data-testid="screen-watch" /> : null;
    },
  };
});
vi.mock("./screens/Editor", () => {
  return {
    Editor: () => {
      return <div data-testid="screen-editor" />;
    },
  };
});
vi.mock("./screens/CrashReviewModal", () => {
  return {
    CrashReviewModal: () => {
      return <div data-testid="crash-modal" />;
    },
  };
});
vi.mock("./editor/ExportDialog", () => {
  return {
    ExportDialog: () => {
      return <div data-testid="export-dialog" />;
    },
  };
});
let closeHandler: (request: { dirty: boolean; exporting: boolean }) => void = () => {};
const stopListeningToClose = vi.fn();

vi.mock("./ipc", () => {
  return {
    api: {
      undo: vi.fn().mockResolvedValue(undefined),
      redo: vi.fn().mockResolvedValue(undefined),
      saveProject: vi.fn().mockResolvedValue(undefined),
      quitApp: vi.fn().mockResolvedValue(undefined),
    },
    onCloseRequested: vi.fn(
      (handler: (request: { dirty: boolean; exporting: boolean }) => void) => {
        closeHandler = handler;
        return Promise.resolve(stopListeningToClose);
      },
    ),
    exportActiveSequence: vi.fn().mockResolvedValue(undefined),
  };
});
vi.mock("./store", async (importOriginal) => {
  return {
    ...(await importOriginal<typeof import("./store")>()),
    connect: vi.fn().mockResolvedValue(undefined),
  };
});

const { App } = await import("./App");
const { api, exportActiveSequence } = await import("./ipc");
const { connect } = await import("./store");

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

    it("shows the editor and home for an open session", () => {
      sessionStore.set(session());
      render(<App />);
      goTo("editor");
      expect(screen.getByTestId("screen-editor")).toBeTruthy();
      goTo("home");
      expect(screen.getByTestId("screen-home")).toBeTruthy();
    });

    it("opens settings as a dialog over whatever screen is showing", () => {
      render(<App />);
      expect(screen.queryByTestId("screen-prefs")).toBeNull();
      act(() => {
        return uiStore.set({ ...uiStore.get(), settingsOpen: true });
      });
      expect(screen.getByRole("dialog", { name: t("preferences") })).toBeTruthy();
      expect(screen.getByTestId("screen-prefs")).toBeTruthy();
    });

    it("shows the watch-folder dialog without a session", () => {
      uiStore.set({ ...uiStore.get(), watchOpen: true });
      render(<App />);
      expect(screen.getByTestId("screen-watch")).toBeTruthy();
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

    it("opens the shortcut sheet and the command palette from the View menu", () => {
      sessionStore.set(session());
      render(<App />);
      fireEvent.click(screen.getByText(t("cmdShortcuts"), { selector: "button" }));
      expect(screen.getByRole("dialog", { name: t("cmdShortcuts") })).toBeTruthy();
      cleanup();
      uiStore.set({ ...uiStore.get(), shortcutsOpen: false, paletteOpen: true });
      render(<App />);
      expect(screen.getByRole("dialog", { name: t("cmdPalette") })).toBeTruthy();
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
      render(<App />);
      fireEvent.click(screen.getByTestId("undo"));
      fireEvent.click(screen.getByTestId("redo"));
      fireEvent.click(screen.getByTestId("save-project"));
      expect(api.undo).toHaveBeenCalled();
      expect(api.redo).toHaveBeenCalled();
      expect(api.saveProject).toHaveBeenCalledWith(false);
    });

    it("opens the export options dialog instead of exporting at once", () => {
      sessionStore.set(session());
      render(<App />);
      fireEvent.click(screen.getByTestId("export"));
      expect(uiStore.get().exportDialogOpen).toBe(true);
      expect(screen.getByTestId("export-dialog")).toBeTruthy();
      expect(exportActiveSequence).not.toHaveBeenCalled();
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
  it("stays closed until toggled, and shows the active job count", () => {
    sessionStore.set(session());
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
    expect(screen.getByRole("dialog", { name: t("exportQueue") })).toBeTruthy();
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
