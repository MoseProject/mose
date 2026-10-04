// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConfirmHost } from "../editor/Notifications";
import { t } from "../i18n";
import { makeSession } from "../testSupport";

vi.mock("../ipc", () => {
  return {
    api: {
      getRecentProjects: vi.fn(),
      createProject: vi.fn().mockResolvedValue(true),
      openProject: vi.fn(),
      openProjectPath: vi.fn().mockResolvedValue(undefined),
      restoreAutosave: vi.fn().mockResolvedValue(undefined),
      discardAutosave: vi.fn().mockResolvedValue(undefined),
      removeRecentProject: vi.fn().mockResolvedValue(undefined),
      revealInFolder: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { Home, visibleProjects } = await import("./Home");
const { api } = await import("../ipc");
const { patchUi, sessionStore, uiStore } = await import("../store");

const recent = (autosavePath: string | null = null) => {
  return {
    path: "C:/p/one.ocproj",
    name: "One",
    modifiedUnixSecs: 1_700_000_000,
    autosavePath,
    folder: "C:/p",
    sequenceCount: 2,
    mediaCount: 5,
    durationSecs: 75,
  };
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getRecentProjects).mockResolvedValue([]);
  vi.mocked(api.createProject).mockResolvedValue(true);
  vi.mocked(api.openProjectPath).mockResolvedValue(undefined);
  sessionStore.set(null);
  patchUi({
    screen: "home",
    selectedClipIds: [3],
    error: null,
    settingsOpen: false,
    watchOpen: false,
  });
});
afterEach(() => {
  cleanup();
});

describe("home screen", () => {
  const startCreating = () => {
    fireEvent.click(screen.getByTestId("new-project"));
    return screen.findByRole("dialog", { name: t("newProjectTitle") });
  };

  it("shows an empty state with a way to create the first project", async () => {
    render(<Home />);
    expect(await screen.findByTestId("projects-empty")).toBeTruthy();
    expect(screen.queryByLabelText(t("searchProjects"))).toBeNull();
  });

  it("asks for a name, creates the project and enters the editor with the selection cleared", async () => {
    render(<Home />);
    await startCreating();
    const input = screen.getByLabelText(t("projectName")) as HTMLInputElement;
    expect(input.value).toBe(t("untitledProject"));
    fireEvent.change(input, { target: { value: "  Boss fight  " } });
    fireEvent.click(screen.getByText(t("createProject")));

    await waitFor(() => {
      return expect(uiStore.get().screen).toBe("editor");
    });
    expect(api.createProject).toHaveBeenCalledWith("Boss fight", t("defaultSequenceName"));
    expect(uiStore.get().selectedClipIds).toEqual([]);
  });

  it("creates with Enter and refuses a blank name", async () => {
    render(<Home />);
    await startCreating();
    const input = screen.getByLabelText(t("projectName"));
    fireEvent.change(input, { target: { value: "   " } });
    expect((screen.getByText(t("createProject")) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByRole("alert").textContent).toBe(t("projectNameEmpty"));
    fireEvent.keyDown(input, { key: "Enter" });
    expect(api.createProject).not.toHaveBeenCalled();

    fireEvent.change(input, { target: { value: "Run" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() => {
      return expect(api.createProject).toHaveBeenCalledWith("Run", t("defaultSequenceName"));
    });
  });

  it("stays on the projects screen when the save dialog is cancelled", async () => {
    vi.mocked(api.createProject).mockResolvedValue(false);
    render(<Home />);
    await startCreating();
    fireEvent.click(screen.getByText(t("createProject")));
    await waitFor(() => {
      return expect(api.createProject).toHaveBeenCalled();
    });
    expect(uiStore.get().screen).toBe("home");
    expect(screen.getByRole("dialog", { name: t("newProjectTitle") })).toBeTruthy();
  });

  it("stays on the projects screen and reports it when creating fails", async () => {
    vi.mocked(api.createProject).mockRejectedValueOnce(new Error("disk full"));
    render(<Home />);
    await startCreating();
    fireEvent.click(screen.getByText(t("createProject")));
    await waitFor(() => {
      return expect(uiStore.get().error).toContain("disk full");
    });
    expect(uiStore.get().screen).toBe("home");
  });

  it("closes the name dialog without creating anything", async () => {
    render(<Home />);
    await startCreating();
    fireEvent.click(screen.getByText(t("cancel")));
    await waitFor(() => {
      return expect(screen.queryByRole("dialog")).toBeNull();
    });
    expect(api.createProject).not.toHaveBeenCalled();
  });

  it("opens a project through the file dialog only when one was chosen", async () => {
    vi.mocked(api.openProject).mockResolvedValueOnce(false as never);
    render(<Home />);
    fireEvent.click(screen.getByTestId("open-project"));
    await waitFor(() => {
      return expect(api.openProject).toHaveBeenCalledTimes(1);
    });
    expect(uiStore.get().screen).toBe("home");

    vi.mocked(api.openProject).mockResolvedValueOnce(true as never);
    fireEvent.click(screen.getByTestId("open-project"));
    await waitFor(() => {
      return expect(uiStore.get().screen).toBe("editor");
    });
  });

  it("asks before discarding unsaved work and respects a refusal", async () => {
    sessionStore.set({ ...makeSession([{ id: 1, name: "A" }]), dirty: true });
    render(
      <>
        <Home />
        <ConfirmHost />
      </>,
    );
    await startCreating();
    fireEvent.click(screen.getByText(t("createProject")));
    const question = await screen.findByRole("dialog", { name: t("confirmDiscardTitle") });
    expect(screen.getByText(t("unsavedConfirm"))).toBeTruthy();
    fireEvent.click(question.querySelector("button.icon")!);
    await waitFor(() => {
      return expect(screen.queryByRole("dialog", { name: t("confirmDiscardTitle") })).toBeNull();
    });
    expect(api.createProject).not.toHaveBeenCalled();

    fireEvent.click(screen.getByText(t("cancel")));
    fireEvent.click(screen.getByTestId("open-project"));
    fireEvent.click(await screen.findByText(t("confirmDiscardAction")));
    await waitFor(() => {
      return expect(api.openProject).toHaveBeenCalled();
    });
  });

  it("offers a way back to the editor, naming the open project, once a session exists", () => {
    sessionStore.set(makeSession([{ id: 1, name: "A" }]));
    render(<Home />);
    expect(screen.getByLabelText(t("continueEditing"))).toBeTruthy();
    fireEvent.click(screen.getByTestId("back-to-editor"));
    expect(uiStore.get().screen).toBe("editor");
  });

  it("hides the back button without a session", () => {
    render(<Home />);
    expect(screen.queryByTestId("back-to-editor")).toBeNull();
  });

  it("keeps the settings and the watch folder one click away", () => {
    render(<Home />);
    fireEvent.click(screen.getByTestId("home-settings"));
    expect(uiStore.get().settingsOpen).toBe(true);
    fireEvent.click(screen.getByTestId("home-watch"));
    expect(uiStore.get().watchOpen).toBe(true);
  });

  it("lists each project with its folder, length and counts, and opens one", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent()]);
    render(<Home />);
    const row = await screen.findByLabelText(t("openProjectNamed") + " One");
    expect(row.textContent).toContain("C:/p");
    expect(row.textContent).toContain("2 " + t("projectSequences"));
    expect(row.textContent).toContain("5 " + t("projectMediaCount"));
    fireEvent.click(row);
    await waitFor(() => {
      return expect(api.openProjectPath).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    expect(uiStore.get().screen).toBe("editor");
  });

  it("stays on the list and reloads it when a project cannot be opened", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent()]);
    vi.mocked(api.openProjectPath).mockRejectedValueOnce("projectMissing");
    render(<Home />);
    fireEvent.click(await screen.findByLabelText(t("openProjectNamed") + " One"));
    await waitFor(() => {
      return expect(api.getRecentProjects).toHaveBeenCalledTimes(2);
    });
    expect(uiStore.get().screen).toBe("home");
    expect(uiStore.get().error).toBe(t("projectMissing"));
  });

  it("shows a project in its folder and removes it from the list", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent()]);
    render(<Home />);
    fireEvent.click(await screen.findByLabelText(t("showInFolder") + ": One"));
    expect(api.revealInFolder).toHaveBeenCalledWith("C:/p/one.ocproj");

    fireEvent.click(screen.getByLabelText(t("removeRecent") + ": One"));
    await waitFor(() => {
      return expect(api.removeRecentProject).toHaveBeenCalledWith("C:/p/one.ocproj");
    });
    await waitFor(() => {
      return expect(api.getRecentProjects).toHaveBeenCalledTimes(2);
    });
  });

  it("filters the list by name or folder and says so when nothing matches", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([
      recent(),
      { ...recent(), path: "D:/x/two.ocproj", name: "Two", folder: "D:/x" },
    ]);
    render(<Home />);
    await screen.findByText("One");
    const search = screen.getByLabelText(t("searchProjects"));
    fireEvent.change(search, { target: { value: "two" } });
    expect(screen.queryByText("One")).toBeNull();
    expect(screen.getByText("Two")).toBeTruthy();
    fireEvent.change(search, { target: { value: "D:/x" } });
    expect(screen.getByText("Two")).toBeTruthy();
    fireEvent.change(search, { target: { value: "zzz" } });
    expect(screen.getByText(t("projectsNoMatches"))).toBeTruthy();
  });

  it("recovers or discards an autosave only when one exists", async () => {
    vi.mocked(api.getRecentProjects).mockResolvedValue([recent("C:/p/one.autosave")]);
    render(<Home />);
    expect(await screen.findByText(t("autosaveAvailable"))).toBeTruthy();
    fireEvent.click(screen.getByText(t("recoverAutosave")));
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

describe("visibleProjects", () => {
  const make = (name: string, modifiedUnixSecs: number, folder = "C:/p") => {
    return { ...recent(), name, modifiedUnixSecs, folder, path: `${folder}/${name}.ocproj` };
  };
  const list = [make("beta", 10), make("Alpha", 30), make("gamma", 20, "D:/games")];
  const names = (projects: { name: string }[]) => {
    return projects.map((project) => {
      return project.name;
    });
  };

  it("keeps the order it was given for recent", () => {
    expect(names(visibleProjects(list, "", "recent"))).toEqual(["beta", "Alpha", "gamma"]);
  });

  it("orders by last modified, newest first", () => {
    expect(names(visibleProjects(list, "", "modified"))).toEqual(["Alpha", "gamma", "beta"]);
  });

  it("orders by name regardless of case", () => {
    expect(names(visibleProjects(list, "", "name"))).toEqual(["Alpha", "beta", "gamma"]);
  });

  it("filters on name and folder without caring about case or surrounding spaces", () => {
    expect(names(visibleProjects(list, "  ALPHA ", "recent"))).toEqual(["Alpha"]);
    expect(names(visibleProjects(list, "games", "recent"))).toEqual(["gamma"]);
    expect(visibleProjects(list, "nothing", "recent")).toEqual([]);
  });

  it("does not change the list it was given", () => {
    const before = names(list);
    visibleProjects(list, "", "name");
    expect(names(list)).toEqual(before);
  });
});
