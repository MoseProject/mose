// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { answerConfirm, confirmStore, toastStore } from "../notifications";
import type { SessionView, YoutubeDownloadEvent } from "../types";

let emitDownload: (event: YoutubeDownloadEvent) => void = () => {};
const stopListening = vi.fn();

vi.mock("../ipc", () => {
  return {
    api: {
      importMediaDialog: vi.fn().mockResolvedValue(undefined),
      importMedia: vi.fn().mockResolvedValue(undefined),
      generateTts: vi.fn().mockResolvedValue(undefined),
      listSoundLibrary: vi.fn(),
      isYtDlpAvailable: vi.fn(),
      downloadYoutube: vi.fn().mockResolvedValue(undefined),
      cancelYoutubeDownload: vi.fn().mockResolvedValue(undefined),
      addAssetToTimeline: vi.fn().mockResolvedValue(undefined),
      transcribeAsset: vi.fn().mockResolvedValue(undefined),
      toggleFavorite: vi.fn().mockResolvedValue(undefined),
      removeAsset: vi.fn().mockResolvedValue(undefined),
    },
    onYoutubeDownload: vi.fn((handler: (event: YoutubeDownloadEvent) => void) => {
      emitDownload = handler;
      return Promise.resolve(stopListening);
    }),
    thumbnailUrl: (id: number) => {
      return `thumb://${id}`;
    },
  };
});

const { MediaLibrary } = await import("./MediaLibrary");
const { api } = await import("../ipc");
const { assetDragStore, transcribingAssetIdStore } = await import("../store");

const asset = (overrides: Record<string, unknown> = {}) => {
  return {
    id: 1,
    kind: "Video",
    file_name: "clip.mp4",
    source_path: "C:/media/clip.mp4",
    duration_secs: 65,
    resolution: [1920, 1080],
    fps: 59.94,
    loudness: { integrated_lufs: -18.04 },
    has_audio: true,
    favorited: false,
    ...overrides,
  };
};

const sessionWith = (assets: unknown[]) => {
  return { project: { media_library: assets } } as unknown as SessionView;
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.isYtDlpAvailable).mockResolvedValue(true);
  vi.mocked(api.listSoundLibrary).mockResolvedValue([]);
  transcribingAssetIdStore.set(null);
  assetDragStore.set(null);
});
afterEach(() => {
  return cleanup();
});

describe("media library", () => {
  it("shows an empty state and imports through the dialog", () => {
    render(<MediaLibrary session={sessionWith([])} />);
    expect(screen.getByText(t("libraryEmpty"))).toBeTruthy();
    fireEvent.click(screen.getAllByText(t("importFiles"))[0]);
    expect(api.importMediaDialog).toHaveBeenCalled();
  });

  it("describes a video asset with its metadata and thumbnail", () => {
    const { container } = render(<MediaLibrary session={sessionWith([asset()])} />);
    expect(screen.getByText("clip.mp4")).toBeTruthy();
    expect(screen.getByText(/1920×1080 · 60 fps · -18\.0 LUFS/)).toBeTruthy();
    expect(container.querySelector("img")!.getAttribute("src")).toBe("thumb://1");
  });

  it("falls back to a glyph when the thumbnail fails, and for audio assets", () => {
    const { container } = render(
      <MediaLibrary
        session={sessionWith([
          asset(),
          asset({
            id: 2,
            kind: "Audio",
            file_name: "song.mp3",
            resolution: null,
            fps: null,
            loudness: null,
          }),
        ])}
      />,
    );
    fireEvent.error(container.querySelector("img")!);
    expect(container.querySelector("img")).toBeNull();
    expect(container.querySelectorAll(".thumb svg")).toHaveLength(1);
    expect(container.querySelectorAll(".audio-icon svg")).toHaveLength(1);
    expect(screen.getByText(t("analyzing"), { exact: false })).toBeTruthy();
  });

  it("adds an asset to the timeline on double click and toggles its favorite", () => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    fireEvent.doubleClick(screen.getByText("clip.mp4"));
    expect(api.addAssetToTimeline).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByTitle(t("favorite")));
    expect(api.toggleFavorite).toHaveBeenCalledWith(1);
    cleanup();

    render(<MediaLibrary session={sessionWith([asset({ favorited: true })])} />);
    expect(screen.getByTitle(t("favorite")).querySelector("svg")?.getAttribute("fill")).toBe(
      "currentColor",
    );
  });

  it("transcribes assets with audio, locking the button while one runs", () => {
    render(<MediaLibrary session={sessionWith([asset(), asset({ id: 2, has_audio: false })])} />);
    expect(screen.getAllByTitle(t("transcribe"))).toHaveLength(1);
    fireEvent.click(screen.getByTitle(t("transcribe")));
    expect(api.transcribeAsset).toHaveBeenCalledWith(1);
    cleanup();

    transcribingAssetIdStore.set(1);
    render(<MediaLibrary session={sessionWith([asset()])} />);
    const busy = screen.getByTitle(t("transcribing")) as HTMLButtonElement;
    expect(busy.textContent).toBe("…");
    expect(busy.disabled).toBe(true);
    cleanup();

    transcribingAssetIdStore.set(9);
    render(<MediaLibrary session={sessionWith([asset()])} />);
    expect((screen.getByTitle(t("transcribe")) as HTMLButtonElement).disabled).toBe(true);
  });
});

