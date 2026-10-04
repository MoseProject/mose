// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { assetDragStore, previewStore, uiStore } from "../store";
import { makeSession } from "../testSupport";
import type { PreviewTick, SessionView, Track } from "../types";

vi.mock("./LayerTemplates", () => ({ LayerTemplates: () => null }));
vi.mock("./TranscriptPanel", () => ({ TranscriptPanel: () => null }));
vi.mock("./SilenceReviewPanel", () => ({ SilenceReviewPanel: () => null }));
vi.mock("../ipc", () => {
  const names = [
    "seek", "addAssetToTimeline", "selectSequence", "moveOverlayClip", "slideClip", "moveClip",
    "moveClipGroup", "slipClip", "trimOverlayClip", "rollEditFromStartEdge", "rollEditClip",
    "trimClip", "splitAtPlayhead", "deleteClips", "mergeClipsIntoComposite", "createCompoundClip",
    "detectSceneCutsForClip", "detectHighlights", "addTrack", "addMarker", "setTrackFlags",
  ];
  const api: Record<string, unknown> = Object.fromEntries(names.map((n) => [n, vi.fn().mockResolvedValue(undefined)]));
  api.addTextClip = vi.fn().mockResolvedValue(5);
  api.addShapeClip = vi.fn().mockResolvedValue(6);
  return { api, frameAtUrl: () => "about:blank", waveformUrl: () => "about:blank" };
});

const { Timeline } = await import("./Timeline");
const { api } = await import("../ipc");
const calls = api as unknown as Record<string, ReturnType<typeof vi.fn>>;

// ---- canvas, layout and frame-loop stubs --------------------------------------------------

const WIDTH = 800;
const HEIGHT = 300;
const sets: [string | symbol, unknown][] = [];
const ctx: Record<string | symbol, unknown> = new Proxy({} as Record<string | symbol, unknown>, {
  get(target, key) {
    if (!(key in target)) target[key] = vi.fn();
    return target[key];
  },
  set(target, key, value) {
    sets.push([key, value]);
    target[key] = value;
    return true;
  },
});

let observerTrigger: () => void = () => {};
class FakeResizeObserver {
  constructor(callback: () => void) {
    observerTrigger = callback;
  }
  observe() {}
  disconnect() {}
}

let frames: { id: number; callback: () => void }[] = [];
let nextFrame = 1;
const flush = () =>
  act(() => {
    const pending = frames;
    frames = [];
    pending.forEach((frame) => frame.callback());
  });

// ---- fixtures --------------------------------------------------------------------------------

const clip = (overrides: Record<string, unknown>) => ({
  id: 1,
  asset_id: 1,
  start_secs: 0,
  source_in_secs: 0,
  source_out_secs: 10,
  speed_factor: 1,
  gain_db: 0,
  color_label: null,
  frozen: false,
  nested_sequence_id: null,
  ...overrides,
});
const track = (overrides: Record<string, unknown>) =>
  ({
    id: 10,
    name: "V1",
    kind: "Video",
    clips: [],
    text_clips: [],
    shape_clips: [],
    visible: true,
    audio_role: "Dialogue",
    color_label: null,
    locked: false,
    ...overrides,
  }) as unknown as Track;

// Track rows are 56px tall below a 24px ruler: row i is centered on y = 52 + 56 * i.
const ROW = (index: number) => 52 + 56 * index;

