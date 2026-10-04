import { describe, expect, it } from "vitest";
import { activeTimeline, aspectRatioDims, clipDuration } from "./types";
import type { ClipInstance, Project } from "./types";

describe("clipDuration", () => {
  const clip = (sourceIn: number, sourceOut: number, speed: number) => {
    return {
      source_in_secs: sourceIn,
      source_out_secs: sourceOut,
      speed_factor: speed,
    } as ClipInstance;
  };

  it("divides the trimmed source length by the speed", () => {
    expect(clipDuration(clip(1, 5, 1))).toBe(4);
    expect(clipDuration(clip(0, 6, 2))).toBe(3);
    expect(clipDuration(clip(0, 6, 0.5))).toBe(12);
  });

  it("never divides by zero for a stalled clip", () => {
    expect(Number.isFinite(clipDuration(clip(0, 1, 0)))).toBe(true);
  });
});

describe("activeTimeline", () => {
  it("returns the timeline of the active sequence", () => {
    const project = {
      active_sequence: 1,
      sequences: [{ timeline: { id: "first" } }, { timeline: { id: "second" } }],
    } as unknown as Project;
    expect(activeTimeline(project)).toEqual({ id: "second" });
  });
});

describe("aspectRatioDims", () => {
  it("returns the fixed export canvas for each preset", () => {
    expect(aspectRatioDims("Landscape", [1, 1])).toEqual([1920, 1080]);
    expect(aspectRatioDims("Portrait", [1, 1])).toEqual([1080, 1920]);
    expect(aspectRatioDims("Square", [1, 1])).toEqual([1080, 1080]);
  });

  it("keeps the source canvas for the original ratio", () => {
    expect(aspectRatioDims("Original", [1280, 720])).toEqual([1280, 720]);
  });
});
