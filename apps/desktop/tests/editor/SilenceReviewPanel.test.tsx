// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      detectSilenceGapsForClip: vi.fn(),
      applySilenceGaps: vi.fn().mockResolvedValue(undefined),
      seek: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { SilenceReviewPanel } = await import("../../src/editor/SilenceReviewPanel");
const { api } = await import("../../src/ipc");
const { uiStore, patchUi } = await import("../../src/store");

const detect = vi.mocked(api.detectSilenceGapsForClip);

beforeEach(() => {
  vi.clearAllMocks();
  patchUi({ selectedClipIds: [], error: null });
});
afterEach(() => {
  return cleanup();
});

describe("silence review panel", () => {
  it("asks for a clip first when none is selected", () => {
    render(<SilenceReviewPanel />);
    fireEvent.click(screen.getByText(t("silenceReview")));
    expect(uiStore.get().error).toBe(t("silenceReviewSelectClipFirst"));
    expect(detect).not.toHaveBeenCalled();
  });

  it("stays closed when the scan fails", async () => {
    patchUi({ selectedClipIds: [5] });
    detect.mockRejectedValueOnce(new Error("scan failed"));
    render(<SilenceReviewPanel />);
    fireEvent.click(screen.getByText(t("silenceReview")));
    await waitFor(() => {
      return expect(uiStore.get().error).toContain("scan failed");
    });
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("lists every gap, accepted by default, and applies only the ones left checked", async () => {
    patchUi({ selectedClipIds: [5] });
    detect.mockResolvedValueOnce({
      trackId: 3,
      gaps: [
        { startSecs: 1, endSecs: 2.5 },
        { startSecs: 10, endSecs: 11 },
      ],
    });
    render(<SilenceReviewPanel />);
    fireEvent.click(screen.getByText(t("silenceReview")));
    await screen.findByRole("heading");
    expect(detect).toHaveBeenCalledWith(5);

    const boxes = screen.getAllByRole("checkbox") as HTMLInputElement[];
    expect(
      boxes.map((box) => {
        return box.checked;
      }),
    ).toEqual([true, true]);
    expect(screen.getByText(/1\.5s/)).toBeTruthy();

    fireEvent.click(boxes[1]);
    fireEvent.click(screen.getByText(t("silenceReviewApply")));
    expect(api.applySilenceGaps).toHaveBeenCalledWith(3, [{ startSecs: 1, endSecs: 2.5 }]);
    expect(screen.queryByRole("heading")).toBeNull();
  });

  it("seeks to a gap and applies nothing when every gap is unchecked", async () => {
    patchUi({ selectedClipIds: [5] });
    detect.mockResolvedValueOnce({ trackId: 3, gaps: [{ startSecs: 4, endSecs: 5 }] });
    render(<SilenceReviewPanel />);
    fireEvent.click(screen.getByText(t("silenceReview")));
    await screen.findByRole("heading");

    fireEvent.click(screen.getByLabelText(t("goToGap")));
    expect(api.seek).toHaveBeenCalledWith(4);

    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByText(t("silenceReviewApply")));
    expect(api.applySilenceGaps).not.toHaveBeenCalled();
  });

  it("shows an empty state, blocks Apply, and closes from Cancel or the backdrop", async () => {
    patchUi({ selectedClipIds: [5] });
    detect.mockResolvedValue({ trackId: 3, gaps: [] });
    const { container } = render(<SilenceReviewPanel />);
    fireEvent.click(screen.getByText(t("silenceReview")));
    await screen.findByText(t("silenceReviewEmpty"));
    expect((screen.getByText(t("silenceReviewApply")) as HTMLButtonElement).disabled).toBe(true);

    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByRole("heading")).toBeNull();

    fireEvent.click(screen.getByText(t("silenceReview")));
    await screen.findByText(t("silenceReviewEmpty"));
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(screen.queryByRole("heading")).toBeNull();
  });
});
