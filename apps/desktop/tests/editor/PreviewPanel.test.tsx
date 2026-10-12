// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { patchUi, previewStore, sessionStore, uiStore } from "../../src/store";
import { makeSession } from "../support/testSupport";
import type { ClipInstance, PreviewTick } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return {
    api: {
      updateClip: vi.fn().mockResolvedValue(undefined),
      addTextClip: vi.fn().mockResolvedValue(9),
      step: vi.fn().mockResolvedValue(undefined),
      seek: vi.fn().mockResolvedValue(undefined),
      setScrubbing: vi.fn().mockResolvedValue(undefined),
      setPlaying: vi.fn().mockResolvedValue(undefined),
    },
    frameUrl: (seq: number) => {
      return `oca://frame/${seq}`;
    },
    scopeUrl: (kind: string, seq: number) => {
      return `oca://scope/${kind}/${seq}`;
    },
  };
});

const { PreviewPanel, clampPan } = await import("../../src/editor/PreviewPanel");
const { shuttleStore } = await import("../../src/shuttle");
const { api } = await import("../../src/ipc");

let resize: (width: number, height: number) => void = () => {};

class FakeResizeObserver {
  constructor(
    private callback: (entries: { contentRect: { width: number; height: number } }[]) => void,
  ) {
    resize = (width, height) => {
      return this.callback([{ contentRect: { width, height } }]);
    };
  }
  observe() {}
  disconnect() {}
}

const clip = (overrides: Partial<ClipInstance> = {}): ClipInstance => {
  return {
    id: 1,
    asset_id: 1,
    start_secs: 0,
    source_in_secs: 0,
    source_out_secs: 10,
    speed_factor: 1,
    position_keyframes: [],
    layer_scale_x: 1,
    layer_scale_y: 1,
    ...overrides,
  } as unknown as ClipInstance;
};

const sessionWith = (c: ClipInstance, resolution: [number, number] | null = [1920, 1080]) => {
  const session = makeSession([{ id: 1, name: "Seq", tracks: [{ clips: [c] }] }]);
  (session.project as unknown as { media_library: unknown[] }).media_library = [
    { id: 1, resolution },
  ];
  return session;
};

const tick = (overrides: Partial<PreviewTick> = {}): PreviewTick => {
  return {
    frameSeq: 1,
    playheadSecs: 0,
    playing: false,
    hasClip: true,
    available: true,
    error: null,
    peakL: 0,
    peakR: 0,
    momentaryLufs: null,
    renderingCompound: false,
    ...overrides,
  };
};

// jsdom's PointerEvent init is unreliable, so build MouseEvents under the pointer event names.
const pointer = (target: Element, type: string, x = 0, y = 0) => {
  return fireEvent(
    target,
    new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 }),
  );
};

const monitor = () => {
  return screen.getByTestId("preview-monitor");
};

