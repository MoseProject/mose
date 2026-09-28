// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
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
const { api } = await import("../ipc");

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
});
