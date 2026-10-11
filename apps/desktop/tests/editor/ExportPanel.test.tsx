// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { exportsStore, patchUi, uiStore } from "../../src/store";
import type { ExportJobView } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      exportActiveSequenceSrt: vi.fn().mockResolvedValue(true),
      matchExportLoudness: vi.fn().mockResolvedValue(undefined),
      pauseExport: vi.fn().mockResolvedValue(undefined),
      resumeExport: vi.fn().mockResolvedValue(undefined),
      cancelExport: vi.fn().mockResolvedValue(undefined),
      removeExport: vi.fn().mockResolvedValue(undefined),
      revealInFolder: vi.fn().mockResolvedValue(undefined),
    },
  };
});

// Imported after the mock so ExportPanel picks up the mocked `../ipc`.
const { ExportPanel, exportSummary, remainingSecs } =
  await import("../../src/editor/export/ExportPanel");
const { api } = await import("../../src/ipc");

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

  it("closes from its own button and on Escape", () => {
    patchUi({ exportQueueOpen: true });
    render(<ExportPanel />);
    fireEvent.click(screen.getByLabelText(t("close")));
    expect(uiStore.get().exportQueueOpen).toBe(false);
    patchUi({ exportQueueOpen: true });
    fireEvent.keyDown(window, { key: "Escape" });
    expect(uiStore.get().exportQueueOpen).toBe(false);
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

describe("export ETA and folder", () => {
  beforeEach(() => {
    exportsStore.set([]);
    vi.clearAllMocks();
  });
  afterEach(() => {
    cleanup();
  });

  it("extrapolates the time left from progress and elapsed time", () => {
    expect(remainingSecs(job({ percent: 25, elapsedSecs: 30 }))).toBeCloseTo(90);
    expect(remainingSecs(job({ percent: 50, elapsedSecs: 30 }))).toBeCloseTo(30);
  });

  it("has no estimate before progress is steady or once finished", () => {
    expect(remainingSecs(job({ percent: 1, elapsedSecs: 5 }))).toBeNull();
    expect(remainingSecs(job({ percent: 100, elapsedSecs: 5 }))).toBeNull();
    expect(remainingSecs(job({ percent: 40, elapsedSecs: null }))).toBeNull();
  });

  it("shows the estimate on a rendering job", () => {
    exportsStore.set([
      job({ status: { Rendering: { percent: 50 } }, percent: 50, elapsedSecs: 30 }),
    ]);
    render(<ExportPanel />);
    expect(screen.getByText(new RegExp(t("etaRemaining")))).toBeTruthy();
  });

  it("opens the output folder of a finished job", () => {
    exportsStore.set([job({ status: "Done", percent: 100 })]);
    render(<ExportPanel />);
    fireEvent.click(screen.getByText(t("openExportFolder")));
    expect(api.revealInFolder).toHaveBeenCalledWith("C:/out/clip.mp4");
  });

  it("offers no folder button for a job that has not finished", () => {
    exportsStore.set([job({ status: "Queued" })]);
    render(<ExportPanel />);
    expect(screen.queryByText(t("openExportFolder"))).toBeNull();
  });
});

describe("export summary", () => {
  const done = (report: ExportJobView["report"]) => {
    return job({ status: "Done", percent: 100, report });
  };

  it("says which encoder ran, whether it is a GPU or the CPU, and how fast", () => {
    expect(exportSummary(done({ encoder: "h264_nvenc", tookSecs: 30, videoSecs: 90 }))).toBe(
      `h264_nvenc (${t("encoderGpu")}) · 00:30.00 · 3.0x`,
    );
    expect(exportSummary(done({ encoder: "libopenh264", tookSecs: 120, videoSecs: 60 }))).toBe(
      `libopenh264 (${t("encoderCpu")}) · 02:00.00 · 0.5x`,
    );
  });

  it("copes with no encoder and with a video of no length", () => {
    expect(exportSummary(done({ encoder: null, tookSecs: 5, videoSecs: 0 }))).toBe(
      `${t("encoderUnknown")} · 00:05.00`,
    );
  });

  it("is null before a job has a report", () => {
    expect(exportSummary(job({ status: "Done" }))).toBeNull();
  });

  it("shows on a finished job only", () => {
    exportsStore.set([done({ encoder: "libopenh264", tookSecs: 10, videoSecs: 20 })]);
    render(<ExportPanel />);
    expect(screen.getByTestId("export-summary").textContent).toContain("libopenh264");
    cleanup();
    exportsStore.set([
      job({
        status: { Rendering: { percent: 10 } },
        percent: 10,
        report: { encoder: "libopenh264", tookSecs: 1, videoSecs: 1 },
      }),
    ]);
    render(<ExportPanel />);
    expect(screen.queryByTestId("export-summary")).toBeNull();
  });
});
