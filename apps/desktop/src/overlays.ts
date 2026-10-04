// Pure helpers for text/shape overlay clips, shared by the timeline and the inspectors.

import type { OverlayCatalog, ShapeKind, ShapePreset } from "./types";

/** Which preset `kind` is, by exact vertex comparison against core's own preset geometry. */
export function shapePresetOf(
  kind: ShapeKind,
  presets: OverlayCatalog["shapePresets"],
): ShapePreset {
  const key = JSON.stringify(kind);
  return (
    presets.find((entry) => {
      return JSON.stringify(entry.kind) === key;
    })?.preset ?? "Custom"
  );
}

/** `#rrggbb` for an `<input type="color">` (alpha dropped). */
export function rgbToHex([r, g, b]: readonly number[]): string {
  return `#${[r, g, b]
    .map((part) => {
      return part.toString(16).padStart(2, "0");
    })
    .join("")}`;
}

/** Inverse of {@link rgbToHex}; `null` for anything that isn't `#rrggbb`. */
export function hexToRgb(hex: string): [number, number, number] | null {
  const match = /^#([0-9a-f]{2})([0-9a-f]{2})([0-9a-f]{2})$/i.exec(hex);
  if (!match) {
    return null;
  }
  return [parseInt(match[1], 16), parseInt(match[2], 16), parseInt(match[3], 16)];
}
