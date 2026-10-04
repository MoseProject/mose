import { describe, expect, it } from "vitest";
import { hexToRgb, rgbToHex, shapePresetOf } from "./overlays";
import type { OverlayCatalog } from "./types";

const presets: OverlayCatalog["shapePresets"] = [
  { preset: "Ellipse", kind: "Ellipse" },
  {
    preset: "Rectangle",
    kind: {
      Polygon: [
        [-0.5, -0.5],
        [0.5, -0.5],
        [0.5, 0.5],
        [-0.5, 0.5],
      ],
    },
  },
  {
    preset: "Triangle",
    kind: {
      Polygon: [
        [0, -0.5],
        [0.5, 0.5],
        [-0.5, 0.5],
      ],
    },
  },
];

describe("shapePresetOf", () => {
  it("matches presets by exact geometry", () => {
    expect(shapePresetOf("Ellipse", presets)).toBe("Ellipse");
    expect(
      shapePresetOf(
        {
          Polygon: [
            [0, -0.5],
            [0.5, 0.5],
            [-0.5, 0.5],
          ],
        },
        presets,
      ),
    ).toBe("Triangle");
  });

  it("treats a polygon with a preset's vertex count but other positions as custom", () => {
    const nearlyRectangle = {
      Polygon: [
        [-0.4, -0.5],
        [0.5, -0.5],
        [0.5, 0.5],
        [-0.5, 0.5],
      ] as [number, number][],
    };
    expect(shapePresetOf(nearlyRectangle, presets)).toBe("Custom");
    expect(shapePresetOf("Ellipse", [])).toBe("Custom");
  });
});

describe("color conversion", () => {
  it("round-trips rgb through hex", () => {
    expect(rgbToHex([255, 220, 0, 255])).toBe("#ffdc00");
    expect(hexToRgb("#ffdc00")).toEqual([255, 220, 0]);
    expect(hexToRgb("red")).toBeNull();
  });
});