function mount() {
  const view = render(<PreviewPanel />);
  act(() => {
    return resize(800, 450);
  });
  return view;
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("ResizeObserver", FakeResizeObserver);
  Element.prototype.setPointerCapture = vi.fn();
  previewStore.set(tick());
  sessionStore.set(null);
  uiStore.set({
    screen: "editor",
    selectedClipIds: [],
    tool: "select",
    pxPerSec: 40,
    error: null,
    drawingShapePoints: null,
    exportQueueOpen: false,
    exportDialogOpen: false,
    exportClipIds: null,
    openDialogs: 0,
    markIn: null,
    markOut: null,
    snapEnabled: true,
    shortcutsOpen: false,
    paletteOpen: false,
    settingsOpen: false,
    watchOpen: false,
    monitorZoom: "fit",
    monitorGuides: false,
    scopesOpen: false,
    scopeKind: "waveform",
  });
});
afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("preview panel", () => {
  it("shows the empty hint when nothing is on the timeline", () => {
    previewStore.set(tick({ hasClip: false, frameSeq: 0 }));
    mount();
    expect(screen.getByText(t("previewEmpty"))).toBeTruthy();
    expect(monitor().querySelector("img")).toBeNull();
  });

  it("shows the decode error with its detail as a tooltip and no frame", () => {
    previewStore.set(tick({ error: "decode failed" }));
    mount();
    expect(screen.getByText(t("previewUnavailable")).getAttribute("title")).toBe("decode failed");
    expect(monitor().querySelector("img")).toBeNull();
  });

  it("tells the user a compound clip is rendering, only while it is", () => {
    previewStore.set(tick({ renderingCompound: true }));
    mount();
    expect(screen.getByTestId("rendering-compound").textContent).toBe(
      t("previewRenderingCompound"),
    );
    cleanup();
    previewStore.set(tick({ renderingCompound: false }));
    mount();
    expect(screen.queryByTestId("rendering-compound")).toBeNull();
  });

  it("opens and closes the colour scopes strip, only while there is a picture", () => {
    previewStore.set(tick({ frameSeq: 3 }));
    mount();
    expect(screen.queryByTestId("scopes-panel")).toBeNull();
    fireEvent.click(screen.getByTestId("toggle-scopes"));
    expect(screen.getByTestId("scopes-panel")).toBeTruthy();
    expect(screen.getByTestId("toggle-scopes").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("toggle-scopes"));
    expect(screen.queryByTestId("scopes-panel")).toBeNull();

    // Nothing on the timeline: there is nothing to draw a scope of.
    fireEvent.click(screen.getByTestId("toggle-scopes"));
    act(() => {
      previewStore.set(tick({ hasClip: false, frameSeq: 0 }));
    });
    expect(screen.queryByTestId("scopes-panel")).toBeNull();
  });

  it("shows the current frame", () => {
    previewStore.set(tick({ frameSeq: 7 }));
    mount();
    expect(monitor().querySelector("img")!.getAttribute("src")).toBe("oca://frame/7");
  });

  it("renders the transport readout against the timeline length", () => {
    sessionStore.set(sessionWith(clip({ start_secs: 5, source_out_secs: 10 })));
    previewStore.set(tick({ playheadSecs: 3 }));
    const { container } = mount();
    expect(container.querySelector(".time-now")!.textContent).toContain("0:03");
    expect(container.querySelector(".time-total")!.textContent).toContain("0:15");
    const scrubber = screen.getByLabelText(t("seek")) as HTMLInputElement;
    expect(Number(scrubber.max)).toBe(15);
    expect(Number(scrubber.value)).toBe(3);
    fireEvent.change(scrubber, { target: { value: "7" } });
    expect(api.seek).toHaveBeenCalledWith(7);
    // Dragging the slider is a scrub too: on while the pointer is down, off on release or blur.
    fireEvent.pointerDown(scrubber);
    expect(api.setScrubbing).toHaveBeenLastCalledWith(true);
    fireEvent.pointerUp(scrubber);
    expect(api.setScrubbing).toHaveBeenLastCalledWith(false);
  });

  it("has a zero length without a session", () => {
    const { container } = mount();
    expect(container.querySelector(".time-total")!.textContent).toContain("0:00");
  });

  it("steps and toggles playback from the transport", () => {
    mount();
    fireEvent.click(screen.getByTitle(t("stepBack")));
    expect(api.step).toHaveBeenCalledWith(-1);
    fireEvent.click(screen.getByTitle(t("stepForward")));
    expect(api.step).toHaveBeenCalledWith(1);
    fireEvent.click(screen.getByTitle(t("play")));
    expect(api.setPlaying).toHaveBeenCalledWith(true);
    cleanup();

    previewStore.set(tick({ playing: true }));
    mount();
    fireEvent.click(screen.getByTitle(t("pause")));
    expect(api.setPlaying).toHaveBeenCalledWith(false);
  });

  it("stops observing the monitor on unmount", () => {
    const disconnect = vi.spyOn(FakeResizeObserver.prototype, "disconnect");
    const { unmount } = mount();
    unmount();
    expect(disconnect).toHaveBeenCalled();
  });

  describe("layer transform overlay", () => {
    beforeEach(() => {
      sessionStore.set(sessionWith(clip()));
      patchUi({ selectedClipIds: [1] });
    });

    it("only appears for a single selected clip", () => {
      patchUi({ selectedClipIds: [1, 2] });
      mount();
      expect(screen.queryByTestId("preview-layer")).toBeNull();
    });

    it("is a draggable layer for a clip with at most one position keyframe", () => {
      mount();
      const layer = screen.getByTestId("preview-layer");
      expect(layer.className).toContain("draggable");
      expect(layer.getAttribute("title")).toBe(t("layerTransformDragHint"));
      expect(screen.getByTestId("preview-layer-resize-handle")).toBeTruthy();
    });

    it("is read-only for an animated position", () => {
      sessionStore.set(
        sessionWith(
          clip({
            position_keyframes: [
              { time_fraction: 0, value: { x: 0, y: 0 } },
              { time_fraction: 1, value: { x: 1, y: 1 } },
            ],
          }),
        ),
      );
      mount();
      const layer = screen.getByTestId("preview-layer");
      expect(layer.className).toContain("animated");
      expect(layer.getAttribute("title")).toBe(t("layerTransformAnimatedHint"));
      expect(screen.queryByTestId("preview-layer-resize-handle")).toBeNull();
    });

    it("moves the layer, clamped to the canvas bounds, and commits one keyframe on release", () => {
      sessionStore.set(
        sessionWith(
          clip({ position_keyframes: [{ time_fraction: 0, value: { x: 0.5, y: 0.5 } }] }),
        ),
      );
      mount();
      const layer = screen.getByTestId("preview-layer");
      pointer(layer, "pointerdown", 100, 100);
      expect(Element.prototype.setPointerCapture).toHaveBeenCalled();
      pointer(monitor(), "pointermove", 100 + 800, 100 - 4500);
      pointer(monitor(), "pointerup");
      expect(api.updateClip).toHaveBeenCalledWith(1, {
        positionKeyframes: [{ time_fraction: 0, value: { x: 1.5, y: -1 } }],
      });
      expect(screen.getByTestId("preview-layer")).toBeTruthy();
    });

    it("commits nothing when the pointer is cancelled mid-drag", () => {
      sessionStore.set(
        sessionWith(
          clip({ position_keyframes: [{ time_fraction: 0, value: { x: 0.5, y: 0.5 } }] }),
        ),
      );
      mount();
      pointer(screen.getByTestId("preview-layer"), "pointerdown", 100, 100);
      pointer(monitor(), "pointermove", 300, 200);
      pointer(monitor(), "pointercancel");
      pointer(monitor(), "pointermove", 500, 300);
      pointer(monitor(), "pointerup");
      expect(api.updateClip).not.toHaveBeenCalled();
    });

    it("resizes from the handle without starting a move, never below the minimum scale", () => {
      mount();
      const handle = screen.getByTestId("preview-layer-resize-handle");
      pointer(handle, "pointerdown", 0, 0);
      pointer(monitor(), "pointermove", -5000, 10);
      pointer(monitor(), "pointerup");
      expect(api.updateClip).toHaveBeenCalledTimes(1);
      const patch = vi.mocked(api.updateClip).mock.calls[0][1] as {
        layerScaleX: number;
        layerScaleY: number;
      };
      expect(patch.layerScaleX).toBe(0.05);
      expect(patch.layerScaleY).toBeGreaterThan(1);
    });

    it("commits nothing when a drag ends without movement", () => {
      mount();
      pointer(screen.getByTestId("preview-layer"), "pointerdown", 10, 10);
      pointer(monitor(), "pointerup");
      expect(api.updateClip).not.toHaveBeenCalled();
    });

    it("ignores pointer movement when nothing is being dragged", () => {
      mount();
      pointer(monitor(), "pointermove", 50, 50);
      expect(api.updateClip).not.toHaveBeenCalled();
    });

    it("falls back to the 16:9 frame when the asset has no resolution", () => {
      sessionStore.set(sessionWith(clip(), null));
      mount();
      expect(screen.getByTestId("preview-layer")).toBeTruthy();
    });

    it("letterboxes a portrait export canvas inside a wide monitor", () => {
      const session = sessionWith(clip());
      session.project.sequences[0].export_settings.aspect_ratio = "Portrait";
      sessionStore.set(session);
      mount();
      const canvas = screen.getByTestId("preview-layer").parentElement as HTMLElement;
      expect(parseFloat(canvas.style.height)).toBe(450);
      expect(parseFloat(canvas.style.width)).toBeLessThan(450);
    });

    it("needs a measured monitor", () => {
      render(<PreviewPanel />);
      expect(screen.queryByTestId("preview-layer")).toBeNull();
    });

    it("is hidden while no frame has been produced", () => {
      previewStore.set(tick({ frameSeq: 0 }));
      mount();
      expect(screen.queryByTestId("preview-layer")).toBeNull();
    });
  });

  describe("text tool", () => {
    beforeEach(() => {
      return patchUi({ tool: "text" });
    });

    it("announces itself with a crosshair cursor and a hint", () => {
      mount();
      expect(monitor().style.cursor).toBe("crosshair");
      expect(monitor().getAttribute("title")).toBe(t("textToolHint"));
    });

    it("places a text clip where the canvas was clicked, selects it and returns to the select tool", async () => {
      mount();
      pointer(monitor(), "pointerup", 400, 225);
      await act(async () => {});
      expect(api.addTextClip).toHaveBeenCalledWith(t("defaultTextContent"), 0.5, 0.5);
      expect(uiStore.get().selectedClipIds).toEqual([9]);
      expect(uiStore.get().tool).toBe("select");
    });

    it("clamps clicks outside the canvas and still leaves the tool when creation fails", async () => {
      vi.mocked(api.addTextClip).mockRejectedValueOnce(new Error("no"));
      mount();
      pointer(monitor(), "pointerup", -900, 9000);
      await act(async () => {});
      expect(api.addTextClip).toHaveBeenCalledWith(t("defaultTextContent"), 0, 1);
      expect(uiStore.get().tool).toBe("select");
    });

    it("ignores clicks before the monitor is measured", () => {
      render(<PreviewPanel />);
      pointer(monitor(), "pointerup", 10, 10);
      expect(api.addTextClip).not.toHaveBeenCalled();
    });
  });

  it("does nothing on a plain click with the select tool", () => {
    mount();
    pointer(monitor(), "pointerup", 10, 10);
    expect(api.addTextClip).not.toHaveBeenCalled();
    expect(monitor().style.cursor).toBe("");
    expect(monitor().getAttribute("title")).toBeNull();
  });

  describe("custom shape drawing", () => {
    it("adds a vertex per click and reports the count", () => {
      patchUi({ drawingShapePoints: [] });
      mount();
      expect(monitor().getAttribute("title")).toBe(t("drawCustomShapeHint"));
      expect(screen.getByTestId("shape-drawing-surface").querySelector("path")).toBeNull();

      pointer(monitor(), "pointerup", 400, 225);
      expect(uiStore.get().drawingShapePoints).toEqual([[0.5, 0.5]]);
    });

    it("draws an open solid path for two points and a closed dashed one for three", () => {
      patchUi({
        drawingShapePoints: [
          [0, 0],
          [1, 1],
        ],
      });
      mount();
      const surface = screen.getByTestId("shape-drawing-surface");
      expect(surface.querySelectorAll("circle")).toHaveLength(2);
      expect(surface.querySelector("path")!.getAttribute("d")).not.toContain("Z");
      expect(surface.querySelector("path")!.getAttribute("stroke-dasharray")).toBeNull();
      expect(surface.textContent).toContain("2 pts");
      act(() => {
        return patchUi({
          drawingShapePoints: [
            [0, 0],
            [1, 1],
            [0, 1],
          ],
        });
      });
      expect(surface.querySelector("path")!.getAttribute("d")).toContain("Z");
      expect(surface.querySelector("path")!.getAttribute("stroke-dasharray")).toBe("4 3");
    });

    it("uses the singular for a single point", () => {
      patchUi({ drawingShapePoints: [[0.2, 0.2]] });
      mount();
      expect(screen.getByTestId("shape-drawing-surface").textContent).toContain("1 pt)");
    });

    it("hides the layer overlay while drawing", () => {
      sessionStore.set(sessionWith(clip()));
      patchUi({ selectedClipIds: [1], drawingShapePoints: [] });
      mount();
      expect(screen.queryByTestId("preview-layer")).toBeNull();
    });

    it("draws nothing until the monitor is measured", () => {
      patchUi({ drawingShapePoints: [] });
      render(<PreviewPanel />);
      expect(screen.queryByTestId("shape-drawing-surface")).toBeNull();
    });
  });
});

