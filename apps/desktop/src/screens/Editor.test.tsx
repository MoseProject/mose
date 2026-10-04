// @vitest-environment jsdom
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../i18n";
import { previewStore, uiStore } from "../store";
import { makeSession } from "../testSupport";
import type { SessionView } from "../types";

vi.mock("../editor/Inspector", () => {
  return {
    Inspector: () => {
      return <div data-testid="inspector" />;
    },
  };
});
vi.mock("../editor/MediaLibrary", () => {
  return {
    MediaLibrary: () => {
      return <div data-testid="library" />;
    },
  };
});
vi.mock("../editor/PreviewPanel", () => {
  return {
    PreviewPanel: () => {
      return <div data-testid="preview" />;
    },
  };
});
vi.mock("../editor/Timeline", () => {
  return {
    Timeline: () => {
      return <div data-testid="timeline" />;
    },
  };
});

type DragDropHandler = (event: { payload: { type: string; paths?: string[] } }) => void;
const webview = vi.hoisted(() => {
  return { handler: null as unknown, unlisten: null as unknown };
});
vi.mock("@tauri-apps/api/webview", () => {
  return {
    getCurrentWebview: () => {
      return {
        onDragDropEvent: (handler: DragDropHandler) => {
          webview.handler = handler;
          return Promise.resolve(webview.unlisten);
        },
      };
    },
  };
});

vi.mock("../ipc", () => {
  const names = [
    "setPlaying",
    "redo",
    "undo",
    "saveProject",
    "importMediaDialog",
    "copyClipFormatting",
    "pasteClipFormatting",
    "copyClip",
    "cutClip",
    "splitAtPlayhead",
    "addMarker",
    "deleteClips",
    "step",
    "seek",
    "importMedia",
    "setPanelLayout",
  ];
  const api: Record<string, unknown> = Object.fromEntries(
    names.map((n) => {
      return [n, vi.fn().mockResolvedValue(undefined)];
    }),
  );
  api.pasteClip = vi.fn().mockResolvedValue([3, 4]);
  api.addCustomShapeClip = vi.fn().mockResolvedValue(8);
  return { api };
});

const { Editor } = await import("./Editor");
const { api } = await import("../ipc");
const calls = api as unknown as Record<string, ReturnType<typeof vi.fn>>;

const sessionWith = (id: number, layout?: Record<string, number>): SessionView => {
  const session = makeSession([{ id: 1, name: "Main" }]);
  Object.assign(session.project, { id, panel_layout: layout });
  return session;
};

const screenEditor = () => {
  return screen.getByTestId("screen-editor");
};
const cssVar = (name: string) => {
  return screenEditor().style.getPropertyValue(name);
};
const emit = (payload: { type: string; paths?: string[] }) => {
  return act(() => {
    return (webview.handler as DragDropHandler)({ payload });
  });
};
const press = (init: KeyboardEventInit & { target?: EventTarget }) => {
  const event = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => {
    return void (init.target ?? window).dispatchEvent(event);
  });
  return event;
};
const pointer = (target: Element, type: string, x = 0, y = 0) => {
  return fireEvent(
    target,
    new MouseEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 }),
  );
};

beforeEach(() => {
  vi.clearAllMocks();
  webview.unlisten = vi.fn();
  webview.handler = null;
  Element.prototype.setPointerCapture = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "clientHeight", {
    configurable: true,
    get: () => {
      return 1000;
    },
  });
  uiStore.set({
    screen: "editor",
    selectedClipIds: [],
    tool: "select",
    pxPerSec: 40,
    error: null,
    drawingShapePoints: null,
    exportQueueOpen: false,
  });
  previewStore.set({
    frameSeq: 0,
    playheadSecs: 0,
    playing: false,
    hasClip: false,
    available: true,
    error: null,
    peakL: 0,
    peakR: 0,
  });
});
afterEach(() => {
  cleanup();
  delete (HTMLElement.prototype as unknown as Record<string, unknown>).clientHeight;
});

