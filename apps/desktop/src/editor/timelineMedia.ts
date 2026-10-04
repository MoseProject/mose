// Per-asset media the timeline paints inside clips: video frame strips and audio peaks. Both come
// from the backend's `oca://` scheme on demand, are cached here, and ask the timeline to repaint
// when something arrives, so `draw()` itself stays synchronous.

import { frameAtUrl, waveformUrl } from "../ipc";
import type { MediaAsset } from "../types";

export const MEDIA_EVENT = "oca-timeline-media";
const notify = () => window.dispatchEvent(new Event(MEDIA_EVENT));

// ---- video frames --------------------------------------------------------------------------

const FRAME_STEPS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600];
const MAX_IN_FLIGHT = 4;
const MAX_QUEUED = 48;
const MAX_CACHED_FRAMES = 600;

type FrameEntry = HTMLImageElement | "pending" | "failed";
const frames = new Map<string, FrameEntry>();
const queue: { key: string; assetId: number; millis: number }[] = [];
let inFlight = 0;

/** The coarsest frame spacing that still gives every tile of `secsPerTile` its own frame. The
 *  spacing is quantized so the same frames are reused as the user zooms and scrolls. */
export function frameStepSecs(secsPerTile: number): number {
  let step = FRAME_STEPS[0];
  for (const candidate of FRAME_STEPS) if (candidate <= secsPerTile) step = candidate;
  return step;
}

function pump() {
  while (inFlight < MAX_IN_FLIGHT && queue.length > 0) {
    const job = queue.shift()!;
    inFlight += 1;
    const image = new Image();
    const settle = (entry: FrameEntry) => {
      frames.set(job.key, entry);
      inFlight -= 1;
      notify();
      pump();
    };
    image.onload = () => settle(image);
    image.onerror = () => settle("failed");
    image.src = frameAtUrl(job.assetId, job.millis);
  }
}

/** The decoded frame of `assetId` at `secs`, or `null` while it loads (or if there is none). */
export function frameAt(assetId: number, secs: number, stepSecs: number): HTMLImageElement | null {
  const millis = Math.round((Math.floor(Math.max(0, secs) / stepSecs) * stepSecs) * 1000);
  const key = `${assetId}:${millis}`;
  const entry = frames.get(key);
  if (entry instanceof HTMLImageElement) return entry;
  if (entry === undefined) {
    if (frames.size >= MAX_CACHED_FRAMES) {
      // Oldest first; `Map` iterates in insertion order. Never evict an in-flight entry's slot.
      for (const stale of frames.keys()) {
        if (frames.get(stale) !== "pending") {
          frames.delete(stale);
          break;
        }
      }
    }
    frames.set(key, "pending");
    queue.push({ key, assetId, millis });
    if (queue.length > MAX_QUEUED) {
      const dropped = queue.shift()!;
      frames.delete(dropped.key);
    }
    pump();
  }
  return null;
}

// ---- audio peaks ---------------------------------------------------------------------------

const LEVEL_FACTOR = 8;

/** Min/max peaks at several resolutions: level 0 is the source, each next level folds
 *  `LEVEL_FACTOR` buckets into one, so a zoomed-out query scans a handful of buckets. */
export interface PeakPyramid {
  levels: Float32Array[]; // interleaved min,max pairs
  bucketsPerSec: number;
}

export function buildPyramid(pairs: Float32Array, durationSecs: number): PeakPyramid | null {
  const buckets = pairs.length >> 1;
  if (buckets === 0 || durationSecs <= 0) return null;
  const levels = [pairs];
  while ((levels[levels.length - 1].length >> 1) > LEVEL_FACTOR) {
    const below = levels[levels.length - 1];
    const count = Math.ceil((below.length >> 1) / LEVEL_FACTOR);
    const next = new Float32Array(count * 2);
    for (let i = 0; i < count; i += 1) {
      let min = Infinity;
      let max = -Infinity;
      const end = Math.min(below.length >> 1, (i + 1) * LEVEL_FACTOR);
      for (let j = i * LEVEL_FACTOR; j < end; j += 1) {
        if (below[2 * j] < min) min = below[2 * j];
        if (below[2 * j + 1] > max) max = below[2 * j + 1];
      }
      next[2 * i] = min;
      next[2 * i + 1] = max;
    }
    levels.push(next);
  }
  return { levels, bucketsPerSec: buckets / durationSecs };
}

/** Min and max amplitude over source time `[from, to)`, written into `out` as `[min, max]`.
 *  Returns false when the range holds no data. */
export function peakRange(pyramid: PeakPyramid, from: number, to: number, out: [number, number]): boolean {
  let level = 0;
  let perSec = pyramid.bucketsPerSec;
  // Coarsest level that still has at least ~1 bucket across the range.
  while (level + 1 < pyramid.levels.length && (to - from) * (perSec / LEVEL_FACTOR) >= 1) {
    level += 1;
    perSec /= LEVEL_FACTOR;
  }
  const data = pyramid.levels[level];
  const count = data.length >> 1;
  const first = Math.max(0, Math.floor(from * perSec));
  const last = Math.min(count - 1, Math.max(first, Math.ceil(to * perSec) - 1));
  if (first >= count) return false;
  let min = Infinity;
  let max = -Infinity;
  for (let i = first; i <= last; i += 1) {
    if (data[2 * i] < min) min = data[2 * i];
    if (data[2 * i + 1] > max) max = data[2 * i + 1];
  }
  out[0] = min;
  out[1] = max;
  return true;
}

type WaveEntry = PeakPyramid | "pending" | "failed";
const detailed = new Map<number, WaveEntry>();
const overview = new WeakMap<object, PeakPyramid | null>();

/** The best peaks available for `asset` right now: the detailed waveform once it has been
 *  fetched (a decode pass over the whole file, so it can take a while on long recordings),
 *  the coarse one stored with the project until then. */
export function peaksFor(asset: MediaAsset): PeakPyramid | null {
  const entry = detailed.get(asset.id);
  if (entry && typeof entry === "object") return entry;
  if (asset.has_audio && entry === undefined && typeof fetch === "function") {
    detailed.set(asset.id, "pending");
    void fetch(waveformUrl(asset.id))
      .then((response) => (response.ok ? response.arrayBuffer() : Promise.reject(new Error(String(response.status)))))
      .then((buffer) => {
        const pyramid = buildPyramid(new Float32Array(buffer), asset.duration_secs);
        detailed.set(asset.id, pyramid ?? "failed");
        notify();
      })
      .catch(() => detailed.set(asset.id, "failed"));
  }
  const coarse = asset.waveform_peaks;
  if (!coarse || coarse.length === 0) return null;
  if (!overview.has(coarse)) {
    const flat = new Float32Array(coarse.length * 2);
    coarse.forEach(([min, max], i) => {
      flat[2 * i] = min;
      flat[2 * i + 1] = max;
    });
    overview.set(coarse, buildPyramid(flat, asset.duration_secs));
  }
  return overview.get(coarse) ?? null;
}