describe("asset dragging", () => {
  const press = (target: Element, x: number, y: number, button = 0) => {
    return fireEvent(
      target,
      new MouseEvent("pointerdown", { bubbles: true, button, clientX: x, clientY: y }),
    );
  };
  const move = (x: number, y: number) => {
    return fireEvent(window, new MouseEvent("pointermove", { clientX: x, clientY: y }));
  };

  it("starts a drag only after the pointer travels past the threshold, then clears it", async () => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    press(screen.getByText("clip.mp4"), 10, 10);
    move(12, 11);
    expect(assetDragStore.get()).toBeNull();

    move(40, 30);
    expect(assetDragStore.get()).toEqual({ assetId: 1, clientX: 40, clientY: 30 });
    move(50, 30);
    expect(assetDragStore.get()).toMatchObject({ clientX: 50 });

    fireEvent(window, new MouseEvent("pointerup"));
    await waitFor(() => {
      return expect(assetDragStore.get()).toBeNull();
    });
    move(90, 90);
    expect(assetDragStore.get()).toBeNull();
  });

  it.each([
    [
      "the pointer is cancelled",
      () => {
        return fireEvent(window, new MouseEvent("pointercancel"));
      },
    ],
    [
      "Escape is pressed",
      () => {
        return fireEvent.keyDown(window, { key: "Escape" });
      },
    ],
    [
      "the window loses focus",
      () => {
        return fireEvent(window, new Event("blur"));
      },
    ],
  ])("drops the drag at once when %s", (_name, interrupt) => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    press(screen.getByText("clip.mp4"), 10, 10);
    move(60, 60);
    expect(assetDragStore.get()).not.toBeNull();

    interrupt();
    expect(assetDragStore.get()).toBeNull();
    move(90, 90);
    expect(assetDragStore.get()).toBeNull();
  });

  it("ignores presses other than the primary button", () => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    press(screen.getByText("clip.mp4"), 10, 10, 2);
    move(100, 100);
    expect(assetDragStore.get()).toBeNull();
  });

  it("does not start a drag from the row's buttons", () => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    press(screen.getByTitle(t("favorite")), 10, 10);
    move(100, 100);
    expect(assetDragStore.get()).toBeNull();
  });

  it("does not start a drag from the transcribe button", () => {
    render(<MediaLibrary session={sessionWith([asset()])} />);
    press(screen.getByTitle(t("transcribe")), 10, 10);
    move(100, 100);
    expect(assetDragStore.get()).toBeNull();
  });
});

describe("text to speech", () => {
  it("generates speech from trimmed non-empty text and closes", () => {
    render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("ttsButton")));
    const generate = screen.getByText(t("ttsGenerate")) as HTMLButtonElement;
    expect(generate.disabled).toBe(true);
    fireEvent.change(screen.getByPlaceholderText(t("ttsPlaceholder")), {
      target: { value: "hello" },
    });
    fireEvent.click(generate);
    expect(api.generateTts).toHaveBeenCalledWith("hello");
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("closes from Cancel and from the backdrop", () => {
    const { container } = render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("ttsButton")));
    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByRole("heading")).toBeNull();

    fireEvent.click(screen.getByText(t("ttsButton")));
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(screen.queryByRole("heading")).toBeNull();
  });
});