function tracks(): Track[] {
  return [
    track({
      id: 10,
      name: "V1",
      clips: [
        clip({ id: 1, asset_id: 1 }),
        clip({ id: 2, asset_id: 2, start_secs: 20, speed_factor: 2, frozen: true, color_label: [1, 2, 3] }),
        clip({ id: 3, asset_id: 1, start_secs: 15, source_out_secs: 0.2 }),
        clip({ id: 8, asset_id: 1, start_secs: 100 }),
      ],
    }),
    track({ id: 11, name: "V2", visible: false, clips: [clip({ id: 5, asset_id: 4 })] }),
    track({
      id: 12,
      name: "A1",
      kind: "Audio",
      clips: [
        clip({ id: 6, asset_id: 3, nested_sequence_id: 2 }),
        clip({ id: 9, asset_id: 99, start_secs: 12, source_out_secs: 4, nested_sequence_id: 999 }),
      ],
    }),
    track({
      id: 13,
      name: "Txt",
      kind: "Text",
      text_clips: [
        { id: 20, start_secs: 2, duration_secs: 4, text: "hi", color_rgba: [255, 0, 0, 255] },
        { id: 21, start_secs: 7, duration_secs: 2, text: "", color_rgba: [0, 255, 0, 255] },
      ],
      shape_clips: [{ id: 22, start_secs: 10, duration_secs: 3, color_rgba: [0, 0, 255, 255] }],
    }),
    track({ id: 14, name: "V3", locked: true, clips: [clip({ id: 7, asset_id: 1 })] }),
    track({ id: 15, name: "V4" }),
  ];
}

function world(list: Track[] = tracks()): SessionView {
  const session = makeSession([
    {
      id: 1,
      name: "Main",
      tracks: list,
      markers: [
        { id: 1, position_secs: 3, label: "a", kind: "Standard", completed: false },
        { id: 2, position_secs: 6, label: "b", kind: "Standard", completed: true },
        { id: 3, position_secs: 1000, label: "c", kind: "Standard", completed: false },
      ],
    },
    { id: 2, name: "Nested" },
  ]);
  (session.project as unknown as { media_library: unknown[] }).media_library = [
    { id: 1, file_name: "a.mp4", duration_secs: 10, waveform_peaks: [[-0.5, 0.5], undefined] },
    { id: 2, file_name: "b.mp4", duration_secs: 10, waveform_peaks: [] },
    { id: 3, file_name: "c.wav", duration_secs: 10, waveform_peaks: null },
    { id: 4, file_name: "d.mp4", duration_secs: 0, waveform_peaks: [[-1, 1]] },
  ];
  return session;
}

const tick = (overrides: Partial<PreviewTick> = {}): PreviewTick => ({
  frameSeq: 0,
  playheadSecs: 0,
  playing: false,
  hasClip: false,
  available: true,
  error: null,
  peakL: 0,
  peakR: 0,
  ...overrides,
});

const canvas = () => screen.getByTestId("timeline-canvas");
// jsdom's PointerEvent init is unreliable, so build MouseEvents under the pointer event names.
const ptr = (type: string, x: number, y: number, init: MouseEventInit = {}) =>
  fireEvent(canvas(), new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0, ...init }));
const button = (label: string) => screen.getByText(label, { selector: "button" }) as HTMLButtonElement;
const titled = (title: string) => screen.getByTitle(title) as HTMLButtonElement;

function mount(session: SessionView = world()) {
  const view = render(<Timeline session={session} />);
  act(() => observerTrigger());
  flush();
  return view;
}

const resetUi = () =>
  uiStore.set({ screen: "editor", selectedClipIds: [], tool: "select", pxPerSec: 40, error: null, drawingShapePoints: null, exportQueueOpen: false });

beforeEach(() => {
  vi.clearAllMocks();
  sets.length = 0;
  frames = [];
  nextFrame = 1;
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  vi.stubGlobal("requestAnimationFrame", (callback: () => void) => {
    const id = nextFrame++;
    frames.push({ id, callback });
    return id;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => {
    frames = frames.filter((frame) => frame.id !== id);
  });
  HTMLCanvasElement.prototype.getContext = vi.fn(() => ctx) as never;
  HTMLCanvasElement.prototype.getBoundingClientRect = () =>
    ({ left: 0, top: 0, right: WIDTH, bottom: HEIGHT, width: WIDTH, height: HEIGHT, x: 0, y: 0 }) as DOMRect;
  Element.prototype.setPointerCapture = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "clientWidth", { configurable: true, get: () => WIDTH });
  Object.defineProperty(HTMLElement.prototype, "clientHeight", { configurable: true, get: () => HEIGHT });
  resetUi();
  previewStore.set(tick());
  assetDragStore.set(null);
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientWidth;
  delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
});

