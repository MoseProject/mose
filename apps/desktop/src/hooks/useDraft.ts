import { useState } from "react";

/** Local editable copy of a value owned elsewhere (the backend session). The draft follows
 *  `source` whenever it changes, without an effect: React re-renders at once when state is set
 *  during render, so the stale draft is never painted. */
export function useDraft<T>(source: T): [T, (draft: T) => void] {
  const [state, setState] = useState({ source, draft: source });
  let current = state;
  if (!Object.is(state.source, source)) {
    current = { source, draft: source };
    setState(current);
  }
  const setDraft = (draft: T) => {
    setState({ source: current.source, draft });
  };
  return [current.draft, setDraft];
}
