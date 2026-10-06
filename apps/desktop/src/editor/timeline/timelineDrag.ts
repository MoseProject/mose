import { snap } from "../../timelineGeometry";

export type Drag =
  | { mode: "scrub" }
  | {
      mode: "move";
      clipId: number;
      grabOffsetSecs: number;
      ghostStart: number;
      ghostTrack: number;
      duration: number;
      /** "slide" keeps the clip's own range and absorbs the move into its neighbors instead of
       *  repositioning; the row never changes for it. */
      editKind: "move" | "slide";
    }
  | {
      mode: "trim-start" | "trim-end";
      clipId: number;
      ghostEdge: number;
      /** Rolls the shared boundary with the neighbor on that side instead of trimming alone. */
      roll: boolean;
    }
  | {
      mode: "slip";
      clipId: number;
      grabX: number;
      deltaSecs: number;
    };

/** Snaps a moved clip by whichever of its edges lands on a candidate with less adjustment. */
export function snapMove(raw: number, duration: number, candidates: number[], threshold: number) {
  const byStart = snap(raw, candidates, threshold);
  const byEnd = snap(raw + duration, candidates, threshold) - duration;
  const startSnapped = byStart !== raw;
  const endSnapped = byEnd !== raw;
  if (startSnapped && (!endSnapped || Math.abs(byStart - raw) <= Math.abs(byEnd - raw))) {
    return byStart;
  }
  return endSnapped ? Math.max(0, byEnd) : raw;
}
