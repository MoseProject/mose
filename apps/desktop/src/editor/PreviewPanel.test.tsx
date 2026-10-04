// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { patchUi, previewStore, sessionStore, uiStore } from "../store";
import { makeSession } from "../testSupport";
import type { ClipInstance, PreviewTick } from "../types";

vi.mock("../ipc", () => {
  return {
    api: {
      updateClip: vi.fn().mockResolvedValue(undefined),
      addTextClip: vi.fn().mockResolvedValue(9),
      step: vi.fn().mockResolvedValue(undefined),
      seek: vi.fn().mockResolvedValue(undefined),
      setPlaying: vi.fn().mockResolvedValue(undefined),
    },
    frameUrl: (seq: number) => {
      return `oca://frame/${seq}`;
    },
  };
});

const { PreviewPanel } = await import("./PreviewPanel");
const { api } = await import("../ipc");

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
