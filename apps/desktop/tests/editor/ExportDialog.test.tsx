// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { uiStore } from "../../src/store";
import type { SessionView } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      getNormalizeAudio: vi.fn().mockResolvedValue(true),
      setNormalizeAudio: vi.fn().mockResolvedValue(undefined),
      setExportSettings: vi.fn().mockResolvedValue(undefined),
      getPrefs: vi.fn().mockResolvedValue({ outputFolder: "D:/videos" }),
      setPrefs: vi.fn().mockResolvedValue(undefined),
      pickPath: vi.fn().mockResolvedValue("E:/picked"),
      exportCanvas: vi
        .fn()
        .mockResolvedValue({ width: 1920, height: 1080, bitrateMbps: 16, durationSecs: 60 }),
      exportTarget: vi.fn(async (folder: string, name: string, extension?: string) => {
        return { path: `${folder}/${name}.${extension ?? "mp4"}`, exists: false };
      }),
      startExport: vi.fn().mockResolvedValue(1),
      exportClipsTargets: vi.fn(async (ids: number[], folder: string, prefix: string) => {
        return ids.map((id) => {
          return { path: `${folder}/${prefix}${id}.mp4`, exists: false };
        });
      }),
      startExportClips: vi.fn().mockResolvedValue([1, 2]),
    },
  };
});

const { ExportDialog } = await import("../../src/editor/export/ExportDialog");
const { api } = await import("../../src/ipc");

/** Waits for the stored folder to arrive, which is what enables Export. */
const ready = async () => {
  await waitFor(() => {
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(false);
  });
};

/** Picks `value` in the dropdown labelled `label`. */
const choose = (label: string, value: string) => {
  fireEvent.change(screen.getByLabelText(label, { exact: false }), { target: { value } });
};

const session = (overrides: { aspect?: string; lufs?: number; loudness?: number | null } = {}) => {
  return {
    project: {
      active_sequence: 0,
      media_library: [
        {
          id: 1,
          loudness:
            overrides.loudness === null ? null : { integrated_lufs: overrides.loudness ?? -18.2 },
        },
      ],
      sequences: [
        {
          name: "Montage",
          export_settings: {
            aspect_ratio: overrides.aspect ?? "Original",
            target_lufs: overrides.lufs ?? -14,
          },
          timeline: { tracks: [{ clips: [{ asset_id: 1 }] }] },
        },
      ],
    },
  } as unknown as SessionView;
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.getNormalizeAudio).mockResolvedValue(true);
  // clearAllMocks keeps implementations set by an earlier test.
  vi.mocked(api.exportTarget).mockImplementation(async (folder, name, extension) => {
    return { path: `${folder}/${name}.${extension ?? "mp4"}`, exists: false };
  });
  vi.mocked(api.exportClipsTargets).mockImplementation(async (ids, folder, prefix) => {
    return ids.map((id) => {
      return { path: `${folder}/${prefix}${id}.mp4`, exists: false };
    });
  });
  uiStore.set({ ...uiStore.get(), exportDialogOpen: true, exportQueueOpen: false });
});
afterEach(() => {
  cleanup();
});

