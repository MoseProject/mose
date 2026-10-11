import { hasKeyframeAt, setKeyframeValue, toggleKeyframe, valueAt } from "../../keyframeAt";
import type { Keyframe } from "../../types";
import type { SectionKeyframe } from "./controls";

/** What one property row needs to be animated from the inspector: the diamond at its end, the
 *  value to show at the playhead, and where an edit goes. */
export interface AnimatedRow {
  diamond: SectionKeyframe;
  /** The static value, or the interpolated one at the playhead once the property has keyframes. */
  shown: number;
  /** Commits an edit: a keyframe at the playhead while the property is animated, otherwise the
   *  static value. */
  edit: (value: number) => Promise<unknown> | void;
}

export function animatedRow(
  list: Keyframe<number>[],
  fraction: number | null,
  staticValue: number,
  commitList: (next: Keyframe<number>[]) => Promise<unknown> | void,
  commitStatic: (value: number) => Promise<unknown> | void,
): AnimatedRow {
  const animated = list.length > 0 && fraction !== null;
  return {
    diamond: {
      active: fraction !== null && hasKeyframeAt(list, fraction),
      disabled: fraction === null,
      onToggle: () => {
        if (fraction !== null) {
          void commitList(toggleKeyframe(list, fraction, valueAt(list, fraction, staticValue)));
        }
      },
    },
    shown: animated ? valueAt(list, fraction, staticValue) : staticValue,
    edit: (value) => {
      return animated ? commitList(setKeyframeValue(list, fraction, value)) : commitStatic(value);
    },
  };
}
