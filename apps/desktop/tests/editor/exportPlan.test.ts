import { describe, expect, it } from "vitest";
import {
  customBitrateValid,
  exportOptions,
  formatSize,
  isUpscale,
  summarize,
} from "../../src/editor/export/exportPlan";

const canvas = { width: 1920, height: 1080, bitrateMbps: 16, durationSecs: 60 };

describe("export plan", () => {
  it("asks for nothing when quality and resolution are the original", () => {
    expect(exportOptions("source", 10, null)).toEqual({});
  });

  it("maps a quality step and a height to backend options", () => {
    expect(exportOptions("medium", 10, 720)).toEqual({ bitrateMbps: 8, maxHeight: 720 });
    expect(exportOptions("custom", 12.5, null)).toEqual({ bitrateMbps: 12.5 });
  });

  it("validates a custom bitrate against the encoder's range", () => {
    expect(customBitrateValid(0.5)).toBe(true);
    expect(customBitrateValid(200)).toBe(true);
    expect(customBitrateValid(0.4)).toBe(false);
    expect(customBitrateValid(Number.NaN)).toBe(false);
  });

  it("keeps the source numbers with no options", () => {
    const s = summarize(canvas, {});
    expect([s.width, s.height, s.bitrateMbps]).toEqual([1920, 1080, 16]);
    // (16 + 0.192) Mbps over 60 s.
    expect(s.sizeMb).toBeCloseTo(121.44, 1);
  });

  it("scales width and bitrate with a lower height, and scales up on a larger height", () => {
    const small = summarize(canvas, { maxHeight: 720 });
    expect([small.width, small.height]).toEqual([1280, 720]);
    expect(small.bitrateMbps).toBeCloseTo(7.11, 2);
    const up = summarize(canvas, { maxHeight: 2160 });
    expect([up.width, up.height, up.bitrateMbps]).toEqual([3840, 2160, 64]);
    const same = summarize(canvas, { maxHeight: 1080 });
    expect([same.width, same.height, same.bitrateMbps]).toEqual([1920, 1080, 16]);
  });

  it("lets an explicit bitrate win over the scaled one", () => {
    expect(summarize(canvas, { maxHeight: 720, bitrateMbps: 5 }).bitrateMbps).toBe(5);
  });

  it("formats sizes in MB and GB", () => {
    expect(formatSize(0.2)).toBe("1 MB");
    expect(formatSize(850.4)).toBe("850 MB");
    expect(formatSize(1536)).toBe("1.5 GB");
  });

  it("tells an upscale from a smaller or unknown size", () => {
    expect(isUpscale(canvas, 1440)).toBe(true);
    expect(isUpscale(canvas, 720)).toBe(false);
    expect(isUpscale(canvas, null)).toBe(false);
    expect(isUpscale(null, 1440)).toBe(false);
  });
});
