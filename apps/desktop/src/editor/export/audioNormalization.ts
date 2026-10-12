import type { SessionView } from "../../types";

/** How far from the target, in LU, a clip's loudness may be and still count as normalized: the
 *  clip gain moves in 0.5 dB steps, so a clip normalized by hand lands within half a step. */
export const NORMALIZED_TOLERANCE_LU = 1;

/** Whether the sound of the active sequence (or of just `clipIds`) is already at `targetLufs`
 *  because each clip's gain was set for it: the clip's measured loudness plus its gain, within
 *  [`NORMALIZED_TOLERANCE_LU`] of the target. `null` when that cannot be told: a clip whose
 *  loudness was not measured, a compound clip, animated gain, or no sound at all. */
export function alreadyNormalized(
  session: SessionView,
  targetLufs: number,
  clipIds: number[] | null = null,
): boolean | null {
  if (!Number.isFinite(targetLufs)) {
    return null;
  }
  const sequence = session.project.sequences[session.project.active_sequence];
  let counted = 0;
  for (const track of sequence.timeline.tracks) {
    if ((track.kind !== "Video" && track.kind !== "Audio") || track.muted) {
      continue;
    }
    for (const clip of track.clips) {
      if (clipIds !== null && !clipIds.includes(clip.id)) {
        continue;
      }
      if (clip.nested_sequence_id !== null) {
        return null;
      }
      const asset = session.project.media_library.find((item) => {
        return item.id === clip.asset_id;
      });
      if (!asset?.has_audio) {
        continue;
      }
      if (!asset.loudness || clip.gain_keyframes.length > 0) {
        return null;
      }
      counted += 1;
      if (
        Math.abs(asset.loudness.integrated_lufs + clip.gain_db - targetLufs) >
        NORMALIZED_TOLERANCE_LU
      ) {
        return false;
      }
    }
  }
  return counted > 0 ? true : null;
}