describe("editor screen", () => {
  it("composes every panel", () => {
    render(<Editor session={sessionWith(1)} />);
    for (const id of ["library", "preview", "inspector", "timeline"]) {
      expect(screen.getByTestId(id)).toBeTruthy();
    }
  });

  describe("layout", () => {
    it("uses the defaults when the project stores no layout", () => {
      render(<Editor session={sessionWith(1)} />);
      expect(cssVar("--library-width")).toBe("340px");
      expect(cssVar("--inspector-width")).toBe("296px");
      expect(cssVar("--timeline-height")).toBe("392px");
    });

    it("restores the stored layout within its bounds", () => {
      render(
        <Editor
          session={sessionWith(1, {
            lib_panel_width: 9999,
            props_panel_width: 10,
            timeline_height: 50,
          })}
        />,
      );
      expect(cssVar("--library-width")).toBe("640px");
      expect(cssVar("--inspector-width")).toBe("180px");
      expect(cssVar("--timeline-height")).toBe("140px");
    });

    it("reloads the layout when another project opens", () => {
      const { rerender } = render(<Editor session={sessionWith(1, { lib_panel_width: 300 })} />);
      expect(cssVar("--library-width")).toBe("300px");
      rerender(<Editor session={sessionWith(2, { lib_panel_width: 400 })} />);
      expect(cssVar("--library-width")).toBe("400px");
    });
  });

  describe("resizing", () => {
    it("drags the timeline splitter within its bounds and saves the layout on release", () => {
      const { container } = render(<Editor session={sessionWith(1)} />);
      const splitter = container.querySelector(".splitter")!;
      pointer(splitter, "pointerdown", 0, 500);
      pointer(splitter, "pointermove", 0, 400);
      expect(cssVar("--timeline-height")).toBe("492px");
      pointer(splitter, "pointermove", 0, -5000);
      expect(cssVar("--timeline-height")).toBe("800px");
      pointer(splitter, "pointermove", 0, 9000);
      expect(cssVar("--timeline-height")).toBe("140px");
      pointer(splitter, "pointerup", 0, 9000);
      expect(calls.setPanelLayout).toHaveBeenCalledWith(340, 296, 140);

      pointer(splitter, "pointermove", 0, 0);
      pointer(splitter, "pointerup", 0, 0);
      expect(calls.setPanelLayout).toHaveBeenCalledTimes(1);
    });

    it("limits the timeline height by the editor height", () => {
      const { container } = render(<Editor session={sessionWith(1)} />);
      const splitter = container.querySelector(".splitter")!;
      pointer(splitter, "pointerdown", 0, 500);
      pointer(splitter, "pointermove", 0, 0);
      expect(cssVar("--timeline-height")).toBe("800px");
    });

    it("drags the library splitter", () => {
      const { container } = render(<Editor session={sessionWith(1)} />);
      const splitter = container.querySelector(".library-splitter")!;
      pointer(splitter, "pointerdown", 100, 0);
      pointer(splitter, "pointermove", 200, 0);
      expect(cssVar("--library-width")).toBe("440px");
      expect(cssVar("--inspector-width")).toBe("296px");
      pointer(splitter, "pointermove", 5000, 0);
      expect(cssVar("--library-width")).toBe("640px");
      pointer(splitter, "pointermove", -5000, 0);
      expect(cssVar("--library-width")).toBe("160px");
      pointer(splitter, "pointerup", -5000, 0);
      expect(calls.setPanelLayout).toHaveBeenCalledWith(160, 296, 392);
    });

    it("drags the inspector splitter the opposite way", () => {
      const { container } = render(<Editor session={sessionWith(1)} />);
      const splitter = container.querySelector(".inspector-splitter")!;
      pointer(splitter, "pointerdown", 100, 0);
      pointer(splitter, "pointermove", 150, 0);
      expect(cssVar("--inspector-width")).toBe("246px");
      expect(cssVar("--library-width")).toBe("340px");
      pointer(splitter, "pointermove", 5000, 0);
      expect(cssVar("--inspector-width")).toBe("180px");
      pointer(splitter, "pointermove", -5000, 0);
      expect(cssVar("--inspector-width")).toBe("640px");
      pointer(splitter, "pointerup", -5000, 0);
      expect(calls.setPanelLayout).toHaveBeenCalledWith(340, 640, 392);
    });
  });

  describe("OS file drops", () => {
    it("shows the overlay while a drag is over the window and hides it afterwards", () => {
      render(<Editor session={sessionWith(1)} />);
      expect(screen.queryByText(t("dropToImport"))).toBeNull();
      emit({ type: "enter" });
      expect(screen.getByText(t("dropToImport"))).toBeTruthy();
      emit({ type: "over" });
      expect(screen.getByText(t("dropToImport"))).toBeTruthy();
      emit({ type: "leave" });
      expect(screen.queryByText(t("dropToImport"))).toBeNull();
    });

    it("imports the dropped files", () => {
      render(<Editor session={sessionWith(1)} />);
      emit({ type: "enter" });
      emit({ type: "drop", paths: ["C:/a.mp4", "C:/b.mp4"] });
      expect(calls.importMedia).toHaveBeenCalledWith(["C:/a.mp4", "C:/b.mp4"]);
      expect(screen.queryByText(t("dropToImport"))).toBeNull();
    });

    it("imports nothing for an empty drop", () => {
      render(<Editor session={sessionWith(1)} />);
      emit({ type: "drop", paths: [] });
      expect(calls.importMedia).not.toHaveBeenCalled();
    });

    it("stops listening on unmount", async () => {
      const { unmount } = render(<Editor session={sessionWith(1)} />);
      unmount();
      await act(async () => {});
      expect(webview.unlisten).toHaveBeenCalled();
    });
  });

  describe("keyboard", () => {
    beforeEach(() => {
      render(<Editor session={sessionWith(1)} />);
      uiStore.set({ ...uiStore.get(), selectedClipIds: [7, 8] });
    });

    it("toggles playback with space", () => {
      expect(press({ code: "Space", key: " " }).defaultPrevented).toBe(true);
      expect(calls.setPlaying).toHaveBeenLastCalledWith(true);
      previewStore.set({ ...previewStore.get(), playing: true });
      press({ code: "Space", key: " " });
      expect(calls.setPlaying).toHaveBeenLastCalledWith(false);
    });

    it("undoes and redoes", () => {
      press({ key: "z", ctrlKey: true });
      expect(calls.undo).toHaveBeenCalledTimes(1);
      press({ key: "Z", ctrlKey: true, shiftKey: true });
      expect(calls.redo).toHaveBeenCalledTimes(1);
      press({ key: "y", metaKey: true });
      expect(calls.redo).toHaveBeenCalledTimes(2);
    });

    it("saves, and saves as with shift", () => {
      press({ key: "s", ctrlKey: true });
      expect(calls.saveProject).toHaveBeenLastCalledWith(false);
      press({ key: "S", ctrlKey: true, shiftKey: true });
      expect(calls.saveProject).toHaveBeenLastCalledWith(true);
    });

    it("opens the import dialog", () => {
      press({ key: "i", ctrlKey: true });
      expect(calls.importMediaDialog).toHaveBeenCalled();
    });

    it("copies, cuts and pastes clips and their formatting", async () => {
      press({ key: "c", ctrlKey: true });
      expect(calls.copyClip).toHaveBeenCalledWith(7);
      press({ key: "x", ctrlKey: true });
      expect(calls.cutClip).toHaveBeenCalledWith(7);
      press({ key: "C", ctrlKey: true, shiftKey: true });
      expect(calls.copyClipFormatting).toHaveBeenCalledWith(7);
      press({ key: "V", ctrlKey: true, shiftKey: true });
      expect(calls.pasteClipFormatting).toHaveBeenCalledWith(7);

      press({ key: "v", ctrlKey: true });
      await act(async () => {});
      expect(calls.pasteClip).toHaveBeenCalled();
      expect(uiStore.get().selectedClipIds).toEqual([3, 4]);
    });

    it("keeps the selection when a paste produced nothing", async () => {
      calls.pasteClip.mockResolvedValueOnce([]);
      press({ key: "v", ctrlKey: true });
      await act(async () => {});
      calls.pasteClip.mockResolvedValueOnce(undefined);
      press({ key: "v", ctrlKey: true });
      await act(async () => {});
      expect(uiStore.get().selectedClipIds).toEqual([7, 8]);
    });

    it("does nothing for clipboard shortcuts without a selection", () => {
      uiStore.set({ ...uiStore.get(), selectedClipIds: [] });
      press({ key: "c", ctrlKey: true });
      press({ key: "x", ctrlKey: true });
      press({ key: "C", ctrlKey: true, shiftKey: true });
      press({ key: "V", ctrlKey: true, shiftKey: true });
      press({ key: "Delete" });
      expect(calls.copyClip).not.toHaveBeenCalled();
      expect(calls.cutClip).not.toHaveBeenCalled();
      expect(calls.copyClipFormatting).not.toHaveBeenCalled();
      expect(calls.pasteClipFormatting).not.toHaveBeenCalled();
      expect(calls.deleteClips).not.toHaveBeenCalled();
    });

    it("splits, marks and picks the text tool", () => {
      press({ key: "s" });
      expect(calls.splitAtPlayhead).toHaveBeenCalledWith([7, 8]);
      press({ key: "m" });
      expect(calls.addMarker).toHaveBeenCalled();
      press({ key: "t" });
      expect(uiStore.get().tool).toBe("text");
    });

    it("deletes the selection, rippling with shift", () => {
      press({ key: "Delete" });
      expect(calls.deleteClips).toHaveBeenLastCalledWith([7, 8], false);
      press({ key: "Backspace", shiftKey: true });
      expect(calls.deleteClips).toHaveBeenLastCalledWith([7, 8], true);
    });

    it("steps frames and jumps to the start", () => {
      press({ key: "ArrowLeft" });
      expect(calls.step).toHaveBeenLastCalledWith(-1);
      press({ key: "ArrowLeft", shiftKey: true });
      expect(calls.step).toHaveBeenLastCalledWith(-10);
      press({ key: "ArrowRight" });
      expect(calls.step).toHaveBeenLastCalledWith(1);
      press({ key: "ArrowRight", shiftKey: true });
      expect(calls.step).toHaveBeenLastCalledWith(10);
      press({ key: "Home" });
      expect(calls.seek).toHaveBeenCalledWith(0);
    });

    it("leaves unbound keys alone", () => {
      expect(press({ key: "q" }).defaultPrevented).toBe(false);
    });

    it("ignores keys typed into form fields", () => {
      const input = document.createElement("input");
      const select = document.createElement("select");
      document.body.append(input, select);
      press({ key: "s", target: input });
      press({ key: "m", target: select });
      expect(calls.splitAtPlayhead).not.toHaveBeenCalled();
      expect(calls.addMarker).not.toHaveBeenCalled();
      input.remove();
      select.remove();
    });

    it("stops handling keys after unmount", () => {
      cleanup();
      press({ key: "m" });
      expect(calls.addMarker).not.toHaveBeenCalled();
    });
  });

  describe("drawing a custom shape", () => {
    const triangle: [number, number][] = [
      [0, 0],
      [1, 0],
      [0.5, 1],
    ];
    beforeEach(() => {
      render(<Editor session={sessionWith(1)} />);
    });

    it("finishes with enter once there are three points and selects the new clip", async () => {
      uiStore.set({ ...uiStore.get(), drawingShapePoints: triangle });
      expect(press({ key: "Enter" }).defaultPrevented).toBe(true);
      await act(async () => {});
      expect(calls.addCustomShapeClip).toHaveBeenCalledWith(triangle);
      expect(uiStore.get().drawingShapePoints).toBeNull();
      expect(uiStore.get().selectedClipIds).toEqual([8]);
    });

    it("leaves drawing mode even when creating the clip fails", async () => {
      calls.addCustomShapeClip.mockRejectedValueOnce(new Error("no"));
      uiStore.set({ ...uiStore.get(), drawingShapePoints: triangle, selectedClipIds: [1] });
      press({ key: "Enter" });
      await act(async () => {});
      expect(uiStore.get().drawingShapePoints).toBeNull();
      expect(uiStore.get().selectedClipIds).toEqual([1]);
    });

    it("needs three points to finish", () => {
      uiStore.set({
        ...uiStore.get(),
        drawingShapePoints: [
          [0, 0],
          [1, 1],
        ],
      });
      press({ key: "Enter" });
      expect(calls.addCustomShapeClip).not.toHaveBeenCalled();
      expect(uiStore.get().drawingShapePoints).toHaveLength(2);
    });

    it("cancels with escape", () => {
      uiStore.set({ ...uiStore.get(), drawingShapePoints: triangle });
      press({ key: "Escape" });
      expect(uiStore.get().drawingShapePoints).toBeNull();
    });

    it("swallows every other shortcut while drawing", () => {
      uiStore.set({ ...uiStore.get(), drawingShapePoints: triangle });
      expect(press({ code: "Space", key: " " }).defaultPrevented).toBe(true);
      press({ key: "m" });
      expect(calls.setPlaying).not.toHaveBeenCalled();
      expect(calls.addMarker).not.toHaveBeenCalled();
    });
  });
});