describe("sound library", () => {
  it("explains an empty library and toggles closed", async () => {
    const { container } = render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("soundLibrary")));
    expect(await screen.findByText(t("soundLibraryEmpty"))).toBeTruthy();
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(screen.queryByText(t("soundLibraryEmpty"))).toBeNull();

    fireEvent.click(screen.getByText(t("soundLibrary")));
    await screen.findByText(t("soundLibraryEmpty"));
    fireEvent.click(screen.getAllByText(t("soundLibrary"))[0]);
    expect(screen.queryByText(t("soundLibraryEmpty"))).toBeNull();
  });

  it("lists tracks and imports the chosen one", async () => {
    vi.mocked(api.listSoundLibrary).mockResolvedValue([
      { name: "Boom", path: "C:/s/boom.wav", durationSecs: 2, category: "sfx" },
      { name: "Theme", path: "C:/s/theme.mp3", durationSecs: 61, category: "music" },
    ]);
    render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("soundLibrary")));
    expect(await screen.findByText(/🔊 Boom/)).toBeTruthy();
    expect(screen.getByText(/🎵 Theme/)).toBeTruthy();

    const imports = screen.getAllByText(t("importFiles"));
    fireEvent.click(imports[imports.length - 1]);
    expect(api.importMedia).toHaveBeenCalledWith(["C:/s/theme.mp3"]);
    expect(screen.queryByText(/Boom/)).toBeNull();
  });

  it("treats a failed listing as empty", async () => {
    vi.mocked(api.listSoundLibrary).mockRejectedValue(new Error("no folder"));
    render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("soundLibrary")));
    expect(await screen.findByText(t("soundLibraryEmpty"))).toBeTruthy();
  });
});

describe("YouTube download", () => {
  const open = async () => {
    const view = render(<MediaLibrary session={sessionWith([])} />);
    fireEvent.click(screen.getByText(t("youtubeDownload")));
    await waitFor(() => {
      return expect(api.isYtDlpAvailable).toHaveBeenCalled();
    });
    return view;
  };
  const type = (value: string) => {
    return fireEvent.change(screen.getByPlaceholderText(t("youtubeDownloadUrlHint")), {
      target: { value },
    });
  };
  const startButton = () => {
    return screen.getByText(t("youtubeDownloadStart")) as HTMLButtonElement;
  };

  it("warns when the download tool is missing", async () => {
    vi.mocked(api.isYtDlpAvailable).mockResolvedValue(false);
    await open();
    expect(await screen.findByText(t("youtubeDownloadToolMissing"))).toBeTruthy();
  });

  it("needs a URL, and switching format resets the quality", async () => {
    await open();
    expect(startButton().disabled).toBe(true);
    const [format, quality] = screen.getAllByRole("combobox") as HTMLSelectElement[];
    expect(
      [...quality.options].map((o) => {
        return o.value;
      }),
    ).toEqual(["best", "1080", "720", "480", "360"]);
    fireEvent.change(format, { target: { value: "mp3" } });
    expect(quality.value).toBe("192");
    expect(
      [...quality.options].map((o) => {
        return o.value;
      }),
    ).toEqual(["320", "192", "128"]);
    fireEvent.change(quality, { target: { value: "128" } });
    fireEvent.change(format, { target: { value: "mp4" } });
    expect(quality.value).toBe("best");
  });

  it("keeps Start disabled for a blank URL", async () => {
    await open();
    type("   ");
    expect(startButton().disabled).toBe(true);
    expect(api.downloadYoutube).not.toHaveBeenCalled();
  });

  it("downloads, shows progress and closes when done", async () => {
    await open();
    type("https://youtu.be/x");
    fireEvent.click(startButton());
    expect(api.downloadYoutube).toHaveBeenCalledWith("https://youtu.be/x", "mp4", "best");

    act(() => {
      return emitDownload({ type: "progress", fraction: 0.5 });
    });
    expect((document.querySelector("progress") as HTMLProgressElement).value).toBe(0.5);
    // The modal cannot be dismissed from the backdrop mid-download.
    fireEvent.click(document.querySelector(".modal-overlay")!);
    expect(screen.getByText(t("youtubeDownloadCancel"))).toBeTruthy();

    act(() => {
      return emitDownload({ type: "done" });
    });
    await waitFor(() => {
      return expect(stopListening).toHaveBeenCalled();
    });
    expect(screen.queryByPlaceholderText(t("youtubeDownloadUrlHint"))).toBeNull();
  });

  it("cancels a running download and reports a failed one", async () => {
    await open();
    type("https://youtu.be/x");
    fireEvent.click(startButton());
    fireEvent.click(screen.getByText(t("youtubeDownloadCancel")));
    expect(api.cancelYoutubeDownload).toHaveBeenCalled();

    act(() => {
      return emitDownload({ type: "failed", message: "403" });
    });
    expect(await screen.findByText("403")).toBeTruthy();
    expect(startButton()).toBeTruthy();
    await waitFor(() => {
      return expect(stopListening).toHaveBeenCalled();
    });
  });

  it("recovers when starting the download itself fails", async () => {
    vi.mocked(api.downloadYoutube).mockRejectedValueOnce(new Error("spawn"));
    await open();
    type("https://youtu.be/x");
    fireEvent.click(startButton());
    await waitFor(() => {
      return expect(document.querySelector("progress")).toBeNull();
    });
  });

  it("closes from Cancel and from the backdrop while idle", async () => {
    const { container } = await open();
    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByPlaceholderText(t("youtubeDownloadUrlHint"))).toBeNull();

    fireEvent.click(screen.getByText(t("youtubeDownload")));
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(screen.queryByPlaceholderText(t("youtubeDownloadUrlHint"))).toBeNull();
  });
});

