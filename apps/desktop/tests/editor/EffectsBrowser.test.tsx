// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { t } from "../../src/i18n";
import { makeSession } from "../support/testSupport";
import type { ClipInstance, SessionView } from "../../src/types";

vi.mock("../../src/ipc", () => {
  return { api: { updateClip: vi.fn().mockResolvedValue(undefined) } };
});

const { EffectsBrowser, filterCatalog } = await import("../../src/editor/library/EffectsBrowser");
const { api } = await import("../../src/ipc");
const { effectDragStore, patchUi } = await import("../../src/store");

const session = (): SessionView => {
  return makeSession([
    {
      id: 1,
      name: "Main",
      tracks: [
        {
          clips: [{ id: 7 } as ClipInstance, { id: 8 } as ClipInstance, { id: 9 } as ClipInstance],
          text_clips: [{ id: 20 }],
          shape_clips: [],
        },
      ],
    },
  ]);
};

beforeEach(() => {
  vi.clearAllMocks();
  patchUi({ selectedClipIds: [7] });
  effectDragStore.set(null);
});
afterEach(() => {
  cleanup();
});

describe("EffectsBrowser", () => {
  it("applies the picked effect to the selected video clip", () => {
    render(<EffectsBrowser session={session()} />);
    fireEvent.click(screen.getByTestId("effect-transitions-Slide"));
    expect(api.updateClip).toHaveBeenCalledWith(7, { transitionIn: "Slide" });
    fireEvent.click(screen.getByTestId("effect-masks-Circle"));
    expect(api.updateClip).toHaveBeenCalledWith(7, { maskShape: "Circle" });
  });

  it("applies it to every selected video clip and leaves overlays alone", async () => {
    patchUi({ selectedClipIds: [7, 9, 20] });
    render(<EffectsBrowser session={session()} />);
    fireEvent.click(screen.getByTestId("effect-transitions-Fade"));
    await vi.waitFor(() => {
      expect(api.updateClip).toHaveBeenCalledTimes(2);
    });
    expect(api.updateClip).toHaveBeenCalledWith(7, { transitionIn: "Fade" });
    expect(api.updateClip).toHaveBeenCalledWith(9, { transitionIn: "Fade" });
  });

  it("says what to do while no clip is selected, and does nothing on a click then", () => {
    patchUi({ selectedClipIds: [] });
    render(<EffectsBrowser session={session()} />);
    expect(screen.getByText(t("effectsNeedClip"))).toBeTruthy();
    fireEvent.click(screen.getByTestId("effect-transitions-Fade"));
    expect(api.updateClip).not.toHaveBeenCalled();
  });

  it("turns a press and a move into a drag the timeline can drop, and clears it on release", () => {
    vi.useFakeTimers();
    render(<EffectsBrowser session={session()} />);
    const item = screen.getByTestId("effect-transitions-Zoom");
    fireEvent.pointerDown(item, { button: 0, clientX: 10, clientY: 10 });
    fireEvent(window, new MouseEvent("pointermove", { clientX: 11, clientY: 10 }));
    expect(effectDragStore.get()).toBeNull();
    fireEvent(window, new MouseEvent("pointermove", { clientX: 60, clientY: 40 }));
    expect(effectDragStore.get()).toMatchObject({
      label: t("transitionZoom"),
      patch: { transitionIn: "Zoom" },
      clientX: 60,
    });
    expect(screen.getByText(t("transitionZoom"), { selector: "div" })).toBeTruthy();
    fireEvent(window, new MouseEvent("pointerup", { clientX: 60, clientY: 40 }));
    expect(effectDragStore.get()).not.toBeNull();
    vi.runAllTimers();
    expect(effectDragStore.get()).toBeNull();
    // The click that follows a drag released over the same button applies nothing.
    fireEvent.click(item);
    expect(api.updateClip).not.toHaveBeenCalled();
    vi.useRealTimers();
  });

  it("cancels a drag on Escape", () => {
    render(<EffectsBrowser session={session()} />);
    fireEvent.pointerDown(screen.getByTestId("effect-masks-Circle"), {
      button: 0,
      clientX: 0,
      clientY: 0,
    });
    fireEvent(window, new MouseEvent("pointermove", { clientX: 50, clientY: 50 }));
    expect(effectDragStore.get()).not.toBeNull();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(effectDragStore.get()).toBeNull();
  });

  it("filters by effect or folder name and says when nothing matches", () => {
    render(<EffectsBrowser session={session()} />);
    const search = screen.getByLabelText(t("searchEffects"));
    fireEvent.change(search, { target: { value: "multiply" } });
    expect(screen.getByTestId("effect-blend-Multiply")).toBeTruthy();
    expect(screen.queryByTestId("effect-transitions-Fade")).toBeNull();
    fireEvent.change(search, { target: { value: t("effectsMasks").toLowerCase() } });
    expect(screen.getByTestId("effect-masks-Circle")).toBeTruthy();
    expect(screen.getByTestId("effect-masks-RoundedRect")).toBeTruthy();
    fireEvent.change(search, { target: { value: "zzzz" } });
    expect(screen.getByText(t("effectsNoMatches"))).toBeTruthy();
  });
});

describe("filterCatalog", () => {
  const folders = [
    { id: "a", label: "Alpha", effects: [{ id: "1", label: "One", patch: {} }] },
    { id: "b", label: "Beta", effects: [{ id: "2", label: "Two", patch: {} }] },
  ];
  it("returns everything for a blank query and drops empty folders otherwise", () => {
    expect(filterCatalog(folders, "  ")).toBe(folders);
    expect(
      filterCatalog(folders, "two").map((f) => {
        return f.id;
      }),
    ).toEqual(["b"]);
  });
});
