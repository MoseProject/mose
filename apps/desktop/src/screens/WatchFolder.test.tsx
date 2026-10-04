// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import type { LoudnessMetrics, WatchFolderEvent } from "../types";

let emit: (event: WatchFolderEvent) => void = () => {};
const stopListening = vi.fn();

vi.mock("../ipc", () => ({
  api: {
    pickWatchFolder: vi.fn(),
    startWatchingFolder: vi.fn().mockResolvedValue(undefined),
    stopWatchingFolder: vi.fn().mockResolvedValue(undefined),
    addWatchedFileToProject: vi.fn().mockResolvedValue(undefined),
  },
  onWatchFolder: vi.fn((handler: (event: WatchFolderEvent) => void) => {
    emit = handler;
    return Promise.resolve(stopListening);
  }),
}));

const { WatchFolder } = await import("./WatchFolder");
const { api } = await import("../ipc");

const metrics = (lufs: number) => ({ integrated_lufs: lufs }) as unknown as LoudnessMetrics;
const file = "C:\\rec\\clip.mp4";

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(api.pickWatchFolder).mockResolvedValue("C:\\rec");
});
afterEach(() => cleanup());

async function startWatching() {
  render(<WatchFolder />);
  fireEvent.click(screen.getByText(t("watchFolderChoose"), { selector: "button" }));
  await screen.findByText("C:\\rec");
  fireEvent.click(screen.getByText(t("watchFolderStart")));
}

describe("watch folder screen", () => {
  it("cannot start before a folder is chosen", () => {
    render(<WatchFolder />);
    expect((screen.getByText(t("watchFolderStart")) as HTMLButtonElement).disabled).toBe(true);
    expect(screen.getByText(t("watchFolderEmpty"))).toBeTruthy();
    expect(screen.queryByText(t("watchFolderStop"))).toBeNull();
  });

  it("keeps the old path when the folder dialog is cancelled", async () => {
    vi.mocked(api.pickWatchFolder).mockResolvedValue(undefined as never);
    render(<WatchFolder />);
    fireEvent.click(screen.getByText(t("watchFolderChoose"), { selector: "button" }));
    await waitFor(() => expect(api.pickWatchFolder).toHaveBeenCalled());
    expect((screen.getByText(t("watchFolderStart")) as HTMLButtonElement).disabled).toBe(true);
  });

  it("starts and stops watching, locking the picker while it runs", async () => {
    await startWatching();
    expect(api.startWatchingFolder).toHaveBeenCalledWith("C:\\rec");
    const picker = screen.getByText(t("watchFolderChoose"), { selector: "button" }) as HTMLButtonElement;
    expect(picker.disabled).toBe(true);

    fireEvent.click(screen.getByText(t("watchFolderStop")));
    expect(api.stopWatchingFolder).toHaveBeenCalled();
    expect(screen.queryByText(t("watchFolderStop"))).toBeNull();
  });

  it("follows a recording from detection through processing to done", async () => {
    await startWatching();
    act(() => emit({ type: "detected", path: file }));
    expect(screen.getByText("clip.mp4")).toBeTruthy();
    expect(screen.getByText(t("watchFolderStabilizing"))).toBeTruthy();

    act(() => emit({ type: "stabilizing", path: file }));
    act(() => emit({ type: "processing", path: file }));
    act(() => emit({ type: "progress", path: file, percent: 40 }));
    expect(screen.getByText(`${t("watchFolderProcessing")} 40%`)).toBeTruthy();

    act(() => emit({ type: "done", path: file, before: metrics(-20), after: metrics(-14) }));
    expect(screen.getByText(/-20\.0 → -14\.0 LUFS/)).toBeTruthy();
  });

  it("adds a finished file to the project once", async () => {
    await startWatching();
    act(() => emit({ type: "done", path: file, before: metrics(-20), after: metrics(-14) }));
    fireEvent.click(screen.getByText(t("watchFolderAddToProject")));
    expect(api.addWatchedFileToProject).toHaveBeenCalledWith("C:\\rec", file);
    expect(screen.queryByText(t("watchFolderAddToProject"))).toBeNull();
  });

  it("shows a failure and clears the list on restart", async () => {
    await startWatching();
    act(() => emit({ type: "failed", path: "/rec/bad.mp4", message: "no audio" }));
    expect(screen.getByText("bad.mp4")).toBeTruthy();
    expect(screen.getByText(`${t("watchFolderError")}: no audio`)).toBeTruthy();

    fireEvent.click(screen.getByText(t("watchFolderStop")));
    fireEvent.click(screen.getByText(t("watchFolderStart")));
    expect(screen.getByText(t("watchFolderEmpty"))).toBeTruthy();
  });

  it("stops listening on unmount", async () => {
    const { unmount } = render(<WatchFolder />);
    unmount();
    await waitFor(() => expect(stopListening).toHaveBeenCalled());
  });
});