describe("monitor zoom and guides", () => {
  beforeEach(() => {
    sessionStore.set(sessionWith(clip()));
    previewStore.set(tick({ frameSeq: 3 }));
    uiStore.set({
      ...uiStore.get(),
      selectedClipIds: [1],
      monitorZoom: "fit",
      monitorGuides: false,
      scopesOpen: false,
      scopeKind: "waveform",
    });
  });

  it("offers Fit, 50% and 100% and marks the active one", () => {
    mount();
    const fit = screen.getByText(t("zoomFitShort"));
    expect(fit.getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByText("50%"));
    expect(uiStore.get().monitorZoom).toBe("50");
    expect(screen.getByText("50%").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByText("100%"));
    expect(uiStore.get().monitorZoom).toBe("100");
  });

  it("sizes the picture to a fraction of the canvas when zoomed", () => {
    uiStore.set({ ...uiStore.get(), monitorZoom: "50" });
    const { container } = mount();
    const img = container.querySelector(".monitor img") as HTMLImageElement;
    expect(img.style.width).toBe("960px");
    expect(img.style.height).toBe("540px");
  });

  it("leaves the picture to the browser's own fit at Fit", () => {
    const { container } = mount();
    expect(
      (container.querySelector(".monitor img") as HTMLImageElement).getAttribute("style"),
    ).toBeNull();
  });

  it("hides the layer handles while zoomed, since they assume the fitted canvas", () => {
    mount();
    expect(screen.queryByTestId("preview-layer")).toBeTruthy();
    cleanup();
    uiStore.set({ ...uiStore.get(), monitorZoom: "100" });
    mount();
    expect(screen.queryByTestId("preview-layer")).toBeNull();
  });

  it("toggles the safe-area guides over the canvas", () => {
    mount();
    expect(screen.queryByTestId("monitor-guides")).toBeNull();
    fireEvent.click(screen.getByLabelText(t("cmdToggleGuides")));
    expect(uiStore.get().monitorGuides).toBe(true);
    const guides = screen.getByTestId("monitor-guides");
    expect(guides.querySelectorAll(".guide-safe")).toHaveLength(2);
    expect(guides.querySelectorAll(".guide-line")).toHaveLength(2);
    expect(guides.getAttribute("aria-hidden")).toBe("true");
  });

  it("shows the timecode with the source frame rate", () => {
    const session = sessionWith(clip());
    (session.project as unknown as { media_library: unknown[] }).media_library = [
      { id: 1, kind: "Video", resolution: [1920, 1080], fps: 24 },
    ];
    sessionStore.set(session);
    previewStore.set(tick({ playheadSecs: 1.5 }));
    const { container } = mount();
    expect(container.querySelector(".time-now")!.textContent).toBe("00:00:01:12");
    expect(container.querySelector(".time-total")!.textContent).toBe("00:00:10:00");
  });
});

