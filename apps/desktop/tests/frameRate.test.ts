import { describe, expect, it } from "vitest";
import {
  COMMON_FRAME_RATES,
  FREE_KEY,
  frameRateFromKey,
  frameRateKey,
  frameRateText,
} from "../src/frameRate";

describe("frame rates", () => {
  it("round-trips every common rate through its picker key", () => {
    for (const rate of COMMON_FRAME_RATES) {
      expect(frameRateFromKey(frameRateKey(rate))).toEqual(rate);
    }
  });

  it("treats no rate as the free key and the free key as no rate", () => {
    expect(frameRateKey(null)).toBe(FREE_KEY);
    expect(frameRateFromKey(FREE_KEY)).toBeNull();
  });

  it("refuses keys that are not a positive fraction", () => {
    for (const key of ["", "abc", "30", "30/0", "0/1", "-30/1", "30/1/1", "30 / 1"]) {
      expect(frameRateFromKey(key), key).toBeNull();
    }
  });

  it("writes rates the way editors do", () => {
    const text = COMMON_FRAME_RATES.map((rate) => {
      return frameRateText(rate);
    });
    expect(text).toEqual([
      "23.976 fps",
      "24 fps",
      "25 fps",
      "29.97 fps",
      "30 fps",
      "50 fps",
      "59.94 fps",
      "60 fps",
    ]);
  });

  it("offers exactly the rates the backend knows", () => {
    expect(COMMON_FRAME_RATES).toHaveLength(8);
  });
});