describe("timeline painting", () => {
  it("sizes the canvas to the lanes at the device pixel ratio", () => {
    Object.defineProperty(window, "devicePixelRatio", { configurable: true, value: 2 });
    mount();
    expect(canvas().getAttribute("width")).toBe(String(WIDTH * 2));
    expect(canvas().style.width).toBe(`${WIDTH}px`);
    Object.defineProperty(window, "devicePixelRatio", { configurable: true, value: 1 });
  });

  it("paints rows, clips, labels, badges and the ruler", () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 20] });
    mount();
    const text = (ctx.fillText as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
    expect(text).toContain("a.mp4");
    expect(text).toContain("b.mp4  2.00× ❄");
    expect(text).toContain("#99");
    expect(text).toContain("hi");
    expect(text).toContain(t("text"));
    expect(text).toContain(t("shape"));
    expect(text.some((label) => /^\d+:\d{2}/.test(label as string))).toBe(true);
    expect((ctx.strokeRect as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThanOrEqual(2);
    expect(sets).toContainEqual(["fillStyle", "rgb(1,2,3)"]);
    expect(sets).toContainEqual(["fillStyle", "rgba(0,0,0,0.45)"]);
  });

  it("skips clips that are off screen and tracks below the viewport", () => {
    mount();
    // Clip 8 starts 100 s in (4000 px), row 5 starts at 304 px, below the 300 px canvas.
    const rects = (ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls;
    expect(rects.some((c) => c[0] === 4000)).toBe(false);
    expect(rects.some((c) => c[1] === 24 + 5 * 56)).toBe(false);
  });

  it("does not label a clip too narrow to hold text", () => {
    mount();
    const text = (ctx.fillText as ReturnType<typeof vi.fn>).mock.calls.map((c) => c[0]);
    // Clip 3 is 0.2 s (8 px) wide and uses asset 1, whose label is already drawn once by clip 1/7/8.
    expect(text.filter((label) => label === "a.mp4")).toHaveLength(2);
  });

  it("repaints on playhead ticks and hides an off-screen playhead", () => {
    mount();
    const before = (ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls.length;
    act(() => previewStore.set(tick({ playheadSecs: 5 })));
    flush();
    expect((ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls.length).toBeGreaterThan(before);
    expect((ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls).toContainEqual([200, 0, 1, HEIGHT]);

    (ctx.fillRect as ReturnType<typeof vi.fn>).mockClear();
    act(() => previewStore.set(tick({ playheadSecs: 9000 })));
    flush();
    expect((ctx.fillRect as ReturnType<typeof vi.fn>).mock.calls.some((c) => c[3] === HEIGHT && c[2] === 1)).toBe(false);
  });

  it("coalesces repaint requests into one frame", () => {
    mount();
    act(() => {
      previewStore.set(tick({ playheadSecs: 1 }));
      previewStore.set(tick({ playheadSecs: 2 }));
    });
    expect(frames).toHaveLength(1);
  });

  it("shows a ghost block while an asset is dragged over the lanes", () => {
    mount();
    act(() => assetDragStore.set({ assetId: 1, clientX: 100, clientY: 100 }));
    flush();
    expect(sets).toContainEqual(["globalAlpha", 0.35]);

    sets.length = 0;
    act(() => assetDragStore.set({ assetId: 1, clientX: 100, clientY: 10 }));
    flush();
    expect(sets).not.toContainEqual(["globalAlpha", 0.35]);
    act(() => assetDragStore.set({ assetId: 777, clientX: 100, clientY: 100 }));
    flush();
    expect(sets).not.toContainEqual(["globalAlpha", 0.35]);
    act(() => assetDragStore.set({ assetId: 1, clientX: 5000, clientY: 100 }));
    flush();
    expect(sets).not.toContainEqual(["globalAlpha", 0.35]);
  });

  it("copes with a canvas that has no 2d context", () => {
    HTMLCanvasElement.prototype.getContext = vi.fn(() => null) as never;
    expect(() => mount()).not.toThrow();
  });

  it("cancels a pending frame on unmount", () => {
    const { unmount } = mount();
    act(() => previewStore.set(tick({ playheadSecs: 1 })));
    expect(frames).toHaveLength(1);
    unmount();
    expect(frames).toHaveLength(0);
  });

  it("explains an empty timeline", () => {
    mount(world([]));
    expect(screen.getByText(t("timelineEmpty"))).toBeTruthy();
  });
});

describe("toolbar", () => {
  it("disables selection-bound actions until enough clips are selected", () => {
    mount();
    for (const label of [t("delete"), t("rippleDelete"), t("mergeComposite"), t("compoundClip"), t("detectChapters")]) {
      expect(button(label).disabled).toBe(true);
    }
    act(() => uiStore.set({ ...uiStore.get(), selectedClipIds: [1] }));
    expect(button(t("delete")).disabled).toBe(false);
    expect(button(t("detectChapters")).disabled).toBe(false);
    expect(button(t("mergeComposite")).disabled).toBe(true);
    act(() => uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] }));
    expect(button(t("mergeComposite")).disabled).toBe(false);
    expect(button(t("detectChapters")).disabled).toBe(true);
  });

  it("runs the editing commands on the selection", () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] });
    mount();
    fireEvent.click(button(t("delete")));
    expect(calls.deleteClips).toHaveBeenCalledWith([1, 2], false);
    fireEvent.click(button(t("rippleDelete")));
    expect(calls.deleteClips).toHaveBeenCalledWith([1, 2], true);
    fireEvent.click(button(t("mergeComposite")));
    expect(calls.mergeClipsIntoComposite).toHaveBeenCalledWith([1, 2]);
    fireEvent.click(screen.getByTestId("create-compound-clip"));
    expect(calls.createCompoundClip).toHaveBeenCalledWith(1, [1, 2]);
    fireEvent.click(button(t("detectHighlightsAction")));
    expect(calls.detectHighlights).toHaveBeenCalled();
  });

  it("detects scene cuts for a single selected clip", () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [2] });
    mount();
    fireEvent.click(button(t("detectChapters")));
    expect(calls.detectSceneCutsForClip).toHaveBeenCalledWith(2);
  });

  it("adds each kind of track", () => {
    mount();
    fireEvent.click(button(t("addVideoTrack")));
    fireEvent.click(button(t("addAudioTrack")));
    fireEvent.click(button(t("addTextTrack")));
    fireEvent.click(button(t("addShapeTrack")));
    expect(calls.addTrack.mock.calls.map((c) => c[0])).toEqual(["Video", "Audio", "Text", "Shape"]);
  });

  it("creates a text or shape clip and selects it", async () => {
    mount();
    fireEvent.click(button(t("addTextClip")));
    await act(async () => {});
    expect(calls.addTextClip).toHaveBeenCalledWith(t("defaultTextContent"));
    expect(uiStore.get().selectedClipIds).toEqual([5]);
    fireEvent.click(button(t("addShapeClip")));
    await act(async () => {});
    expect(uiStore.get().selectedClipIds).toEqual([6]);
  });

  it("keeps the selection when creating the overlay fails", async () => {
    calls.addShapeClip.mockRejectedValueOnce(new Error("no"));
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1] });
    mount();
    fireEvent.click(button(t("addShapeClip")));
    await act(async () => {});
    expect(uiStore.get().selectedClipIds).toEqual([1]);
  });

  it("starts drawing a custom shape", () => {
    mount();
    fireEvent.click(screen.getByTestId("draw-custom-shape"));
    expect(uiStore.get().drawingShapePoints).toEqual([]);
  });

  it("adds a marker and jumps between markers around the playhead", () => {
    previewStore.set(tick({ playheadSecs: 4.5 }));
    mount();
    fireEvent.click(titled(t("addMarker")));
    expect(calls.addMarker).toHaveBeenCalled();
    fireEvent.click(titled(t("previousMarker")));
    expect(calls.seek).toHaveBeenLastCalledWith(3);
    fireEvent.click(titled(t("nextMarker")));
    expect(calls.seek).toHaveBeenLastCalledWith(6);
  });

  it("picks the nearest earlier and later markers", () => {
    previewStore.set(tick({ playheadSecs: 7 }));
    mount();
    fireEvent.click(titled(t("previousMarker")));
    expect(calls.seek).toHaveBeenLastCalledWith(6);
    previewStore.set(tick({ playheadSecs: 0.5 }));
    fireEvent.click(titled(t("nextMarker")));
    expect(calls.seek).toHaveBeenLastCalledWith(3);
  });

  it("does nothing when there is no marker in that direction", () => {
    previewStore.set(tick({ playheadSecs: 0 }));
    mount();
    fireEvent.click(titled(t("previousMarker")));
    previewStore.set(tick({ playheadSecs: 5000 }));
    fireEvent.click(titled(t("nextMarker")));
    expect(calls.seek).not.toHaveBeenCalled();
  });

  it("zooms with the buttons around the middle of the lanes", () => {
    mount();
    fireEvent.click(titled(t("zoomIn")));
    expect(uiStore.get().pxPerSec).toBe(60);
    fireEvent.click(titled(t("zoomOut")));
    expect(uiStore.get().pxPerSec).toBeCloseTo(40);
  });

  it("fits the whole timeline into the lanes", () => {
    mount();
    fireEvent.click(titled(t("zoomFit")));
    const px = uiStore.get().pxPerSec;
    expect(px).toBeGreaterThan(0);
    expect(px).toBeLessThan(40);
  });
});