describe("ExportDialog", () => {
  it("exports only the sound in the chosen format, without the picture settings", async () => {
    render(<ExportDialog session={session()} />);
    await ready();
    expect(screen.getByLabelText(t("exportQuality"), { exact: false })).toBeTruthy();
    choose(t("exportFormat"), "Flac");
    // The picture settings go away; the loudness stays.
    expect(screen.queryByLabelText(t("exportQuality"), { exact: false })).toBeNull();
    expect(screen.queryByLabelText(t("exportResolution"), { exact: false })).toBeNull();
    expect(screen.getByLabelText(t("loudnessTarget"), { exact: false })).toBeTruthy();
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledTimes(1);
    });
    expect(api.exportTarget).toHaveBeenCalledWith("D:/videos", "Montage", "flac");
    expect(api.startExport).toHaveBeenCalledWith("D:/videos/Montage.flac", false, {
      audioFormat: "Flac",
    });
  });

  it("offers no audio-only format for a batch of clips", () => {
    render(
      <ExportDialog
        session={session()}
        clipIds={[1, 2]}
      />,
    );
    expect(screen.queryByLabelText(t("exportFormat"), { exact: false })).toBeNull();
  });

  it("starts from the sequence's own settings", () => {
    render(<ExportDialog session={session({ aspect: "Portrait", lufs: -16 })} />);
    const value = (label: string) => {
      return (screen.getByLabelText(label, { exact: false }) as HTMLSelectElement).value;
    };
    expect(value(t("loudnessTarget"))).toBe("-16");
    // A preset and the custom field are exclusive: the field exists only for "custom".
    expect(screen.queryByRole("spinbutton")).toBeNull();
  });

  it("takes the aspect ratio from the project, with no choice offered", async () => {
    render(<ExportDialog session={session({ aspect: "Portrait" })} />);
    expect(screen.queryByLabelText(t("aspectRatio"), { exact: false })).toBeNull();
    await waitFor(() => {
      expect(api.exportCanvas).toHaveBeenCalledWith("Portrait", undefined);
    });
  });

  it("starts on custom, with the field, when the sequence target is not a preset", () => {
    render(<ExportDialog session={session({ lufs: -18 })} />);
    expect(
      (screen.getByLabelText(t("loudnessTarget"), { exact: false }) as HTMLSelectElement).value,
    ).toBe("custom");
    expect((screen.getByRole("spinbutton") as HTMLInputElement).value).toBe("-18");
  });

  it("shows the measured loudness of the first clip, when there is one", () => {
    const { unmount } = render(<ExportDialog session={session({ loudness: -18.24 })} />);
    expect(screen.getByText(/-18\.2 LUFS/, { selector: "p" })).toBeTruthy();
    unmount();
    render(<ExportDialog session={session({ loudness: null })} />);
    expect(screen.queryByText(t("loudnessMeasured"), { exact: false })).toBeNull();
  });

  it("does not offer the encoder choice, which lives in the preferences", () => {
    render(<ExportDialog session={session()} />);
    expect(screen.queryByLabelText(t("gpuEncoder"), { exact: false })).toBeNull();
  });

  it("starts with the sequence name and the stored output folder", async () => {
    render(<ExportDialog session={session()} />);
    expect(
      (screen.getByLabelText(t("exportFileName"), { exact: false }) as HTMLInputElement).value,
    ).toBe("Montage");
    await waitFor(() => {
      expect((screen.getByLabelText(t("exportFolder")) as HTMLInputElement).value).toBe(
        "D:/videos",
      );
    });
  });

  it("browses for the folder", async () => {
    render(<ExportDialog session={session()} />);
    fireEvent.click(screen.getByText(t("browse")));
    await waitFor(() => {
      expect((screen.getByLabelText(t("exportFolder")) as HTMLInputElement).value).toBe(
        "E:/picked",
      );
    });
    expect(api.pickPath).toHaveBeenCalledWith("folder");
  });

  it("shows the output size and a size estimate, following quality and resolution", async () => {
    render(<ExportDialog session={session()} />);
    const summary = await screen.findByTestId("export-summary");
    expect(summary.textContent).toContain("1920x1080");
    expect(summary.textContent).toContain("16.0 Mbps");
    choose(t("exportQuality"), "low");
    choose(t("exportResolution"), "720");
    expect(screen.getByTestId("export-summary").textContent).toContain("1280x720");
    expect(screen.getByTestId("export-summary").textContent).toContain("4.0 Mbps");
    // 1080p is the original here, so it is not offered as a lower resolution.
    const heights = [
      ...(screen.getByLabelText(t("exportResolution"), { exact: false }) as HTMLSelectElement)
        .options,
    ].map((option) => {
      return option.value;
    });
    expect(heights).not.toContain("1080");
    expect(heights).toContain("1440");
  });

  it("falls back to the source quality when a smaller picture makes the chosen step too high", async () => {
    render(<ExportDialog session={session()} />);
    const quality = (await screen.findByLabelText(t("exportQuality"), {
      exact: false,
    })) as HTMLSelectElement;
    choose(t("exportQuality"), "medium");
    expect(quality.value).toBe("medium");
    // 720p carries about 7.1 Mbps of the source's 16, so an 8 Mbps step no longer makes sense.
    choose(t("exportResolution"), "720");
    await waitFor(() => {
      expect(quality.value).toBe("source");
    });
  });

  it("saves the chosen settings, opens the queue and queues the file with its options", async () => {
    render(<ExportDialog session={session()} />);
    await waitFor(() => {
      expect((screen.getByLabelText(t("exportFolder")) as HTMLInputElement).value).toBe(
        "D:/videos",
      );
    });
    fireEvent.change(screen.getByLabelText(t("exportFileName"), { exact: false }), {
      target: { value: "final" },
    });
    choose(t("loudnessTarget"), "-23");
    choose(t("exportQuality"), "low");
    choose(t("exportResolution"), "720");
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledTimes(1);
    });
    expect(api.exportTarget).toHaveBeenCalledWith("D:/videos", "final");
    expect(api.startExport).toHaveBeenCalledWith("D:/videos/final.mp4", false, {
      bitrateMbps: 4,
      maxHeight: 720,
    });
    expect(api.setExportSettings).toHaveBeenCalledWith(-23, "Original");
    // Untouched, the stored normalization setting is left alone.
    expect(api.setNormalizeAudio).not.toHaveBeenCalled();
    expect(uiStore.get().exportDialogOpen).toBe(false);
    expect(uiStore.get().exportQueueOpen).toBe(true);
  });

  it("remembers a folder chosen for next time", async () => {
    render(<ExportDialog session={session()} />);
    fireEvent.click(screen.getByText(t("browse")));
    await waitFor(() => {
      expect(api.pickPath).toHaveBeenCalled();
    });
    await waitFor(() => {
      expect((screen.getByLabelText(t("exportFolder")) as HTMLInputElement).value).toBe(
        "E:/picked",
      );
    });
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.setPrefs).toHaveBeenCalledWith({ outputFolder: "E:/picked" });
    });
  });

  it("warns as soon as the file exists, and keeps both files unless told to replace", async () => {
    vi.mocked(api.exportTarget).mockResolvedValue({ path: "D:/videos/a.mp4", exists: true });
    render(<ExportDialog session={session()} />);
    expect((await screen.findByRole("status")).textContent).toBe(t("exportExistsOne"));
    const choice = screen.getByLabelText(t("exportIfExists"), {
      exact: false,
    }) as HTMLSelectElement;
    expect(choice.value).toBe("rename");
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledWith("D:/videos/a.mp4", true, {});
    });
  });

  it("replaces the existing file when that is chosen", async () => {
    vi.mocked(api.exportTarget).mockResolvedValue({ path: "D:/videos/a.mp4", exists: true });
    render(<ExportDialog session={session()} />);
    await screen.findByRole("status");
    choose(t("exportIfExists"), "replace");
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledWith("D:/videos/a.mp4", false, {});
    });
  });

  it("does not warn when the name is free", async () => {
    render(<ExportDialog session={session()} />);
    await ready();
    await new Promise((resolve) => {
      setTimeout(resolve, 400);
    });
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("keeps the dialog open and says why when the folder or name is refused", async () => {
    vi.mocked(api.exportTarget).mockRejectedValueOnce("folderMissing");
    render(<ExportDialog session={session()} />);
    await ready();
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    expect((await screen.findByRole("alert")).textContent).toBe(t("exportErrFolderMissing"));
    expect(uiStore.get().exportDialogOpen).toBe(true);
    expect(api.startExport).not.toHaveBeenCalled();
  });

  it("rejects a custom bitrate out of range", () => {
    render(<ExportDialog session={session()} />);
    choose(t("exportQuality"), "custom");
    fireEvent.change(screen.getByLabelText(t("exportBitrate"), { exact: false }), {
      target: { value: "900" },
    });
    expect(screen.getByRole("alert").textContent).toBe(t("exportBitrateRange"));
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(screen.getByLabelText(t("exportBitrate"), { exact: false }), {
      target: { value: "12" },
    });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("can export without normalization, even with an out-of-range target left in the field", async () => {
    vi.mocked(api.getNormalizeAudio).mockResolvedValue(false);
    render(<ExportDialog session={session()} />);
    const box = screen.getByLabelText(t("normalizeAudio")) as HTMLInputElement;
    await waitFor(() => {
      expect(box.checked).toBe(false);
    });
    choose(t("loudnessTarget"), "custom");
    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "" } });
    await ready();
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledTimes(1);
    });
    expect(api.startExport).toHaveBeenCalledWith(expect.any(String), false, { normalize: false });
    expect(api.setExportSettings).toHaveBeenCalledWith(-14, "Original");
  });

  it("accepts a custom loudness target and rejects one out of range", async () => {
    render(<ExportDialog session={session()} />);
    // Only the "custom" choice brings the field.
    expect(screen.queryByRole("spinbutton")).toBeNull();
    choose(t("loudnessTarget"), "custom");
    const input = screen.getByRole("spinbutton") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "-3" } });
    expect(screen.getByRole("alert").textContent).toBe(t("loudnessRange"));
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "" } });
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "-20.5" } });
    expect(screen.queryByRole("alert")).toBeNull();
    await ready();
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.setExportSettings).toHaveBeenCalledWith(-20.5, "Original");
    });
  });

  it("closes without exporting on Cancel and on Escape", () => {
    render(<ExportDialog session={session()} />);
    fireEvent.click(screen.getByText(t("cancel")));
    expect(uiStore.get().exportDialogOpen).toBe(false);
    uiStore.set({ ...uiStore.get(), exportDialogOpen: true });
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(uiStore.get().exportDialogOpen).toBe(false);
    expect(api.startExport).not.toHaveBeenCalled();
  });

  it("keeps Export off until there is a folder and a name", async () => {
    vi.mocked(api.getPrefs).mockResolvedValueOnce({ outputFolder: "" } as never);
    render(<ExportDialog session={session()} />);
    const button = screen.getByText(t("exportChooseFile")) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.change(screen.getByLabelText(t("exportFolder")), { target: { value: "D:/out" } });
    expect(button.disabled).toBe(false);
    fireEvent.change(screen.getByLabelText(t("exportFileName"), { exact: false }), {
      target: { value: "  " },
    });
    expect(button.disabled).toBe(true);
  });

  it("refuses to export an empty timeline and says so", async () => {
    vi.mocked(api.exportCanvas).mockRejectedValue("nothing to export");
    render(<ExportDialog session={session()} />);
    expect((await screen.findByRole("alert")).textContent).toBe(t("exportEmptyTimeline"));
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
  });

  it("disables quality steps above the source bitrate, and falls back from one chosen", async () => {
    vi.mocked(api.exportCanvas).mockResolvedValue({
      width: 1920,
      height: 1080,
      bitrateMbps: 6,
      durationSecs: 60,
    });
    render(<ExportDialog session={session()} />);
    const select = (await screen.findByLabelText(t("exportQuality"), {
      exact: false,
    })) as HTMLSelectElement;
    await waitFor(() => {
      expect(screen.getByText(t("exportQualityAboveSource"))).toBeTruthy();
    });
    const disabled = [...select.options]
      .filter((option) => {
        return option.disabled;
      })
      .map((option) => {
        return option.value;
      });
    expect(disabled).toEqual(["high", "medium"]);
  });

  it("shows the copy hint only while nothing asks for a different bitrate or size", async () => {
    render(<ExportDialog session={session()} />);
    await screen.findByTestId("export-summary");
    expect(screen.queryByText(t("exportKeepsBitrate"))).toBeTruthy();
    choose(t("exportQuality"), "low");
    expect(screen.queryByText(t("exportKeepsBitrate"))).toBeNull();
  });

  it("names the original resolution as the recommendation", async () => {
    render(<ExportDialog session={session()} />);
    const select = screen.getByLabelText(t("exportResolution"), {
      exact: false,
    }) as HTMLSelectElement;
    await waitFor(() => {
      expect(select.options[0].textContent).toBe(
        `${t("exportResolutionSource")} (1920x1080) · ${t("exportRecommended")}`,
      );
    });
  });

  it("offers larger sizes as an upscale and explains it only once one is chosen", async () => {
    render(<ExportDialog session={session()} />);
    const select = screen.getByLabelText(t("exportResolution"), {
      exact: false,
    }) as HTMLSelectElement;
    await waitFor(() => {
      expect(select.options.length).toBeGreaterThan(1);
    });
    const label = (value: string) => {
      return [...select.options].find((option) => {
        return option.value === value;
      })?.textContent;
    };
    expect(label("1440")).toBe(`1440p · ${t("exportUpscale")}`);
    expect(label("720")).toBe("720p");
    expect(screen.queryByRole("tooltip")).toBeNull();
    choose(t("exportResolution"), "1440");
    // The note is a tooltip described by an info button beside the dropdown.
    const tip = screen.getByRole("button", { name: t("exportUpscale") });
    const note = screen.getByRole("tooltip");
    expect(note.textContent).toBe(t("exportUpscaleNote"));
    expect(tip.getAttribute("aria-describedby")).toBe(note.id);
    expect(screen.getByTestId("export-summary").textContent).toContain("2560x1440");
    choose(t("exportResolution"), "720");
    expect(screen.queryByRole("tooltip")).toBeNull();
  });

  describe("for several clips", () => {
    const many = () => {
      return (
        <ExportDialog
          session={session()}
          clipIds={[4, 7, 9]}
        />
      );
    };

    it("is titled with the number of videos and asks the canvas of those clips", async () => {
      render(many());
      expect(screen.getByRole("dialog").getAttribute("aria-labelledby")).toBeTruthy();
      expect(screen.getByText(t("exportClipsTitle").replace("{n}", "3"))).toBeTruthy();
      await waitFor(() => {
        expect(api.exportCanvas).toHaveBeenCalledWith("Original", [4, 7, 9]);
      });
      await screen.findByTestId("export-summary");
      expect(screen.getByTestId("export-summary").textContent).toContain(t("exportEstimateTotal"));
    });

    it("takes an optional prefix instead of a file name", async () => {
      render(many());
      const prefix = screen.getByLabelText(t("exportFilePrefix"), { exact: false });
      expect((prefix as HTMLInputElement).value).toBe("");
      expect(screen.queryByText(".mp4")).toBeNull();
      // An empty prefix is fine: only the folder is needed.
      await ready();
    });

    it("queues each clip with the prefix, and closes", async () => {
      render(many());
      await ready();
      fireEvent.change(screen.getByLabelText(t("exportFilePrefix"), { exact: false }), {
        target: { value: "Cut" },
      });
      choose(t("exportQuality"), "low");
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      await waitFor(() => {
        expect(api.startExportClips).toHaveBeenCalledWith([4, 7, 9], "D:/videos", "Cut", false, {
          bitrateMbps: 4,
        });
      });
      expect(api.startExport).not.toHaveBeenCalled();
      expect(uiStore.get().exportDialogOpen).toBe(false);
      expect(uiStore.get().exportClipIds).toBeNull();
      expect(uiStore.get().exportQueueOpen).toBe(true);
    });

    it("warns with the count of files that exist and keeps both by default", async () => {
      vi.mocked(api.exportClipsTargets).mockResolvedValue([
        { path: "a.mp4", exists: true },
        { path: "b.mp4", exists: true },
        { path: "c.mp4", exists: false },
      ]);
      render(many());
      expect((await screen.findByRole("status")).textContent).toBe(
        t("exportExistsMany").replace("{n}", "2"),
      );
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      await waitFor(() => {
        expect(api.startExportClips).toHaveBeenCalledWith([4, 7, 9], "D:/videos", "", true, {});
      });
    });

    it("says why when nothing can be exported, and keeps the dialog open", async () => {
      vi.mocked(api.exportClipsTargets).mockRejectedValueOnce("noClips");
      render(many());
      await ready();
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      expect((await screen.findByRole("alert")).textContent).toBe(t("exportNoClips"));
      expect(api.startExportClips).not.toHaveBeenCalled();
    });
  });

  describe("audio already normalized on the clips", () => {
    const withLoudness = (integrated: number, gain: number) => {
      return {
        project: {
          active_sequence: 0,
          media_library: [{ id: 1, has_audio: true, loudness: { integrated_lufs: integrated } }],
          sequences: [
            {
              name: "Montage",
              export_settings: { aspect_ratio: "Original", target_lufs: -14 },
              timeline: {
                tracks: [
                  {
                    kind: "Video",
                    muted: false,
                    clips: [
                      {
                        id: 1,
                        asset_id: 1,
                        gain_db: gain,
                        gain_keyframes: [],
                        nested_sequence_id: null,
                      },
                    ],
                  },
                ],
              },
            },
          ],
        },
      } as unknown as SessionView;
    };

    it("leaves normalization off by default and says why", async () => {
      render(<ExportDialog session={withLoudness(-18, 4)} />);
      const box = screen.getByLabelText(t("normalizeAudio")) as HTMLInputElement;
      await waitFor(() => {
        expect(box.checked).toBe(false);
      });
      expect(screen.getByText(t("normalizeAlready").replace("{lufs}", "-14"))).toBeTruthy();
      await ready();
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      await waitFor(() => {
        expect(api.startExport).toHaveBeenCalledWith(expect.any(String), false, {
          normalize: false,
        });
      });
      // The stored preference is not rewritten by the automatic choice.
      expect(api.setNormalizeAudio).not.toHaveBeenCalled();
    });

    it("lets the user normalize anyway, which also saves the setting", async () => {
      render(<ExportDialog session={withLoudness(-18, 4)} />);
      const box = screen.getByLabelText(t("normalizeAudio")) as HTMLInputElement;
      fireEvent.click(box);
      expect(box.checked).toBe(true);
      await ready();
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      await waitFor(() => {
        expect(api.startExport).toHaveBeenCalledWith(expect.any(String), false, {});
      });
      expect(api.setNormalizeAudio).toHaveBeenCalledWith(true);
    });

    it("keeps normalizing when the clips are not at the target", () => {
      render(<ExportDialog session={withLoudness(-18, 0)} />);
      expect((screen.getByLabelText(t("normalizeAudio")) as HTMLInputElement).checked).toBe(true);
      expect(screen.queryByText(t("normalizeAlready").replace("{lufs}", "-14"))).toBeNull();
    });
  });

  describe("motion", () => {
    const motion = (on: boolean) => {
      Object.defineProperty(window, "matchMedia", {
        configurable: true,
        value: () => {
          return { matches: !on };
        },
      });
    };
    afterEach(() => {
      Reflect.deleteProperty(window, "matchMedia");
    });
    const overlay = () => {
      return screen.getByRole("dialog").parentElement as HTMLElement;
    };

    it("fades away on Cancel before closing", async () => {
      motion(true);
      render(<ExportDialog session={session()} />);
      fireEvent.click(screen.getByText(t("cancel")));
      expect(overlay().className).toContain("leaving");
      expect(overlay().className).not.toContain("flying");
      expect(uiStore.get().exportDialogOpen).toBe(true);
      await waitFor(() => {
        expect(uiStore.get().exportDialogOpen).toBe(false);
      });
    });

    it("closes at once when the system asks for reduced motion", () => {
      motion(false);
      render(<ExportDialog session={session()} />);
      fireEvent.click(screen.getByText(t("cancel")));
      expect(uiStore.get().exportDialogOpen).toBe(false);
    });

    it("flies towards the export queue button when exporting", async () => {
      motion(true);
      const target = document.createElement("button");
      target.setAttribute("data-fly-target", "export-queue");
      target.getBoundingClientRect = () => {
        return { left: 900, top: 10, width: 30, height: 30 } as DOMRect;
      };
      document.body.append(target);
      render(<ExportDialog session={session()} />);
      screen.getByRole("dialog").getBoundingClientRect = () => {
        return { left: 300, top: 200, width: 400, height: 500 } as DOMRect;
      };
      await ready();
      fireEvent.click(screen.getByText(t("exportChooseFile")));
      await waitFor(() => {
        expect(overlay().className).toContain("flying");
      });
      // From the middle of the panel (500, 450) to the middle of the button (915, 25).
      expect(screen.getByRole("dialog").style.transform).toBe(
        "translate(415px, -425px) scale(0.075)",
      );
      await waitFor(() => {
        expect(uiStore.get().exportDialogOpen).toBe(false);
      });
      expect(uiStore.get().exportQueueOpen).toBe(true);
      expect(api.startExport).toHaveBeenCalled();
      target.remove();
    });
  });
});
