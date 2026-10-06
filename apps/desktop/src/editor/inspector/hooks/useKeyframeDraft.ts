import { useDraft } from "../../../hooks/useDraft";
import type { Keyframe } from "../../../types";

/** Local keyframe edits are committed on add/remove and after value gestures. */
export function useKeyframeDraft<T>(
  keyframes: Keyframe<T>[],
  onCommit: (next: Keyframe<T>[]) => void,
) {
  const [draft, setDraft] = useDraft(keyframes);
  const commitNow = (next: Keyframe<T>[]) => {
    setDraft(next);
    onCommit(next);
  };
  const removeAt = (index: number) => {
    return commitNow(
      draft.filter((_, i) => {
        return i !== index;
      }),
    );
  };
  return { draft, setDraft, commitNow, removeAt };
}