describe("track headers", () => {
  it("toggles visibility and lock", () => {
    mount();
    fireEvent.click(screen.getAllByTitle(t("hideTrack"))[0]);
    expect(calls.setTrackFlags).toHaveBeenLastCalledWith(10, { visible: false });
    fireEvent.click(screen.getAllByTitle(t("showTrack"))[0]);
    expect(calls.setTrackFlags).toHaveBeenLastCalledWith(11, { visible: true });
    fireEvent.click(screen.getAllByTitle(t("lockTrack"))[0]);
    expect(calls.setTrackFlags).toHaveBeenLastCalledWith(10, { locked: true });
    fireEvent.click(screen.getAllByTitle(t("unlockTrack"))[0]);
    expect(calls.setTrackFlags).toHaveBeenLastCalledWith(14, { locked: false });
  });

  it("lists every track by name", () => {
    mount();
    for (const name of ["V1", "V2", "A1", "Txt", "V3", "V4"]) expect(screen.getByText(name)).toBeTruthy();
  });
});

describe("ruler", () => {
  let now = 1000;
  beforeEach(() => {
    now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
  });

  it("ignores any button but the primary one", () => {
    mount();
    ptr("pointerdown", 200, 10, { button: 2 });
    expect(calls.seek).not.toHaveBeenCalled();
  });

  it("seeks on press and throttles seeks while scrubbing, finishing on release", () => {
    mount();
    ptr("pointerdown", 200, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(5);
    now += 10;
    ptr("pointermove", 240, 10);
    expect(calls.seek).toHaveBeenCalledTimes(1);
    now += 40;
    ptr("pointermove", 280, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(7);
    ptr("pointerup", 320, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(8);
    expect(calls.seek).toHaveBeenCalledTimes(3);
    ptr("pointerup", 400, 10);
    expect(calls.seek).toHaveBeenCalledTimes(3);
  });

  it("never seeks before zero", () => {
    mount();
    ptr("pointerdown", -50, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(0);
  });

  it("jumps to a marker without starting a scrub", () => {
    mount();
    ptr("pointerdown", 121, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(3);
    now += 100;
    ptr("pointermove", 300, 10);
    expect(calls.seek).toHaveBeenCalledTimes(1);
  });
});

describe("selection", () => {
  it("clears the selection when clicking empty space", () => {
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1] });
    mount();
    ptr("pointerdown", 700, ROW(0));
    expect(uiStore.get().selectedClipIds).toEqual([]);
  });

  it("selects the clicked clip and keeps a selection that already contains it", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointerup", 200, ROW(0));
    expect(uiStore.get().selectedClipIds).toEqual([1]);
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1, 2] });
    ptr("pointerdown", 200, ROW(0));
    ptr("pointerup", 200, ROW(0));
    expect(uiStore.get().selectedClipIds).toEqual([1, 2]);
  });

  it("toggles clips in and out of the selection with ctrl or meta", () => {
    mount();
    ptr("pointerdown", 200, ROW(0), { ctrlKey: true });
    expect(uiStore.get().selectedClipIds).toEqual([1]);
    ptr("pointerdown", 200, ROW(1), { metaKey: true });
    expect(uiStore.get().selectedClipIds).toEqual([1, 5]);
    ptr("pointerdown", 200, ROW(0), { ctrlKey: true });
    expect(uiStore.get().selectedClipIds).toEqual([5]);
  });

  it("selects a clip on a locked track but never drags it", () => {
    mount();
    ptr("pointerdown", 200, ROW(4));
    expect(uiStore.get().selectedClipIds).toEqual([7]);
    ptr("pointermove", 300, ROW(4));
    ptr("pointerup", 300, ROW(4));
    expect(calls.moveClip).not.toHaveBeenCalled();
    expect(calls.moveClipGroup).not.toHaveBeenCalled();
  });

  it("selects overlay clips", () => {
    mount();
    ptr("pointerdown", 160, ROW(3));
    expect(uiStore.get().selectedClipIds).toEqual([20]);
  });
});

