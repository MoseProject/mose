import { describe, expect, it } from "vitest";
import { PREVIEW_MAX_DIM, PREVIEW_MIN_DIM, previewMaxDim } from "./previewResolution";

describe("previewMaxDim", () => {
  it("rounds the on-screen size, in device pixels, up to a step", () => {
    expect(previewMaxDim(395, 222, 1)).toBe(480);
    expect(previewMaxDim(395, 222, 2)).toBe(800);
    expect(previewMaxDim(600, 340, 1.5)).toBe(960);
  });

  it("stays inside the backend limits", () => {
    expect(previewMaxDim(100, 60, 1)).toBe(PREVIEW_MIN_DIM);
    expect(previewMaxDim(3000, 1700, 2)).toBe(PREVIEW_MAX_DIM);
  });

  it("asks for the most when the size is unknown", () => {
    expect(previewMaxDim(0, 0, 1)).toBe(PREVIEW_MAX_DIM);
    expect(previewMaxDim(Number.NaN, 10, 1)).toBe(PREVIEW_MAX_DIM);
  });
});
