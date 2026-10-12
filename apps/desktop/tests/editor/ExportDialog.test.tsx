// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { ConfirmHost } from "../../src/editor/chrome/Notifications";
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
      exportTarget: vi.fn(async (folder: string, name: string) => {
        return { path: `${folder}/${name}.mp4`, exists: false };
      }),
      startExport: vi.fn().mockResolvedValue(1),
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
  uiStore.set({ ...uiStore.get(), exportDialogOpen: true, exportQueueOpen: false });
});
afterEach(() => {
  cleanup();
});

describe("ExportDialog", () => {
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
      expect(api.exportCanvas).toHaveBeenCalledWith("Portrait");
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
    expect(api.setNormalizeAudio).toHaveBeenCalledWith(true);
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

  it("asks before replacing an existing file, naming the two outcomes", async () => {
    vi.mocked(api.exportTarget).mockResolvedValueOnce({ path: "D:/videos/a.mp4", exists: true });
    render(
      <>
        <ExportDialog session={session()} />
        <ConfirmHost />
      </>,
    );
    await waitFor(() => {
      expect((screen.getByLabelText(t("exportFolder")) as HTMLInputElement).value).toBe(
        "D:/videos",
      );
    });
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    fireEvent.click(await screen.findByText(t("confirmKeepBoth")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledWith("D:/videos/a.mp4", true, {});
    });
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
    expect(api.setNormalizeAudio).toHaveBeenCalledWith(false);
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
});