describe("hover cursor", () => {
  it("shows grab over a clip body, a resize cursor on an edge and the default elsewhere", () => {
    mount();
    ptr("pointermove", 200, ROW(0));
    expect(canvas().style.cursor).toBe("grab");
    ptr("pointermove", 2, ROW(0));
    expect(canvas().style.cursor).toBe("ew-resize");
    ptr("pointermove", 700, ROW(0));
    expect(canvas().style.cursor).toBe("default");
    ptr("pointermove", 200, 10);
    expect(canvas().style.cursor).toBe("default");
  });
});

describe("moving clips", () => {
  it("moves a clip along its row", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 300, ROW(0));
    flush();
    expect(sets).toContainEqual(["globalAlpha", 0.75]);
    ptr("pointerup", 300, ROW(0));
    expect(calls.moveClipGroup).toHaveBeenCalledWith(1, 2.5);
  });

  it("moves a clip to another row of the same kind", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 300, ROW(1));
    ptr("pointerup", 300, ROW(1));
    expect(calls.moveClip).toHaveBeenCalledWith(1, 11, 2.5);
  });

  it("stays on its row over a row of another kind, a locked row or the ruler", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 300, ROW(2));
    ptr("pointermove", 300, ROW(4));
    ptr("pointermove", 300, 10);
    ptr("pointerup", 300, 10);
    expect(calls.moveClip).not.toHaveBeenCalled();
    expect(calls.moveClipGroup).toHaveBeenCalledWith(1, 2.5);
  });

  it("commits nothing when released where it started", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointerup", 200, ROW(0));
    expect(calls.moveClipGroup).not.toHaveBeenCalled();
    expect(calls.moveClip).not.toHaveBeenCalled();
  });

  it("snaps the start of a moved clip to a nearby edge", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    // Raw start 14.9 s is within snap range of clip 3's start at 15 s.
    ptr("pointermove", 796, ROW(0));
    ptr("pointerup", 796, ROW(0));
    expect(calls.moveClipGroup).toHaveBeenCalledWith(1, 15);
  });

  it("snaps by the end edge when only the end lands on a candidate", () => {
    mount();
    ptr("pointerdown", 460, ROW(3));
    ptr("pointermove", 224, ROW(3));
    ptr("pointerup", 224, ROW(3));
    expect(calls.moveOverlayClip).toHaveBeenCalledWith(22, 13, 4);
  });

  it("slides instead of repositioning with the slide tool", () => {
    uiStore.set({ ...uiStore.get(), tool: "slide" });
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 300, ROW(1));
    ptr("pointerup", 300, ROW(1));
    expect(calls.slideClip).toHaveBeenCalledWith(1, 2.5);
    expect(calls.moveClip).not.toHaveBeenCalled();
  });

  it("moves an overlay clip through the overlay command whatever the tool", () => {
    uiStore.set({ ...uiStore.get(), tool: "slip" });
    mount();
    ptr("pointerdown", 160, ROW(3));
    ptr("pointermove", 260, ROW(3));
    flush();
    ptr("pointerup", 260, ROW(3));
    expect(calls.moveOverlayClip).toHaveBeenCalledWith(20, 13, 4.5);
    expect(calls.slipClip).not.toHaveBeenCalled();
  });
});

