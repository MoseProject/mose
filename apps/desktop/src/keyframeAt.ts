import { clipDuration, type ClipInstance, type Keyframe } from "./types";

/** Two keyframes closer than this (as a fraction of the clip) count as the same moment. */
export const KEYFRAME_EPSILON = 0.005;

/** Where the playhead sits inside `clip`, from 0 at its first frame to 1 at its last; null when
 *  the playhead is outside the clip. */
export function playheadFraction(clip: ClipInstance, playheadSecs: number): number | null {
  const duration = clipDuration(clip);
  if (!(duration > 0)) {
    return null;
  }
  const fraction = (playheadSecs - clip.start_secs) / duration;
  return fraction >= 0 && fraction <= 1 ? fraction : null;
}

/** Value of a numeric keyframe list at `fraction`, linearly interpolated and held flat before
 *  the first and after the last keyframe; `fallback` when the list is empty. */
export function valueAt(keyframes: Keyframe<number>[], fraction: number, fallback: number): number {
  if (keyframes.length === 0) {
    return fallback;
  }
  const sorted = [...keyframes].sort((a, b) => {
    return a.time_fraction - b.time_fraction;
  });
  if (fraction <= sorted[0].time_fraction) {
    return sorted[0].value;
  }
  for (let i = 1; i < sorted.length; i += 1) {
    const next = sorted[i];
    if (fraction <= next.time_fraction) {
      const prev = sorted[i - 1];
      const span = next.time_fraction - prev.time_fraction;
      const mix = span > 0 ? (fraction - prev.time_fraction) / span : 1;
      return prev.value + (next.value - prev.value) * mix;
    }
  }
  return sorted[sorted.length - 1].value;
}

/** Whether `keyframes` already holds one at `fraction`. */
export function hasKeyframeAt<T>(keyframes: Keyframe<T>[], fraction: number): boolean {
  return keyframes.some((keyframe) => {
    return Math.abs(keyframe.time_fraction - fraction) <= KEYFRAME_EPSILON;
  });
}

/** The list without the keyframe at `fraction`, when there is one. */
export function withoutKeyframe<T>(keyframes: Keyframe<T>[], fraction: number): Keyframe<T>[] {
  return keyframes.filter((keyframe) => {
    return Math.abs(keyframe.time_fraction - fraction) > KEYFRAME_EPSILON;
  });
}

/** The list with a keyframe carrying `value` at `fraction`, in time order; unchanged when one is
 *  already there. */
export function withKeyframe<T>(
  keyframes: Keyframe<T>[],
  fraction: number,
  value: T,
): Keyframe<T>[] {
  if (hasKeyframeAt(keyframes, fraction)) {
    return keyframes;
  }
  return [...keyframes, { time_fraction: fraction, value }].sort((a, b) => {
    return a.time_fraction - b.time_fraction;
  });
}

/** Removes the keyframe at `fraction` when there is one, otherwise adds one carrying `value`:
 *  the behaviour of a diamond button. */
export function toggleKeyframe<T>(
  keyframes: Keyframe<T>[],
  fraction: number,
  value: T,
): Keyframe<T>[] {
  return hasKeyframeAt(keyframes, fraction)
    ? withoutKeyframe(keyframes, fraction)
    : withKeyframe(keyframes, fraction, value);
}

/** The list with a keyframe carrying `value` at `fraction`, replacing the one already there. */
export function setKeyframeValue<T>(
  keyframes: Keyframe<T>[],
  fraction: number,
  value: T,
): Keyframe<T>[] {
  return withKeyframe(withoutKeyframe(keyframes, fraction), fraction, value);
}
