// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { uiStore } from "../store";
import type { SessionView } from "../types";

vi.mock("../ipc", () => {
  return {
    api: {
      getGpuEncoderPreference: vi.fn().mockResolvedValue("auto"),
      setGpuEncoderPreference: vi.fn().mockResolvedValue(undefined),
      setExportSettings: vi.fn().mockResolvedValue(undefined),
    },
    exportActiveSequence: vi.fn().mockResolvedValue(undefined),
  };
});

const { ExportDialog } = await import("./ExportDialog");
const { api, exportActiveSequence } = await import("../ipc");

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

  it("saves the chosen settings, then opens the queue and asks for the file", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<ExportDialog session={session()} />);
    fireEvent.click(screen.getByRole("radio", { name: t("aspectSquare") }));
    fireEvent.click(screen.getByRole("radio", { name: "-23 LUFS" }));
    fireEvent.click(screen.getByText(t("exportChooseFile")));
    await waitFor(() => {
      expect(exportActiveSequence).toHaveBeenCalledTimes(1);
    });
    expect(api.setExportSettings).toHaveBeenCalledWith(-23, "Square");
    expect(api.setGpuEncoderPreference).toHaveBeenCalledWith("auto");
    expect(uiStore.get().exportDialogOpen).toBe(false);
    expect(uiStore.get().exportQueueOpen).toBe(true);
    const ask = vi.mocked(exportActiveSequence).mock.calls[0][0];
    expect(ask()).toBe(true);
    expect(confirm).toHaveBeenCalledWith(t("exportOverwriteConfirm"));
    confirm.mockRestore();
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
    expect(exportActiveSequence).not.toHaveBeenCalled();
  });
});