describe("media library tabs, search and view", () => {
  const library = () => {
    return {
      project: {
        media_library: [
          asset({ id: 1, file_name: "alpha.mp4", favorited: true }),
          asset({ id: 2, file_name: "beta.mp4" }),
          asset({ id: 3, file_name: "gamma.mp4" }),
        ],
        recent_asset_ids: [3, 1, 99],
      },
    } as unknown as SessionView;
  };

  it("filters by the favorites tab", () => {
    render(<MediaLibrary session={library()} />);
    fireEvent.click(screen.getByRole("tab", { name: t("libraryTabFavorites") }));
    expect(screen.getByText("alpha.mp4")).toBeTruthy();
    expect(screen.queryByText("beta.mp4")).toBeNull();
  });

  it("lists recent assets in recency order and ignores ids that no longer exist", () => {
    const { container } = render(<MediaLibrary session={library()} />);
    fireEvent.click(screen.getByRole("tab", { name: t("libraryTabRecent") }));
    const names = [...container.querySelectorAll(".asset .name")].map((n) => {
      return n.textContent;
    });
    expect(names).toEqual(["gamma.mp4", "alpha.mp4"]);
  });

  it("searches by name, counts the matches and explains an empty result", () => {
    const { container } = render(<MediaLibrary session={library()} />);
    expect(container.querySelector(".media-head .count")!.textContent).toContain(`3 ${t("items")}`);
    fireEvent.change(screen.getByLabelText(t("searchMedia")), { target: { value: "ET" } });
    expect(screen.getByText("beta.mp4")).toBeTruthy();
    expect(screen.queryByText("alpha.mp4")).toBeNull();
    expect(container.querySelector(".media-head .count")!.textContent).toContain(`1 ${t("items")}`);
    fireEvent.change(screen.getByLabelText(t("searchMedia")), { target: { value: "zzz" } });
    expect(screen.getByText(t("libraryNoMatches"))).toBeTruthy();
  });

  it("switches between grid and list", () => {
    const { container } = render(<MediaLibrary session={library()} />);
    expect(container.querySelector(".asset-list")!.className).toContain("grid");
    fireEvent.click(screen.getByLabelText(t("viewList")));
    expect(container.querySelector(".asset-list")!.className).toContain("list");
    expect(screen.getByLabelText(t("viewList")).getAttribute("aria-pressed")).toBe("true");
  });
});

