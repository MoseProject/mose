// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { t } from "../i18n";
import { exportsStore, sessionStore, transcribingAssetIdStore, uiStore } from "../store";
import { makeSession } from "../testSupport";
import type { ExportJobView } from "../types";
import { StatusBar } from "./StatusBar";

const withAssets = (assets: unknown[]) => {
  const session = makeSession([{ id: 1, name: "S" }]);
  session.project.media_library = assets as never;
  return session;
};
const job = (overrides: Partial<ExportJobView>): ExportJobView => {
  return {
    id: 1,
    title: "cut.mp4",
    outputPath: "C:/cut.mp4",
    status: "Queued",
    percent: 0,
    elapsedSecs: null,
    ...overrides,
  };
};

beforeEach(() => {
  sessionStore.set(null);
  exportsStore.set([]);
  transcribingAssetIdStore.set(null);
  uiStore.set({ ...uiStore.get(), exportQueueOpen: false });
});
afterEach(() => {
  cleanup();
});

describe("StatusBar", () => {
  it("is absent while nothing is running", () => {
    sessionStore.set(
      withAssets([{ id: 1, file_name: "a.mp4", loudness: { integrated_lufs: -18 } }]),
    );
    const { container } = render(<StatusBar />);
    expect(container.firstChild).toBeNull();
  });

  it("lists media still being analyzed, without a percentage", () => {
    sessionStore.set(withAssets([{ id: 1, file_name: "long.mp4", loudness: null }]));
    render(<StatusBar />);
    expect(screen.getByText(t("taskAnalyzing") + " long.mp4")).toBeTruthy();
    expect(document.querySelector(".task-bar.indeterminate")).toBeTruthy();
  });

  it("shows a running transcription by file name", () => {
    sessionStore.set(
      withAssets([{ id: 4, file_name: "talk.mp4", loudness: { integrated_lufs: -1 } }]),
    );
    transcribingAssetIdStore.set(4);
    render(<StatusBar />);
    expect(screen.getByText(t("transcribing") + " talk.mp4")).toBeTruthy();
  });

  it("shows export progress and opens the queue when clicked", () => {
    exportsStore.set([
      job({ id: 1, status: { Rendering: { percent: 42 } }, percent: 42 }),
      job({ id: 2, status: "Done" }),
    ]);
    render(<StatusBar />);
    const task = screen.getByText(/42%/);
    expect(task.textContent).toContain("cut.mp4");
    expect(screen.queryAllByRole("button")).toHaveLength(1);
    fireEvent.click(task);
    expect(uiStore.get().exportQueueOpen).toBe(true);
  });

  it("counts queued and paused exports as running and hides finished ones", () => {
    exportsStore.set([
      job({ id: 1, status: "Queued" }),
      job({ id: 2, status: { Paused: { percent: 10 } }, percent: 10 }),
      job({ id: 3, status: "Done" }),
      job({ id: 4, status: { Failed: { message: "x" } } as ExportJobView["status"] }),
    ]);
    render(<StatusBar />);
    expect(screen.getAllByRole("button")).toHaveLength(2);
  });

  it("shows four tasks and counts the rest", () => {
    sessionStore.set(
      withAssets(
        Array.from({ length: 6 }, (_, i) => {
          return { id: i, file_name: "f" + i + ".mp4", loudness: null };
        }),
      ),
    );
    render(<StatusBar />);
    expect(screen.getAllByRole("button")).toHaveLength(4);
    expect(screen.getByText("+2")).toBeTruthy();
    act(() => {
      sessionStore.set(null);
    });
    expect(screen.queryByRole("status")).toBeNull();
  });
});
