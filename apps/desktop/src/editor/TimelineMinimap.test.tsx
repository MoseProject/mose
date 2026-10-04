// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { Track } from "../types";
import { TimelineMinimap } from "./TimelineMinimap";

afterEach(() => {
  cleanup();
});

const clip = (start: number, length: number) => {
  return {
    id: start,
    start_secs: start,
    source_in_secs: 0,
    source_out_secs: length,
    speed_factor: 1,
  };
};
const tracks = [
  { id: 1, kind: "Video", clips: [clip(0, 50), clip(60, 40)], text_clips: [], shape_clips: [] },
  {
    id: 2,
    kind: "Text",
    clips: [],
    text_clips: [{ start_secs: 10, duration_secs: 5 }],
    shape_clips: [],
  },
] as unknown as Track[];

const mount = (scrollSecs = 0, viewSecs = 20, onScrollTo = vi.fn()) => {
  const view = render(
    <TimelineMinimap
      tracks={tracks}
      scrollSecs={scrollSecs}
      viewSecs={viewSecs}
      onScrollTo={onScrollTo}
    />,
  );
  const bar = screen.getByRole("scrollbar");
  bar.getBoundingClientRect = () => {
    return { left: 100, width: 200, top: 0, height: 22, right: 300, bottom: 22 } as DOMRect;
  };
  bar.setPointerCapture = vi.fn();
  bar.hasPointerCapture = vi.fn(() => {
    return true;
  });
  return { ...view, bar, onScrollTo };
};

describe("TimelineMinimap", () => {
  it("draws every clip and overlay over the whole sequence", () => {
    const { container } = mount();
    expect(container.querySelectorAll(".minimap-clip")).toHaveLength(3);
    const first = container.querySelector(".minimap-clip") as HTMLElement;
    expect(first.style.left).toBe("0%");
    expect(parseFloat(first.style.width)).toBeCloseTo(50);
    expect(container.querySelector(".minimap-clip.text")).toBeTruthy();
  });

  it("shows the visible window as a share of the total", () => {
    const { container } = mount(30, 20);
    const window_ = container.querySelector(".minimap-window") as HTMLElement;
    expect(parseFloat(window_.style.left)).toBeCloseTo(30);
    expect(parseFloat(window_.style.width)).toBeCloseTo(20);
  });

  it("describes itself as a horizontal scrollbar", () => {
    const { bar } = mount(30, 20);
    expect(bar.getAttribute("aria-orientation")).toBe("horizontal");
    expect(bar.getAttribute("aria-valuenow")).toBe("30");
    expect(bar.getAttribute("aria-valuemax")).toBe("100");
  });

  it("centres the window on a click", () => {
    const { bar, onScrollTo } = mount(0, 20);
    fireEvent.pointerDown(bar, { clientX: 200, pointerId: 1 });
    // The middle of the bar is 50 s of 100; centred on a 20 s window that is 40 s.
    expect(onScrollTo).toHaveBeenCalledWith(40);
  });

  it("follows a drag and never scrolls before zero", () => {
    const { bar, onScrollTo } = mount(0, 20);
    fireEvent.pointerDown(bar, { clientX: 100, pointerId: 1 });
    expect(onScrollTo).toHaveBeenLastCalledWith(0);
    fireEvent.pointerMove(bar, { clientX: 250, pointerId: 1 });
    expect(onScrollTo).toHaveBeenLastCalledWith(65);
    fireEvent.pointerMove(bar, { clientX: 9999, pointerId: 1 });
    expect(onScrollTo).toHaveBeenLastCalledWith(90);
  });

  it("makes room when the window extends past the last clip", () => {
    const { bar } = mount(150, 40);
    expect(bar.getAttribute("aria-valuemax")).toBe("190");
  });
});
