// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { exportsStore } from "../store";
import type { ExportJobView } from "../types";

vi.mock("../ipc", () => ({
  api: {
    getGpuEncoderPreference: vi.fn().mockResolvedValue("auto"),
    setGpuEncoderPreference: vi.fn().mockResolvedValue(undefined),
    exportActiveSequenceSrt: vi.fn().mockResolvedValue(true),
    matchExportLoudness: vi.fn().mockResolvedValue(undefined),
    pauseExport: vi.fn().mockResolvedValue(undefined),
    resumeExport: vi.fn().mockResolvedValue(undefined),
    cancelExport: vi.fn().mockResolvedValue(undefined),
    removeExport: vi.fn().mockResolvedValue(undefined),
  },
  exportActiveSequence: vi.fn().mockResolvedValue(undefined),
}));

// Imported after the mock so ExportPanel picks up the mocked `../ipc`.
const { ExportPanel } = await import("./ExportPanel");
const { api, exportActiveSequence } = await import("../ipc");

function job(overrides: Partial<ExportJobView>): ExportJobView {
  return {
    id: 1,
    title: "clip.mp4",
    outputPath: "C:/out/clip.mp4",
    status: "Queued",
    percent: 0,
    elapsedSecs: null,
    ...overrides,
  };
}

describe("ExportPanel", () => {
  beforeEach(() => {
    exportsStore.set([]);
  });
  afterEach(() => {
    cleanup();
  });

  it("shows the empty state with no jobs", () => {
    render(<ExportPanel />);
    expect(screen.getByText(t("noExports"))).toBeTruthy();
  });

  it("renders a queued job with a remove button", () => {
    exportsStore.set([job({ status: "Queued" })]);
    render(<ExportPanel />);
    expect(screen.getByText("clip.mp4")).toBeTruthy();
    expect(screen.getByText(t("removeExport"))).toBeTruthy();
  });

  it("renders a rendering job with progress, pause and cancel", () => {
    exportsStore.set([
      job({ status: { Rendering: { percent: 42 } }, percent: 42, elapsedSecs: 12 }),
    ]);
    render(<ExportPanel />);
    expect(screen.getByText(t("pauseExport"))).toBeTruthy();
    expect(screen.getByText(t("cancel"))).toBeTruthy();
    expect(screen.getByText(/42%/)).toBeTruthy();
  });

  it("renders a paused job with resume and cancel", () => {
    exportsStore.set([job({ status: { Paused: { percent: 10 } }, percent: 10 })]);
    render(<ExportPanel />);
    expect(screen.getByText(t("resumeExport"))).toBeTruthy();
  });

  it("only offers loudness-match shortcuts once at least two jobs are queued", () => {
    exportsStore.set([job({ id: 1, status: "Queued" })]);
    const { rerender } = render(<ExportPanel />);
    expect(screen.queryByText("-14 LUFS")).toBeNull();

    exportsStore.set([job({ id: 1, status: "Queued" }), job({ id: 2, status: "Queued" })]);
    rerender(<ExportPanel />);
    expect(screen.getByText("-14 LUFS")).toBeTruthy();
  });

  it("calls cancelExport when a rendering job's cancel button is clicked", () => {
    exportsStore.set([job({ id: 7, status: { Rendering: { percent: 5 } }, percent: 5 })]);
    render(<ExportPanel />);
    fireEvent.click(screen.getByText(t("cancel")));
    expect(api.cancelExport).toHaveBeenCalledWith(7);
  });

  it("adopts the stored GPU encoder preference and saves a change", async () => {
    vi.mocked(api.getGpuEncoderPreference).mockResolvedValueOnce("nvenc");
    render(<ExportPanel />);
    const select = screen.getByRole("combobox") as HTMLSelectElement;
    await waitFor(() => expect(select.value).toBe("nvenc"));
    fireEvent.change(select, { target: { value: "cpu" } });
    expect(select.value).toBe("cpu");
    expect(api.setGpuEncoderPreference).toHaveBeenCalledWith("cpu");
  });

  it("keeps the default GPU preference when none is stored", async () => {
    vi.mocked(api.getGpuEncoderPreference).mockResolvedValueOnce("");
    render(<ExportPanel />);
    await waitFor(() => expect(api.getGpuEncoderPreference).toHaveBeenCalled());
    expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("auto");
  });

  it("starts an export asking for overwrite confirmation through window.confirm", async () => {
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
    render(<ExportPanel />);
    fireEvent.click(screen.getByText(t("export"), { selector: "button" }));
    expect(exportActiveSequence).toHaveBeenCalledTimes(1);
    const ask = vi.mocked(exportActiveSequence).mock.calls[0][0];
    expect(ask()).toBe(true);
    expect(confirm).toHaveBeenCalledWith(t("exportOverwriteConfirm"));
    confirm.mockRestore();
  });

  it("exports the subtitles as SRT", () => {
    render(<ExportPanel />);
    fireEvent.click(screen.getByText(t("exportSrt")));
    expect(api.exportActiveSequenceSrt).toHaveBeenCalled();
  });

  it("matches the queue loudness to the chosen target", () => {
    exportsStore.set([job({ id: 1 }), job({ id: 2 })]);
    render(<ExportPanel />);
    fireEvent.click(screen.getByText("-16 LUFS"));
    expect(api.matchExportLoudness).toHaveBeenCalledWith(-16);
  });

  it("pauses a rendering job and resumes then cancels a paused one", () => {
    exportsStore.set([job({ id: 3, status: { Rendering: { percent: 5 } }, percent: 5 })]);
    const { rerender } = render(<ExportPanel />);
    fireEvent.click(screen.getByText(t("pauseExport")));
    expect(api.pauseExport).toHaveBeenCalledWith(3);

    exportsStore.set([job({ id: 3, status: { Paused: { percent: 5 } }, percent: 5 })]);
    rerender(<ExportPanel />);
    fireEvent.click(screen.getByText(t("resumeExport")));
    expect(api.resumeExport).toHaveBeenCalledWith(3);
    fireEvent.click(screen.getByText(t("cancel")));
    expect(api.cancelExport).toHaveBeenCalledWith(3);
  });

  it("removes finished and failed jobs", () => {
    exportsStore.set([
      job({ id: 1, status: "Done" }),
      job({ id: 2, status: { Failed: { message: "boom" } } as ExportJobView["status"] }),
    ]);
    render(<ExportPanel />);
    const buttons = screen.getAllByText(t("removeExport"));
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[0]);
    fireEvent.click(buttons[1]);
    expect(api.removeExport).toHaveBeenCalledWith(1);
    expect(api.removeExport).toHaveBeenCalledWith(2);
  });
});