describe("monitor pan", () => {
  beforeEach(() => {
    sessionStore.set(sessionWith(clip()));
    previewStore.set(tick({ frameSeq: 3 }));
    uiStore.set({ ...uiStore.get(), selectedClipIds: [1], monitorZoom: "100" });
  });

  const imageLeft = (container: HTMLElement) => {
    return parseFloat((container.querySelector(".monitor img") as HTMLImageElement).style.left);
  };

  it("keeps a margin of the canvas in view on every side", () => {
    const canvas = { width: 1000, height: 500 };
    const monitor = { width: 400, height: 300 };
    expect(clampPan({ x: 0, y: 0 }, canvas, monitor)).toEqual({ x: 0, y: 0 });
    expect(clampPan({ x: 9999, y: -9999 }, canvas, monitor)).toEqual({ x: 652, y: -352 });
  });

  it("drags the zoomed picture and recenters on a double click", () => {
    const { container } = mount();
    const monitor = screen.getByTestId("preview-monitor");
    const before = imageLeft(container);
    fireEvent.pointerDown(monitor, { clientX: 100, clientY: 100, button: 0 });
    fireEvent.pointerMove(monitor, { clientX: 60, clientY: 90 });
    fireEvent.pointerUp(monitor, { clientX: 60, clientY: 90 });
    expect(imageLeft(container)).toBe(before - 40);

    fireEvent.doubleClick(monitor);
    expect(imageLeft(container)).toBe(before);
  });

  it("pans with the wheel and recenters when the zoom changes", () => {
    const { container } = mount();
    const monitor = screen.getByTestId("preview-monitor");
    const before = imageLeft(container);
    fireEvent.wheel(monitor, { deltaX: 30, deltaY: 0 });
    expect(imageLeft(container)).toBe(before - 30);
    act(() => {
      uiStore.set({ ...uiStore.get(), monitorZoom: "50" });
    });
    act(() => {
      uiStore.set({ ...uiStore.get(), monitorZoom: "100" });
    });
    expect(imageLeft(container)).toBe(before);
  });

  it("does not pan at Fit", () => {
    uiStore.set({ ...uiStore.get(), monitorZoom: "fit" });
    mount();
    const monitor = screen.getByTestId("preview-monitor");
    fireEvent.wheel(monitor, { deltaX: 30, deltaY: 30 });
    expect(monitor.title).not.toBe(t("monitorPanHint"));
  });
});

describe("shuttle badge", () => {
  beforeEach(() => {
    sessionStore.set(sessionWith(clip()));
    previewStore.set(tick({ frameSeq: 3 }));
  });
  afterEach(() => {
    shuttleStore.set({ direction: 0, level: 1 });
  });

  it("shows the speed while shuttling and hides it for plain forward play", () => {
    mount();
    expect(screen.queryByTestId("shuttle-badge")).toBeNull();
    act(() => {
      shuttleStore.set({ direction: 1, level: 1 });
    });
    expect(screen.queryByTestId("shuttle-badge")).toBeNull();
    act(() => {
      shuttleStore.set({ direction: -1, level: 4 });
    });
    expect(screen.getByTestId("shuttle-badge").textContent).toBe("-4x");
    act(() => {
      shuttleStore.set({ direction: 1, level: 2 });
    });
    expect(screen.getByTestId("shuttle-badge").textContent).toBe("2x");
  });
});
