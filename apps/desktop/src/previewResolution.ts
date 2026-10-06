/** Smallest and largest longest side, in pixels, the backend composites the preview at. */
export const PREVIEW_MIN_DIM = 480;
export const PREVIEW_MAX_DIM = 1280;
/** Requested sizes snap up to a multiple of this, so resizing the window does not rebuild the
 *  preview scene at every pixel. */
const STEP = 160;

/**
 * The longest side the preview needs: what its canvas covers on screen, in device pixels,
 * rounded up to a step and kept inside the backend's limits.
 */
export function previewMaxDim(
  displayWidth: number,
  displayHeight: number,
  pixelRatio: number,
): number {
  const longest = Math.max(displayWidth, displayHeight) * Math.max(pixelRatio, 1);
  if (!Number.isFinite(longest) || longest <= 0) {
    return PREVIEW_MAX_DIM;
  }
  const stepped = Math.ceil(longest / STEP) * STEP;
  return Math.min(PREVIEW_MAX_DIM, Math.max(PREVIEW_MIN_DIM, stepped));
}