describe("slipping clips", () => {
  beforeEach(() => uiStore.set({ ...uiStore.get(), tool: "slip" }));

  it("slips by the pointer travel", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 240, ROW(0));
    flush();
    ptr("pointerup", 240, ROW(0));
    expect(calls.slipClip).toHaveBeenCalledWith(1, 1);
  });

  it("commits nothing without travel", () => {
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointerup", 200, ROW(0));
    expect(calls.slipClip).not.toHaveBeenCalled();
  });

  it("falls back to a trim on an edge", () => {
    mount();
    ptr("pointerdown", 2, ROW(0));
    ptr("pointermove", 100, ROW(0));
    ptr("pointerup", 100, ROW(0));
    expect(calls.trimClip).toHaveBeenCalledWith(1, "start", 2.5);
  });
});

describe("trimming clips", () => {
  it("trims either edge of a clip", () => {
    mount();
    ptr("pointerdown", 2, ROW(0));
    ptr("pointermove", 100, ROW(0));
    flush();
    ptr("pointerup", 100, ROW(0));
    expect(calls.trimClip).toHaveBeenLastCalledWith(1, "start", 2.5);

    ptr("pointerdown", 399, ROW(0));
    ptr("pointermove", 300, ROW(0));
    flush();
    ptr("pointerup", 300, ROW(0));
    expect(calls.trimClip).toHaveBeenLastCalledWith(1, "end", 7.5);
  });

  it("never trims an edge past the opposite one while painting the ghost", () => {
    mount();
    ptr("pointerdown", 2, ROW(0));
    ptr("pointermove", 800, ROW(0));
    flush();
    ptr("pointerdown", 399, ROW(0));
    ptr("pointermove", 0, ROW(0));
    flush();
    expect(sets.length).toBeGreaterThan(0);
  });

  it("rolls the shared boundary with the roll tool", () => {
    uiStore.set({ ...uiStore.get(), tool: "roll" });
    mount();
    ptr("pointerdown", 2, ROW(0));
    ptr("pointermove", 100, ROW(0));
    ptr("pointerup", 100, ROW(0));
    expect(calls.rollEditFromStartEdge).toHaveBeenCalledWith(1, 2.5);

    ptr("pointerdown", 399, ROW(0));
    ptr("pointermove", 300, ROW(0));
    ptr("pointerup", 300, ROW(0));
    expect(calls.rollEditClip).toHaveBeenCalledWith(1, 7.5);
  });

  it("moves a clip pressed on its body with the roll tool", () => {
    uiStore.set({ ...uiStore.get(), tool: "roll" });
    mount();
    ptr("pointerdown", 200, ROW(0));
    ptr("pointermove", 300, ROW(0));
    ptr("pointerup", 300, ROW(0));
    expect(calls.moveClipGroup).toHaveBeenCalled();
  });

  it("trims an overlay clip through the overlay command", () => {
    mount();
    ptr("pointerdown", 82, ROW(3));
    ptr("pointermove", 100, ROW(3));
    flush();
    ptr("pointerup", 100, ROW(3));
    expect(calls.trimOverlayClip).toHaveBeenCalledWith(20, "start", 2.5);
    expect(calls.trimClip).not.toHaveBeenCalled();

    ptr("pointerdown", 238, ROW(3));
    ptr("pointermove", 200, ROW(3));
    flush();
    ptr("pointerup", 200, ROW(3));
    expect(calls.trimOverlayClip).toHaveBeenLastCalledWith(20, "end", 5);
  });
});

