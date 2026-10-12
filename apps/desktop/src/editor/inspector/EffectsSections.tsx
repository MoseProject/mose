import type { ClipInstance, ClipPatch } from "../../types";
import { AiActionsSection } from "./AiActionsSection";
import { KeyframesSection } from "./KeyframesSection";
import { MaskSection } from "./MaskSection";
import { TransitionSection } from "./TransitionSection";

/** The inspector sections that only the Effects tab shows, together so that they are one chunk,
 *  fetched when that tab is first on show. */
export function EffectsSections({
  clip,
  commit,
}: {
  clip: ClipInstance;
  commit: (patch: ClipPatch) => Promise<unknown>;
}) {
  return (
    <>
      <MaskSection
        clip={clip}
        commit={commit}
      />
      <TransitionSection
        clip={clip}
        commit={commit}
      />
      <KeyframesSection
        clip={clip}
        commit={commit}
      />
      <AiActionsSection clip={clip} />
    </>
  );
}
