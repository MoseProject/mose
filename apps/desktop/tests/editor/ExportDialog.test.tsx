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
      getGpuEncoderPreference: vi.fn().mockResolvedValue("auto"),
      setGpuEncoderPreference: vi.fn().mockResolvedValue(undefined),
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
  vi.mocked(api.getGpuEncoderPreference).mockResolvedValue("auto");
  vi.mocked(api.getNormalizeAudio).mockResolvedValue(true);
  uiStore.set({ ...uiStore.get(), exportDialogOpen: true, exportQueueOpen: false });
});
afterEach(() => {
  cleanup();
});

describe("ExportDialog", () => {
  it("starts from the sequence's own settings", () => {
    render(<ExportDialog session={session({ aspect: "Portrait", lufs: -16 })} />);
    expect(
      screen.getByRole("radio", { name: t("aspectPortrait") }).getAttribute("aria-checked"),
    ).toBe("true");
    expect(screen.getByRole("radio", { name: "-16 LUFS" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    expect((screen.getByRole("spinbutton") as HTMLInputElement).value).toBe("-16");
  });

  it("shows the measured loudness of the first clip, when there is one", () => {
    const { unmount } = render(<ExportDialog session={session({ loudness: -18.24 })} />);
    expect(screen.getByText(/-18\.2 LUFS/, { selector: "p" })).toBeTruthy();
    unmount();
    render(<ExportDialog session={session({ loudness: null })} />);
    expect(screen.queryByText(t("loudnessMeasured"), { exact: false })).toBeNull();
  });

  it("adopts the stored encoder preference", async () => {
    vi.mocked(api.getGpuEncoderPreference).mockResolvedValue("nvenc");
    render(<ExportDialog session={session()} />);
    await waitFor(() => {
      expect((screen.getByLabelText(t("gpuEncoder")) as HTMLSelectElement).value).toBe("nvenc");
    });
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
    fireEvent.click(screen.getByRole("radio", { name: t("exportQualityLow") }));
    fireEvent.click(screen.getByRole("radio", { name: "720p" }));
    expect(screen.getByTestId("export-summary").textContent).toContain("1280x720");
    expect(screen.getByTestId("export-summary").textContent).toContain("4.0 Mbps");
    // 1080p is the original here, so it is not offered as a lower resolution.
    expect(screen.queryByRole("radio", { name: "1080p" })).toBeNull();
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
    fireEvent.click(screen.getByRole("radio", { name: t("aspectSquare") }));
    fireEvent.click(screen.getByRole("radio", { name: "-23 LUFS" }));
    fireEvent.click(screen.getByRole("radio", { name: t("exportQualityMedium") }));
    fireEvent.click(screen.getByRole("radio", { name: "720p" }));
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledTimes(1);
    });
    expect(api.exportTarget).toHaveBeenCalledWith("D:/videos", "final");
    expect(api.startExport).toHaveBeenCalledWith("D:/videos/final.mp4", false, {
      bitrateMbps: 8,
      maxHeight: 720,
    });
    expect(api.setExportSettings).toHaveBeenCalledWith(-23, "Square");
    expect(api.setGpuEncoderPreference).toHaveBeenCalledWith("auto");
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
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    expect((await screen.findByRole("alert")).textContent).toBe(t("exportErrFolderMissing"));
    expect(uiStore.get().exportDialogOpen).toBe(true);
    expect(api.startExport).not.toHaveBeenCalled();
  });

  it("rejects a custom bitrate out of range", () => {
    render(<ExportDialog session={session()} />);
    fireEvent.click(screen.getByRole("radio", { name: t("exportQualityCustom") }));
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
    fireEvent.change(screen.getByRole("spinbutton"), { target: { value: "" } });
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(api.startExport).toHaveBeenCalledTimes(1);
    });
    expect(api.setNormalizeAudio).toHaveBeenCalledWith(false);
    expect(api.setExportSettings).toHaveBeenCalledWith(-14, "Original");
  });

  it("accepts a custom loudness target and rejects one out of range", async () => {
    render(<ExportDialog session={session()} />);
    const input = screen.getByRole("spinbutton");
    fireEvent.change(input, { target: { value: "-3" } });
    expect(screen.getByRole("alert").textContent).toBe(t("loudnessRange"));
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "" } });
    expect((screen.getByText(t("exportChooseFile")) as HTMLButtonElement).disabled).toBe(true);
    fireEvent.change(input, { target: { value: "-20.5" } });
    expect(screen.queryByRole("alert")).toBeNull();
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
});