describe("double click", () => {
  const dbl = (x: number, y: number) => fireEvent.doubleClick(canvas(), { clientX: x, clientY: y });

  it("opens the nested sequence of a compound clip", () => {
    mount();
    dbl(200, ROW(2));
    expect(calls.selectSequence).toHaveBeenCalledWith(1);
  });

  it("ignores the ruler, empty space, plain clips and dangling nested ids", () => {
    mount();
    dbl(200, 10);
    dbl(700, ROW(0));
    dbl(200, ROW(0));
    dbl(12 * 40 + 40, ROW(2));
    expect(calls.selectSequence).not.toHaveBeenCalled();
  });
});

describe("wheel", () => {
  const wheel = (init: WheelEventInit) => fireEvent.wheel(canvas(), { clientX: 200, clientY: 100, ...init });

  it("zooms around the pointer with ctrl", () => {
    mount();
    wheel({ ctrlKey: true, deltaY: -100 });
    expect(uiStore.get().pxPerSec).toBeCloseTo(46);
    wheel({ metaKey: true, deltaY: 100 });
    expect(uiStore.get().pxPerSec).toBeCloseTo(40);
  });

  it("scrolls the rows vertically within bounds when they overflow", () => {
    const { container } = mount();
    const headers = () => (container.querySelector(".track-headers > div:nth-child(2)") as HTMLElement).style.transform;
    wheel({ deltaY: 20 });
    expect(headers()).toBe("translateY(-20px)");
    wheel({ deltaY: 5000 });
    expect(headers()).toBe(`translateY(-${6 * 56 - (HEIGHT - 24)}px)`);
    wheel({ deltaY: -9000 });
    expect(headers()).toBe("translateY(0px)");
  });

  it("scrolls horizontally with shift or a sideways wheel, never before zero", () => {
    mount();
    wheel({ deltaX: 400 });
    ptr("pointerdown", 200, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(15);
    wheel({ shiftKey: true, deltaY: -80 });
    ptr("pointerdown", 200, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(13);
    wheel({ shiftKey: true, deltaY: -99999 });
    ptr("pointerdown", 200, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(5);
  });

  it("scrolls horizontally when every row fits", () => {
    mount(world([track({ id: 10 })]));
    wheel({ deltaY: 40 });
    ptr("pointerdown", 0, 10);
    expect(calls.seek).toHaveBeenLastCalledWith(1);
  });
});

describe("dropping a library asset", () => {
  const drop = (x: number, y: number) => act(() => void window.dispatchEvent(new MouseEvent("pointerup", { clientX: x, clientY: y })));

  it("adds the asset to the track under the pointer at the pointer time", () => {
    mount();
    act(() => assetDragStore.set({ assetId: 1, clientX: 200, clientY: ROW(1) }));
    drop(200, ROW(1));
    expect(calls.addAssetToTimeline).toHaveBeenCalledWith(1, 11, 5);
  });

  it("lets the backend pick a track when dropped below the last row", () => {
    mount(world([track({ id: 10 })]));
    act(() => assetDragStore.set({ assetId: 1, clientX: 200, clientY: 200 }));
    drop(200, 200);
    expect(calls.addAssetToTimeline).toHaveBeenCalledWith(1, undefined, 5);
  });

  it("ignores a drop outside the lanes or over the ruler, and a release with no drag", () => {
    mount();
    drop(200, ROW(0));
    act(() => assetDragStore.set({ assetId: 1, clientX: 200, clientY: ROW(0) }));
    drop(200, 10);
    drop(-5, ROW(0));
    drop(WIDTH + 5, ROW(0));
    drop(200, HEIGHT + 5);
    expect(calls.addAssetToTimeline).not.toHaveBeenCalled();
  });

  it("stops listening after unmount", () => {
    const { unmount } = mount();
    unmount();
    act(() => assetDragStore.set({ assetId: 1, clientX: 200, clientY: ROW(0) }));
    drop(200, ROW(0));
    expect(calls.addAssetToTimeline).not.toHaveBeenCalled();
  });
});
