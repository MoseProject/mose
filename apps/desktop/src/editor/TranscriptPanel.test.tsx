// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { makeSession } from "../testSupport";
import type { TranscriptDocument } from "../types";

vi.mock("../ipc", () => {
  return {
    api: {
      getTranscript: vi.fn(),
      searchTranscripts: vi.fn(),
      seekToTranscriptWord: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { TranscriptPanel } = await import("./TranscriptPanel");
const { api } = await import("../ipc");
const { patchUi, previewStore } = await import("../store");

const clip = (overrides: Record<string, unknown> = {}) => {
  return {
    id: 7,
    asset_id: 3,
    start_secs: 10,
    source_in_secs: 2,
    speed_factor: 1,
    frozen: false,
    ...overrides,
  };
};

const session = (clipOverrides: Record<string, unknown> = {}) => {
  return makeSession([{ id: 1, name: "Main", tracks: [{ clips: [clip(clipOverrides)] }] }]);
};

const document = (): TranscriptDocument => {
  return {
    asset_id: 3,
    words: [
      { id: 1, text: "hello", start_secs: 2, end_secs: 3, confidence: 0.9 },
      { id: 2, text: "world", start_secs: 3, end_secs: 4, confidence: 0.3 },
    ],
  } as unknown as TranscriptDocument;
};

beforeEach(() => {
  vi.clearAllMocks();
  patchUi({ selectedClipIds: [7], error: null });
  previewStore.set({ ...previewStore.get(), playheadSecs: 10 });
  vi.mocked(api.getTranscript).mockResolvedValue(document());
});
afterEach(() => {
  return cleanup();
});

function open(view = session()) {
  const result = render(<TranscriptPanel session={view} />);
  fireEvent.click(screen.getByText(t("transcriptPanelTitle")));
  return result;
}

describe("transcript panel", () => {
  it("loads the selected clip's transcript and seeks to a word", async () => {
    open();
    await screen.findByText("hello");
    expect(api.getTranscript).toHaveBeenCalledWith(3);

    fireEvent.click(screen.getByText("world"));
    expect(api.seekToTranscriptWord).toHaveBeenCalledWith(3, 3);
  });

  it("highlights the word under the playhead and dims low-confidence ones", async () => {
    // Playhead 10 maps to source time 2: the first word.
    open();
    const active = await screen.findByText("hello");
    expect(active.className).toContain("active");
    const dim = screen.getByText("world");
    expect(dim.className).not.toContain("active");
    expect(dim.style.opacity).toBe("0.6");
    expect(active.title).toContain("90%");
  });

  it("maps the playhead through the clip's speed, and a frozen clip stays on its first frame", async () => {
    previewStore.set({ ...previewStore.get(), playheadSecs: 11 });
    // At speed 2 one second of timeline is two seconds of source: source time 4.
    open(session({ speed_factor: 2 }));
    await screen.findByText("hello");
    expect(screen.getByText("hello").className).not.toContain("active");
    cleanup();

    previewStore.set({ ...previewStore.get(), playheadSecs: 50 });
    open(session({ frozen: true }));
    expect((await screen.findByText("hello")).className).toContain("active");
  });

  it("filters the words by the search text", async () => {
    open();
    await screen.findByText("hello");
    fireEvent.change(screen.getByPlaceholderText(t("transcriptPanelSearchHint")), {
      target: { value: "  WOR " },
    });
    expect(screen.queryByText("hello")).toBeNull();
    expect(screen.getByText("world")).toBeTruthy();
  });

  it("explains an empty selection and a clip without a transcript", async () => {
    patchUi({ selectedClipIds: [] });
    open();
    expect(await screen.findByText(t("transcriptPanelNoClip"))).toBeTruthy();
    cleanup();

    patchUi({ selectedClipIds: [7] });
    vi.mocked(api.getTranscript).mockResolvedValue(undefined as never);
    open();
    expect(await screen.findByText(t("transcriptPanelNoTranscript"))).toBeTruthy();
  });

  it("does not load anything for a multi-clip selection", () => {
    patchUi({ selectedClipIds: [7, 8] });
    open();
    expect(api.getTranscript).not.toHaveBeenCalled();
  });

  it("searches the whole project and groups hits by asset", async () => {
    const word = (id: number, text: string, start: number) => {
      return {
        id,
        text,
        start_secs: start,
        end_secs: start + 1,
        confidence: 0.8,
      };
    };
    vi.mocked(api.searchTranscripts).mockResolvedValue([
      { assetId: 3, assetName: "a.mp4", word: word(1, "alpha", 1) },
      { assetId: 4, assetName: "b.mp4", word: word(2, "beta", 5) },
      { assetId: 3, assetName: "a.mp4", word: word(3, "alpine", 9) },
    ] as never);
    open();
    await screen.findByText("hello");

    fireEvent.click(screen.getByLabelText(t("transcriptSearchProject")));
    // Nothing is searched until there is a query.
    expect(await screen.findByText(t("transcriptProjectEmpty"))).toBeTruthy();
    expect(api.searchTranscripts).not.toHaveBeenCalled();

    fireEvent.change(screen.getByPlaceholderText(t("transcriptProjectSearchHint")), {
      target: { value: "Al" },
    });
    await waitFor(() => {
      return expect(api.searchTranscripts).toHaveBeenCalledWith("al");
    });
    expect(await screen.findByText("b.mp4")).toBeTruthy();
    expect(screen.getAllByText("a.mp4")).toHaveLength(1);

    fireEvent.click(screen.getByText(/00:05\.00 - beta/));
    expect(api.seekToTranscriptWord).toHaveBeenCalledWith(4, 5);
  });

  it("closes from Cancel and from the backdrop", async () => {
    const { container } = open();
    await screen.findByText("hello");
    fireEvent.click(screen.getByText(t("cancel")));
    expect(screen.queryByPlaceholderText(t("transcriptPanelSearchHint"))).toBeNull();

    fireEvent.click(screen.getByText(t("transcriptPanelTitle")));
    await screen.findByText("hello");
    fireEvent.click(container.querySelector(".modal-overlay")!);
    expect(screen.queryByPlaceholderText(t("transcriptPanelSearchHint"))).toBeNull();
  });
});
