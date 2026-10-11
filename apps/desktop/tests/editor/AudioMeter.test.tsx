// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { previewStore } from "../../src/store";
import {
  AudioMeter,
  holdStore,
  loudnessState,
  nextHold,
  peakFill,
  scaleFill,
} from "../../src/editor/chrome/AudioMeter";

afterEach(() => {
  return cleanup();
});

describe("peakFill", () => {
  it("maps -60..0 dBFS onto 0..1", () => {
    expect(peakFill(1)).toBeCloseTo(1);
    expect(peakFill(0.5)).toBeCloseTo((20 * Math.log10(0.5) + 60) / 60);
    expect(peakFill(0.001)).toBeCloseTo(0);
  });

  it("clamps overs and silence", () => {
    expect(peakFill(4)).toBe(1);
    expect(peakFill(0)).toBe(0);
    expect(peakFill(-1)).toBe(0);
    expect(peakFill(Number.NaN)).toBe(0);
  });
});

describe("AudioMeter", () => {
  it("uncovers each bar in proportion to its channel's peak", () => {
    const { container } = render(<AudioMeter />);
    act(() => {
      return previewStore.set({ ...previewStore.get(), peakL: 1, peakR: 0 });
    });
    const covers = container.querySelectorAll<HTMLElement>(".meter-cover");
    expect(covers).toHaveLength(2);
    expect(parseFloat(covers[0].style.height)).toBeCloseTo(0);
    expect(parseFloat(covers[1].style.height)).toBeCloseTo(100);
  });

  it("labels the scale from 0 down to -60", () => {
    const { container } = render(<AudioMeter />);
    const marks = [...container.querySelectorAll(".meter-scale span")].map((span) => {
      return span.textContent;
    });
    expect(marks[0]).toBe("0");
    expect(marks[marks.length - 1]).toBe("-60");
  });
});

describe("nextHold", () => {
  const start = { left: 0, right: 0, leftAt: 0, rightAt: 0 };

  it("takes a higher peak at once", () => {
    const next = nextHold(start, 0.5, 0.25, 100);
    expect(next).toEqual({ left: 0.5, right: 0.25, leftAt: 100, rightAt: 100 });
  });

  it("keeps the held peak while a lower one arrives within the hold time", () => {
    const held = nextHold(start, 0.8, 0.8, 100);
    const next = nextHold(held, 0.2, 0.9, 600);
    expect(next.left).toBe(0.8);
    expect(next.leftAt).toBe(100);
    expect(next.right).toBe(0.9);
    expect(next.rightAt).toBe(600);
  });

  it("lets a lower peak replace the held one after the hold time", () => {
    const held = nextHold(start, 0.8, 0.8, 100);
    const next = nextHold(held, 0.2, 0.2, 100 + 1501);
    expect(next.left).toBe(0.2);
    expect(next.leftAt).toBe(1601);
  });
});

describe("peak hold marker", () => {
  it("draws a marker at the held level and turns it red at full scale", () => {
    holdStore.set({ left: 0, right: 0, leftAt: 0, rightAt: 0 });
    const { container } = render(<AudioMeter />);
    expect(container.querySelector(".meter-hold")).toBeNull();
    act(() => {
      holdStore.set({ left: 0.5, right: 1, leftAt: performance.now(), rightAt: performance.now() });
    });
    const marks = container.querySelectorAll<HTMLElement>(".meter-hold");
    expect(marks).toHaveLength(2);
    expect(parseFloat(marks[0].style.bottom)).toBeCloseTo(peakFill(0.5) * 100);
    expect(marks[0].className).not.toContain("clip");
    expect(marks[1].className).toContain("clip");
    act(() => {
      holdStore.set({ left: 0, right: 0, leftAt: 0, rightAt: 0 });
    });
  });

  it("is fed by the preview ticks", () => {
    act(() => {
      previewStore.set({ ...previewStore.get(), peakL: 0.7, peakR: 0.3 });
    });
    expect(holdStore.get().left).toBeGreaterThanOrEqual(0.7);
    expect(holdStore.get().right).toBeGreaterThanOrEqual(0.3);
  });
});

describe("loudness readout", () => {
  it("shows the momentary LUFS and a mark for each preset target", () => {
    const { getByTestId, getAllByTestId } = render(<AudioMeter />);
    expect(getByTestId("meter-lufs").textContent).toContain("-");
    act(() => {
      return previewStore.set({ ...previewStore.get(), momentaryLufs: -14.26 });
    });
    expect(getByTestId("meter-lufs").textContent).toContain("-14.3");
    expect(getAllByTestId("meter-target")).toHaveLength(3);
  });

  it("classifies a reading against the target with a tolerance", () => {
    expect(loudnessState(-14.5, -14)).toBe("ok");
    expect(loudnessState(-12, -14)).toBe("hot");
    expect(loudnessState(-18, -14)).toBe("low");
  });

  it("places a target on the same scale as the peaks", () => {
    expect(scaleFill(-60)).toBe(0);
    expect(scaleFill(0)).toBe(1);
    expect(scaleFill(-30)).toBeCloseTo(0.5);
    expect(scaleFill(-14)).toBeCloseTo(peakFill(10 ** (-14 / 20)));
  });
});
