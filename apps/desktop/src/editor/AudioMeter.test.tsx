// @vitest-environment jsdom
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { previewStore } from "../store";
import { AudioMeter, peakFill } from "./AudioMeter";

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
