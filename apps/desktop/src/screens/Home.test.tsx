// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { makeSession } from "../testSupport";

vi.mock("../ipc", () => {
  return {
    api: {
      getRecentProjects: vi.fn(),
      newProject: vi.fn().mockResolvedValue(undefined),
      openProject: vi.fn(),
      openProjectPath: vi.fn().mockResolvedValue(undefined),
      restoreAutosave: vi.fn().mockResolvedValue(undefined),
      discardAutosave: vi.fn().mockResolvedValue(undefined),
      removeRecentProject: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { Home } = await import("./Home");
const { api } = await import("../ipc");
const { patchUi, sessionStore, uiStore } = await import("../store");

const recent = (autosavePath: string | null = null) => {
  return {
    path: "C:/p/one.ocproj",
    name: "One",
    modifiedUnixSecs: 1_700_000_000,
    autosavePath,
  };
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getRecentProjects).mockResolvedValue([]);
  sessionStore.set(null);
  patchUi({ screen: "home", selectedClipIds: [3], error: null });
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("home screen", () => {
  it("creates a project and enters the editor with the selection cleared", async () => {
    render(<Home />);
    fireEvent.click(screen.getByTestId("new-project"));
    await waitFor(() => {
      return expect(uiStore.get().screen).toBe("editor");
    });
    expect(api.newProject).toHaveBeenCalledWith(t("untitledProject"), t("defaultSequenceName"));
    expect(uiStore.get().selectedClipIds).toEqual([]);
  });

  it("stays on home when creating fails", async () => {
    vi.mocked(api.newProject).mockRejectedValueOnce(new Error("disk full"));
    render(<Home />);
    fireEvent.click(screen.getByTestId("new-project"));
    await waitFor(() => {
      return expect(uiStore.get().error).toContain("disk full");
    });
    expect(uiStore.get().screen).toBe("home");
  });

  it("opens a project through the dialog only when one was chosen", async () => {
    vi.mocked(api.openProject).mockResolvedValueOnce(false as never);
    render(<Home />);
    fireEvent.click(screen.getByText(t("openProject")));
    await waitFor(() => {
      return expect(api.openProject).toHaveBeenCalledTimes(1);
    });
    expect(uiStore.get().screen).toBe("home");

    vi.mocked(api.openProject).mockResolvedValueOnce(true as never);
    fireEvent.click(screen.getByText(t("openProject")));
    await waitFor(() => {
      return expect(uiStore.get().screen).toBe("editor");
    });
  });

  it("asks before discarding unsaved work and respects a refusal", async () => {
    sessionStore.set({ ...makeSession([{ id: 1, name: "A" }]), dirty: true });
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(<Home />);
    fireEvent.click(screen.getByTestId("new-project"));
    fireEvent.click(screen.getByText(t("openProject")));
    expect(confirm).toHaveBeenCalledWith(t("unsavedConfirm"));
    expect(api.newProject).not.toHaveBeenCalled();
    expect(api.openProject).not.toHaveBeenCalled();

    confirm.mockReturnValue(true);
    fireEvent.click(screen.getByTestId("new-project"));
    await waitFor(() => {
      return expect(api.newProject).toHaveBeenCalled();
    });
  });

  it("offers a way back to the editor once a session exists", () => {
    sessionStore.set(makeSession([{ id: 1, name: "A" }]));
    render(<Home />);
    fireEvent.click(screen.getByTestId("back-to-editor"));
    expect(uiStore.get().screen).toBe("editor");
  });

  it("hides the back button without a session", () => {
    render(<Home />);
    expect(screen.queryByTestId("back-to-editor")).toBeNull();
  });

  it("lists recent projects, opens one and removes one", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent()]);
    render(<Home />);
    fireEvent.click(await screen.findByText("One"));
    await waitFor(() => {
      return expect(api.openProjectPath).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    expect(uiStore.get().screen).toBe("editor");

    fireEvent.click(screen.getByTitle(t("removeRecent")));
    await waitFor(() => {
      return expect(api.removeRecentProject).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    await waitFor(() => {
      return expect(api.getRecentProjects).toHaveBeenCalledTimes(2);
    });
  });

  it("recovers or discards an autosave only when one exists", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent("C:/p/one.autosave")]);
    render(<Home />);
    fireEvent.click(await screen.findByText(t("recoverAutosave")));
    await waitFor(() => {
      return expect(api.restoreAutosave).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    await waitFor(() => {
      return expect(uiStore.get().screen).toBe("editor");
    });

    fireEvent.click(screen.getByText(t("discardAutosave")));
    await waitFor(() => {
      return expect(api.discardAutosave).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    cleanup();

    vi.mocked(api.getRecentProjects).mockResolvedValue([recent()]);
    render(<Home />);
    await screen.findByText("One");
    expect(screen.queryByText(t("recoverAutosave"))).toBeNull();
  });
});