describe("media card keyboard and context menu", () => {
  const library = () => {
    return {
      project: {
        media_library: [
          asset({ id: 1, file_name: "alpha.mp4" }),
          asset({ id: 2, file_name: "mute.mp4", has_audio: false }),
        ],
        recent_asset_ids: [],
      },
    } as unknown as SessionView;
  };
  const card = (name: string) => {
    return screen.getByText(name).closest(".asset") as HTMLElement;
  };

  it("makes each card focusable and adds it to the timeline on Enter", () => {
    render(<MediaLibrary session={library()} />);
    expect(card("alpha.mp4").getAttribute("tabindex")).toBe("0");
    fireEvent.keyDown(card("alpha.mp4"), { key: "Enter" });
    expect(api.addAssetToTimeline).toHaveBeenCalledWith(1);
  });

  it("ignores Enter pressed on a button inside the card", () => {
    render(<MediaLibrary session={library()} />);
    fireEvent.keyDown(within(card("alpha.mp4")).getAllByRole("button")[0], { key: "Enter" });
    expect(api.addAssetToTimeline).not.toHaveBeenCalled();
  });

  it("opens a menu on right click with the card's actions", () => {
    render(<MediaLibrary session={library()} />);
    fireEvent.contextMenu(card("alpha.mp4"), { clientX: 30, clientY: 40 });
    fireEvent.click(screen.getByText(t("addToTimeline"), { selector: "span" }));
    expect(api.addAssetToTimeline).toHaveBeenCalledWith(1);
    fireEvent.contextMenu(card("alpha.mp4"));
    fireEvent.click(screen.getByText(t("favorite"), { selector: "span" }));
    expect(api.toggleFavorite).toHaveBeenCalledWith(1);
    fireEvent.contextMenu(card("alpha.mp4"));
    fireEvent.click(screen.getByText(t("transcribe"), { selector: "span" }));
    expect(api.transcribeAsset).toHaveBeenCalledWith(1);
  });

  it("disables transcribing for media without audio", () => {
    render(<MediaLibrary session={library()} />);
    fireEvent.contextMenu(card("mute.mp4"));
    const item = screen.getByText(t("transcribe"), { selector: "span" }).closest("button")!;
    expect(item.disabled).toBe(true);
  });

  it("keeps Import one click away and the other sources in a menu", () => {
    render(<MediaLibrary session={library()} />);
    fireEvent.click(screen.getByText(t("importFiles"), { selector: "button" }));
    expect(api.importMediaDialog).toHaveBeenCalled();
    expect(screen.getByLabelText(t("moreSources"))).toBeTruthy();
  });
});

describe("removing media from the library", () => {
  const sessionWithClip = (usedAssetId: number) => {
    return {
      project: {
        media_library: [asset({ id: 1, file_name: "alpha.mp4" })],
        recent_asset_ids: [],
        sequences: [
          {
            timeline: {
              tracks: [{ clips: [{ asset_id: usedAssetId }], text_clips: [], shape_clips: [] }],
            },
          },
        ],
      },
    } as unknown as SessionView;
  };
  const openRemove = () => {
    const card = screen.getByText("alpha.mp4").closest(".asset") as HTMLElement;
    fireEvent.contextMenu(card);
    fireEvent.click(screen.getByText(t("removeFromLibrary"), { selector: "span" }));
  };

  beforeEach(() => {
    toastStore.set([]);
    confirmStore.set(null);
  });

  it("asks first, then removes the asset", async () => {
    render(<MediaLibrary session={sessionWithClip(99)} />);
    openRemove();
    await waitFor(() => {
      expect(confirmStore.get()).not.toBeNull();
    });
    expect(api.removeAsset).not.toHaveBeenCalled();
    answerConfirm(true);
    await waitFor(() => {
      expect(api.removeAsset).toHaveBeenCalledWith(1);
    });
  });

  it("does nothing when the question is dismissed", async () => {
    render(<MediaLibrary session={sessionWithClip(99)} />);
    openRemove();
    await waitFor(() => {
      expect(confirmStore.get()).not.toBeNull();
    });
    answerConfirm(false);
    await Promise.resolve();
    expect(api.removeAsset).not.toHaveBeenCalled();
  });

  it("refuses with a message while a clip still uses the media", () => {
    render(<MediaLibrary session={sessionWithClip(1)} />);
    openRemove();
    expect(confirmStore.get()).toBeNull();
    expect(api.removeAsset).not.toHaveBeenCalled();
    expect(
      toastStore.get().some((toast) => {
        return toast.message === t("assetInUse");
      }),
    ).toBe(true);
  });
});
