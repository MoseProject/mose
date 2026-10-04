import { describe, expect, it } from "vitest";
import { buildPyramid, frameStepSecs, peakRange } from "./timelineMedia";

describe("frameStepSecs", () => {
  it("picks the coarsest step that still gives each tile its own frame", () => {
    expect(frameStepSecs(0.05)).toBe(0.1);
    expect(frameStepSecs(2.25)).toBe(2);
    expect(frameStepSecs(640)).toBe(600);
    expect(frameStepSecs(1e9)).toBe(3600);
  });
});

describe("peak pyramid", () => {
  // 64 buckets over 64 s: bucket i spans [i, i + 1) and peaks at +-(i / 64).
  const pairs = new Float32Array(128);
  for (let i = 0; i < 64; i += 1) {
    pairs[2 * i] = -i / 64;
    pairs[2 * i + 1] = i / 64;
  }
  const pyramid = buildPyramid(pairs, 64)!;
  const out: [number, number] = [0, 0];

  it("folds buckets into coarser levels", () => {
    expect(pyramid.bucketsPerSec).toBe(1);
    expect(pyramid.levels.length).toBeGreaterThan(1);
    expect(pyramid.levels[1].length >> 1).toBe(8);
  });

  it("reads exact buckets at a fine range", () => {
    expect(peakRange(pyramid, 10, 11, out)).toBe(true);
    expect(out).toEqual([-10 / 64, 10 / 64]);
  });

  it("covers the whole range when zoomed out", () => {
    expect(peakRange(pyramid, 0, 64, out)).toBe(true);
    expect(out[0]).toBeCloseTo(-63 / 64);
    expect(out[1]).toBeCloseTo(63 / 64);
  });

  it("reports no data past the end and builds nothing from nothing", () => {
    expect(peakRange(pyramid, 100, 101, out)).toBe(false);
    expect(buildPyramid(new Float32Array(0), 10)).toBeNull();
    expect(buildPyramid(pairs, 0)).toBeNull();
  });
});
