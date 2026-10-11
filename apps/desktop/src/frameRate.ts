// The timeline frame rates the editor offers, and how one is written in a picker. The rates
// are exact fractions so 29.97 is 30000/1001; the backend snaps edits to whole frames of it.

import type { FrameRate } from "./types";

export const COMMON_FRAME_RATES: FrameRate[] = [
  { num: 24000, den: 1001 },
  { num: 24, den: 1 },
  { num: 25, den: 1 },
  { num: 30000, den: 1001 },
  { num: 30, den: 1 },
  { num: 50, den: 1 },
  { num: 60000, den: 1001 },
  { num: 60, den: 1 },
];

/** The picker value that means "no frame rate". */
export const FREE_KEY = "free";

export function frameRateKey(rate: FrameRate | null): string {
  return rate ? rate.num + "/" + rate.den : FREE_KEY;
}

/** The rate a picker value stands for; `null` for "free" and for anything unreadable. */
export function frameRateFromKey(key: string): FrameRate | null {
  const match = /^(\d+)\/(\d+)$/.exec(key);
  if (!match) {
    return null;
  }
  const num = Number(match[1]);
  const den = Number(match[2]);
  return num > 0 && den > 0 ? { num, den } : null;
}

/** `29.97 fps`, `24 fps`. */
export function frameRateText(rate: FrameRate): string {
  const fps = rate.num / rate.den;
  const text = Number.isInteger(fps) ? String(fps) : fps.toFixed(3).replace(/0+$/, "");
  return text + " fps";
}
