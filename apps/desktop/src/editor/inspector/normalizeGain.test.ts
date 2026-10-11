import { describe, expect, it } from "vitest";
import { normalizeGainDb } from "./normalizeGain";

describe("normalizeGainDb", () => {
  it("lifts a quiet clip to the target in half-dB steps", () => {
    expect(normalizeGainDb(-24.3, -14)).toBe(10.5);
  });
  it("lowers a loud clip", () => {
    expect(normalizeGainDb(-8, -14)).toBe(-6);
  });
  it("is zero when already on target", () => {
    expect(Object.is(normalizeGainDb(-14, -14), 0)).toBe(true);
  });
  it("clamps to the gain slider range", () => {
    expect(normalizeGainDb(-60, -14)).toBe(24);
    expect(normalizeGainDb(10, -40)).toBe(-24);
  });
  it("rejects unmeasured or invalid values", () => {
    expect(normalizeGainDb(Number.NaN, -14)).toBeNull();
    expect(normalizeGainDb(-20, Number.POSITIVE_INFINITY)).toBeNull();
  });
});
