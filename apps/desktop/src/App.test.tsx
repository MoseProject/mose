// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "./i18n";
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
    WatchFolder: () => {
      return <div data-testid="screen-watch" />;
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
vi.mock("./ipc", () => {
  return {
    api: {
      undo: vi.fn().mockResolvedValue(undefined),
      redo: vi.fn().mockResolvedValue(undefined),
      saveProject: vi.fn().mockResolvedValue(undefined),
    },
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

const goTo = (screenName: "home" | "prefs" | "watch" | "editor") => {
  return act(() => {
    return uiStore.set({ ...uiStore.get(), screen: screenName });
  });
};

beforeEach(() => {
  vi.clearAllMocks();
  sessionStore.set(null);
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

    it("shows the editor, preferences, home and watch-folder screens for an open session", () => {
      sessionStore.set(session());
      render(<App />);
      goTo("editor");
      expect(screen.getByTestId("screen-editor")).toBeTruthy();
      goTo("prefs");
      expect(screen.getByTestId("screen-prefs")).toBeTruthy();
      goTo("home");
      expect(screen.getByTestId("screen-home")).toBeTruthy();
      goTo("watch");
      expect(screen.getByTestId("screen-watch")).toBeTruthy();
    });

    it("shows the watch-folder screen without a session", () => {
      uiStore.set({ ...uiStore.get(), screen: "watch" });
      render(<App />);
      expect(screen.getByTestId("screen-watch")).toBeTruthy();
    });

    it("navigates from the top bar and marks the current screen", () => {
      sessionStore.set(session());
      render(<App />);
      fireEvent.click(screen.getByTestId("nav-prefs"));
      expect(uiStore.get().screen).toBe("prefs");
      expect(screen.getByTestId("nav-prefs").getAttribute("aria-current")).toBe("page");
      fireEvent.click(screen.getByTestId("nav-watch"));
      expect(uiStore.get().screen).toBe("watch");
      expect(screen.getByTestId("nav-watch").getAttribute("aria-current")).toBe("page");
      fireEvent.click(screen.getByTestId("nav-home"));
      expect(uiStore.get().screen).toBe("home");
      expect(screen.getByTestId("nav-home").getAttribute("aria-current")).toBe("page");
      expect(screen.getByTestId("nav-prefs").getAttribute("aria-current")).toBeNull();
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

  describe("error banner", () => {
    it("shows the error and dismisses it", () => {
      uiStore.set({ ...uiStore.get(), error: "disk full" });
      render(<App />);
      const banner = screen.getByRole("alert");
      expect(banner.textContent).toContain(`${t("errorPrefix")}: disk full`);
      fireEvent.click(screen.getByLabelText(t("close")));
      expect(uiStore.get().error).toBeNull();
      expect(screen.queryByRole("alert")).toBeNull();
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
