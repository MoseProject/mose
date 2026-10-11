/** Gain range of the clip gain slider, in dB. */
export const GAIN_MIN_DB = -24;
export const GAIN_MAX_DB = 24;

/** Gain, in dB and in the slider's 0.5 dB steps, that brings a clip measured at `measuredLufs`
 *  to `targetLufs`. `null` when either value is unusable. */
export function normalizeGainDb(measuredLufs: number, targetLufs: number): number | null {
  if (!Number.isFinite(measuredLufs) || !Number.isFinite(targetLufs)) {
    return null;
  }
  const raw = Math.round((targetLufs - measuredLufs) * 2) / 2;
  return Math.min(GAIN_MAX_DB, Math.max(GAIN_MIN_DB, raw)) + 0;
}
