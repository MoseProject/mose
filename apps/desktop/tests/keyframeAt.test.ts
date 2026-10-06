import { describe, expect, it } from "vitest";
import {
  hasKeyframeAt,
  playheadFraction,
  toggleKeyframe,
  valueAt,
  withKeyframe,
  withoutKeyframe,
} from "../src/keyframeAt";
import type { ClipInstance } from "../src/types";

const clip = (patch: Partial<ClipInstance> = {}) => {
  return {
    start_secs: 10,
    source_in_secs: 0,
    source_out_secs: 8,
    speed_factor: 2,
    ...patch,
  } as ClipInstance;
};

describe("playheadFraction", () => {
  it("maps the playhead onto the clip's own duration, speed included", () => {
    expect(playheadFraction(clip(), 10)).toBe(0);
    expect(playheadFraction(clip(), 12)).toBe(0.5);
    expect(playheadFraction(clip(), 14)).toBe(1);
  });

  it("is null outside the clip or for an empty one", () => {
    expect(playheadFraction(clip(), 9.9)).toBeNull();
    expect(playheadFraction(clip(), 14.1)).toBeNull();
    expect(playheadFraction(clip({ source_out_secs: 0 }), 10)).toBeNull();
  });
});

describe("valueAt", () => {
  const list = [
    { time_fraction: 0.2, value: 10 },
    { time_fraction: 0.6, value: 30 },
  ];

  it("uses the fallback for an empty list", () => {
    expect(valueAt([], 0.5, 7)).toBe(7);
  });

  it("holds the ends flat and interpolates between keyframes", () => {
    expect(valueAt(list, 0, 0)).toBe(10);
    expect(valueAt(list, 1, 0)).toBe(30);
    expect(valueAt(list, 0.4, 0)).toBeCloseTo(20);
  });

  it("does not depend on the list order", () => {
    expect(valueAt([...list].reverse(), 0.4, 0)).toBeCloseTo(20);
  });
});

describe("keyframe toggling", () => {
  const list = [{ time_fraction: 0.5, value: 1 }];

  it("detects a keyframe within the tolerance", () => {
    expect(hasKeyframeAt(list, 0.502)).toBe(true);
    expect(hasKeyframeAt(list, 0.6)).toBe(false);
  });

  it("adds in time order and leaves an existing one alone", () => {
    expect(
      withKeyframe(list, 0.1, 5).map((k) => {
        return k.time_fraction;
      }),
    ).toEqual([0.1, 0.5]);
    expect(withKeyframe(list, 0.5, 9)).toBe(list);
  });

  it("removes only the keyframe at that moment", () => {
    const two = [...list, { time_fraction: 0.9, value: 2 }];
    expect(withoutKeyframe(two, 0.5)).toEqual([{ time_fraction: 0.9, value: 2 }]);
    expect(withoutKeyframe(two, 0.1)).toEqual(two);
  });

  it("toggles: removes when present, adds when absent", () => {
    expect(toggleKeyframe(list, 0.5, 3)).toEqual([]);
    expect(toggleKeyframe(list, 0.8, 3)).toHaveLength(2);
  });
});
