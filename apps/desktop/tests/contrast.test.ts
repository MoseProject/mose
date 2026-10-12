// The palette's text-on-fill pairs must stay readable (WCAG AA, 4.5:1 for normal text). The colours
// are read from the stylesheet itself, so changing a token without checking its pairs fails here.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../src/styles/base.module.css", import.meta.url), "utf8");

function token(name: string): string {
  const match = css.match(new RegExp(`--${name}:\\s*(#[0-9a-fA-F]{6})`));
  if (!match) {
    throw new Error(`--${name} is not a #rrggbb token in base.module.css`);
  }
  return match[1];
}

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => {
    const channel = parseInt(hex.slice(i, i + 2), 16) / 255;
    return channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => {
    return y - x;
  });
  return (light + 0.05) / (dark + 0.05);
}

describe("palette contrast", () => {
  it("keeps the text on an accent fill readable in every state", () => {
    for (const fill of ["accent", "accent-hover"]) {
      expect(contrast(token("on-accent"), token(fill))).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("keeps the main text readable on the app background", () => {
    expect(contrast(token("text-primary"), token("bg"))).toBeGreaterThanOrEqual(4.5);
  });
});
