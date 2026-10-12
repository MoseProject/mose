// QA sweep over the timeline's pure math: whatever the zoom, scroll or time, labels and ticks must
// stay well-formed and the functions must terminate.

import { describe, expect, it } from "vitest";
import {
  fitPxPerSec,
  rulerLabel,
  rulerTicks,
  snap,
  timeToX,
  xToTime,
  zoomAround,
} from "../src/timelineGeometry";

describe("rulerLabel", () => {
  it("never shows a fraction of a hundred or more", () => {
    // 1.996 rounds to .100 hundredths if the carry into the seconds is forgotten.
    for (const secs of [0.996, 1.996, 59.996, 59.999, 119.9951, 3599.999]) {
      expect(rulerLabel(secs)).not.toMatch(/\.\d{3,}$/);
      expect(rulerLabel(secs)).not.toMatch(/\.100$/);
    }
  });

  it("carries a rounded-up fraction into the seconds", () => {
    expect(rulerLabel(1.996)).toBe("0:02");
    expect(rulerLabel(59.999)).toBe("1:00");
    expect(rulerLabel(3599.9999)).toBe("1:00:00");
  });

  it("is monotonic across every hundredth of the first two minutes", () => {
    const parse = (label: string) => {
      const [minutes, rest] = label.split(":");
      return Number(minutes) * 60 + Number(rest);
    };
    let previous = -1;
    for (let hundredths = 0; hundredths <= 12000; hundredths += 1) {
      const value = parse(rulerLabel(hundredths / 100));
      expect(value).toBeGreaterThanOrEqual(previous);
      previous = value;
    }
  });
});

describe("rulerTicks", () => {
  it("terminates and stays inside the window for any zoom and scroll", () => {
    for (const pxPerSec of [0.001, 0.02, 1, 40, 500, 5000]) {
      for (const scroll of [0, 0.3, 17, 99999]) {
        const ticks = rulerTicks(pxPerSec, scroll, 1200);
        const end = scroll + 1200 / pxPerSec;
        expect(ticks.length).toBeLessThan(200);
        for (const tick of ticks) {
          expect(tick).toBeGreaterThanOrEqual(scroll - 1e-6);
          expect(tick).toBeLessThanOrEqual(end + 1e-6);
        }
      }
    }
  });

  it("does not hang on a zero or negative zoom", () => {
    expect(rulerTicks(0, 0, 1000).length).toBeLessThan(200);
    expect(rulerTicks(-5, 0, 1000).length).toBeLessThan(200);
    expect(rulerTicks(Number.NaN, 0, 1000).length).toBeLessThan(200);
  });

  it("keeps ticks spaced at least the minimum apart on screen", () => {
    const ticks = rulerTicks(37, 0, 2000);
    for (let i = 1; i < ticks.length; i += 1) {
      expect((ticks[i] - ticks[i - 1]) * 37).toBeGreaterThanOrEqual(79);
    }
  });
});

describe("coordinate mapping", () => {
  it("round-trips time through x for any zoom and scroll", () => {
    for (const pxPerSec of [0.05, 1, 40, 900]) {
      for (const scroll of [0, 12.5, 4000]) {
        for (const secs of [0, 0.001, 33.3, 5000]) {
          const back = xToTime(timeToX(secs, pxPerSec, scroll), pxPerSec, scroll);
          expect(back).toBeCloseTo(secs, 6);
        }
      }
    }
  });

  it("keeps the time under the pointer fixed while zooming", () => {
    for (const factor of [0.25, 0.8, 1.25, 4]) {
      for (const anchorX of [0, 100, 777]) {
        const before = xToTime(anchorX, 40, 30);
        const next = zoomAround(anchorX, 40, 30, factor, 1, 400);
        if (next.scrollSecs > 0) {
          expect(xToTime(anchorX, next.pxPerSec, next.scrollSecs)).toBeCloseTo(before, 6);
        }
        expect(next.pxPerSec).toBeGreaterThanOrEqual(1);
        expect(next.pxPerSec).toBeLessThanOrEqual(400);
        expect(next.scrollSecs).toBeGreaterThanOrEqual(0);
      }
    }
  });

  it("never scrolls to a negative time", () => {
    expect(zoomAround(500, 40, 0, 0.1, 1, 400).scrollSecs).toBe(0);
  });
});

describe("snap and fit", () => {
  it("picks the nearest candidate, and the first of two equally near", () => {
    expect(snap(5, [4, 6.5, 5.4], 1)).toBe(5.4);
    expect(snap(5, [4.5, 5.5], 1)).toBe(5.5);
    expect(snap(5, [9], 1)).toBe(5);
    expect(snap(5, [], 1)).toBe(5);
  });

  it("fits a duration into the view inside the allowed zoom range", () => {
    expect(fitPxPerSec(0, 800, 2, 300)).toBe(2);
    expect(fitPxPerSec(10, 0, 2, 300)).toBe(2);
    expect(fitPxPerSec(10, 10000, 2, 300)).toBe(300);
    expect(fitPxPerSec(100000, 800, 2, 300)).toBe(2);
    const fit = fitPxPerSec(120, 1224, 2, 300);
    expect(fit * 120).toBeLessThanOrEqual(1224);
  });
});
