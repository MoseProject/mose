// Per-asset media the timeline paints inside clips: video frame strips and audio peaks. Both come
// from the backend's `oca://` scheme on demand, are cached here, and ask the timeline to repaint
// when something arrives, so `draw()` itself stays synchronous.

import { coarseWaveformUrl, frameAtUrl, waveformUrl } from "../../ipc";
import type { MediaAsset } from "../../types";

export const MEDIA_EVENT = "oca-timeline-media";
const notify = () => {
  return window.dispatchEvent(new Event(MEDIA_EVENT));
};

// ---- video frames --------------------------------------------------------------------------

const FRAME_STEPS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 30, 60, 120, 300, 600, 1800, 3600];
const MAX_IN_FLIGHT = 4;
const MAX_QUEUED = 48;
const MAX_CACHED_FRAMES = 600;
const MAX_FRAME_BYTES = 32 * 1024 * 1024;

/** A load that failed, with when, so it can be asked for again later: the file may have been
 *  offline or still arriving, and the backend retries after a while too. */
interface Failed {
  failedAt: number;
}
const RETRY_MS = 10_000;
const isFailed = (entry: unknown): entry is Failed => {
  return typeof entry === "object" && entry !== null && "failedAt" in entry;
};

type FrameEntry = HTMLImageElement | "pending" | Failed;
const frames = new Map<string, FrameEntry>();
const queue: { key: string; assetId: number; millis: number; epoch: number }[] = [];
let inFlight = 0;

// Asset ids restart from 1 in every project, so what is cached under an id is only valid while
// the id still means the same file. `epoch` bumps when that stops being true, and answers that
// were requested before the bump are dropped when they arrive.
const sources = new Map<number, string>();
let epoch = 0;
let projectNamespace: unknown;
export function syncProject(namespace: unknown) {
  if (projectNamespace !== namespace) {
    projectNamespace = namespace;
    sources.clear();
    epoch += 1;
    frames.clear();
    queue.length = 0;
    detailed.clear();
    overview.clear();
  }
}

/** Tells the cache which file each asset id stands for now; drops everything cached for ids
 *  that point to a different file than before (another project, a re-linked file). */
export function syncAssets(assets: MediaAsset[]) {
  const next = new Map(
    assets.map((asset) => {
      return [asset.id, asset.source_path];
    }),
  );
  const stale = sources.size !== next.size || [...sources].some(([id, path]) => {
    return next.get(id) !== path;
  });
  sources.clear();
  next.forEach((path, id) => {
    return sources.set(id, path);
  });
  if (stale) {
    epoch += 1;
    frames.clear();
    queue.length = 0;
    detailed.clear();
    overview.clear();
    notify();
  }
}

/** The coarsest frame spacing that still gives every tile of `secsPerTile` its own frame. The
 *  spacing is quantized so the same frames are reused as the user zooms and scrolls. */
export function frameStepSecs(secsPerTile: number): number {
  let step = FRAME_STEPS[0];
  for (const candidate of FRAME_STEPS) {
    if (candidate <= secsPerTile) {
      step = candidate;
    }
  }
  return step;
}

function trimFrames() {
  let bytes = 0;
  for (const entry of frames.values()) {
    if (entry instanceof HTMLImageElement) {
      bytes += entry.naturalWidth * entry.naturalHeight * 4;
    }
  }
  for (const [key, entry] of frames) {
    if (bytes <= MAX_FRAME_BYTES && frames.size <= MAX_CACHED_FRAMES) {
      break;
    }
    if (entry === "pending") {
      continue;
    }
    if (entry instanceof HTMLImageElement) {
      bytes -= entry.naturalWidth * entry.naturalHeight * 4;
    }
    frames.delete(key);
  }
}

function pump() {
  while (inFlight < MAX_IN_FLIGHT && queue.length > 0) {
    const job = queue.shift()!;
    inFlight += 1;
    const image = new Image();
    const settle = (entry: FrameEntry) => {
      if (job.epoch === epoch) {
        frames.set(job.key, entry);
        trimFrames();
      }
      inFlight -= 1;
      notify();
      pump();
    };
    image.onload = () => {
      return settle(image);
    };
    image.onerror = () => {
      return settle({ failedAt: Date.now() });
    };
    image.src = frameAtUrl(job.assetId, job.millis);
  }
}

/** The decoded frame of `assetId` at `secs`, or `null` while it loads (or if there is none). */
export function frameAt(assetId: number, secs: number, stepSecs: number): HTMLImageElement | null {
  const millis = Math.round(Math.floor(Math.max(0, secs) / stepSecs) * stepSecs * 1000);
  const key = `${assetId}:${millis}`;
  let entry = frames.get(key);
  if (entry instanceof HTMLImageElement) {
    frames.delete(key);
    frames.set(key, entry);
    return entry;
  }
  if (isFailed(entry) && Date.now() - entry.failedAt >= RETRY_MS) {
    frames.delete(key);
    entry = undefined;
  }
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
    queue.push({ key, assetId, millis, epoch });
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
  if (buckets === 0 || durationSecs <= 0) {
    return null;
  }
  const levels = [pairs];
  while (levels[levels.length - 1].length >> 1 > LEVEL_FACTOR) {
    const below = levels[levels.length - 1];
    const count = Math.ceil((below.length >> 1) / LEVEL_FACTOR);
    const next = new Float32Array(count * 2);
    for (let i = 0; i < count; i += 1) {
      let min = Infinity;
      let max = -Infinity;
      const end = Math.min(below.length >> 1, (i + 1) * LEVEL_FACTOR);
      for (let j = i * LEVEL_FACTOR; j < end; j += 1) {
        if (below[2 * j] < min) {
          min = below[2 * j];
        }
        if (below[2 * j + 1] > max) {
          max = below[2 * j + 1];
        }
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
export function peakRange(
  pyramid: PeakPyramid,
  from: number,
  to: number,
  out: [number, number],
): boolean {
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
  if (first >= count) {
    return false;
  }
  let min = Infinity;
  let max = -Infinity;
  for (let i = first; i <= last; i += 1) {
    if (data[2 * i] < min) {
      min = data[2 * i];
    }
    if (data[2 * i + 1] > max) {
      max = data[2 * i + 1];
    }
  }
  out[0] = min;
  out[1] = max;
  return true;
}

type WaveEntry = PeakPyramid | "pending" | Failed;
const detailed = new Map<number, WaveEntry>();
/** The coarse overviews: one small fetch per asset that has one, answered from memory. */
const overview = new Map<number, WaveEntry>();

/** Fetches little-endian f32 (min, max) pairs from `url` into `cache` under `assetId`. */
function fetchPeaks(
  cache: Map<number, WaveEntry>,
  assetId: number,
  url: string,
  durationSecs: number,
) {
  if (waveInFlight >= 2) {
    return;
  }
  waveInFlight += 1;
  const requestedIn = epoch;
  cache.set(assetId, "pending");
  void fetch(url)
    .then((response) => {
      return response.ok
        ? response.arrayBuffer()
        : Promise.reject(new Error(String(response.status)));
    })
    .then((buffer) => {
      if (requestedIn !== epoch) {
        return;
      }
      const pyramid = buildPyramid(new Float32Array(buffer), durationSecs);
      cache.set(assetId, pyramid ?? { failedAt: Date.now() });
      trimPeaks();
      notify();
    })
    .catch(() => {
      if (requestedIn === epoch) {
        cache.set(assetId, { failedAt: Date.now() });
      }
    }).finally(() => {
      waveInFlight -= 1;
      notify();
    });
}

let waveInFlight = 0;
const WAVE_BUDGET = 64 * 1024 * 1024;
function trimPeaks() {
  const cost = (value: WaveEntry) => {
    return typeof value === "object" && "levels" in value
    ? value.levels.reduce((bytes, level) => {
      return bytes + level.byteLength;
    }, 0) : 0;
  };
  let bytes = [...detailed.values(), ...overview.values()].reduce((sum, entry) => {
    return sum + cost(entry);
  }, 0);
  for (const cache of [detailed, overview]) {
    for (const [id, entry] of cache) {
      if (bytes <= WAVE_BUDGET && cache.size <= 1500) {
        break;
      }
      if (entry !== "pending") {
        bytes -= cost(entry);
        cache.delete(id);
      }
    }
  }
}

/** The entry of `cache` for `assetId`, forgetting a failure once it is old enough to retry. */
function entryFor(cache: Map<number, WaveEntry>, assetId: number): WaveEntry | undefined {
  const entry = cache.get(assetId);
  if (entry && entry !== "pending") {
    cache.delete(assetId);
    cache.set(assetId, entry);
  }
  if (isFailed(entry) && Date.now() - entry.failedAt >= RETRY_MS) {
    cache.delete(assetId);
    return undefined;
  }
  return entry;
}

const isPyramid = (entry: WaveEntry | undefined): entry is PeakPyramid => {
  return typeof entry === "object" && "levels" in entry;
};

/** The best peaks available for `asset` right now: the detailed waveform once it has been
 *  fetched (a decode pass over the whole file, so it can take a while on long recordings),
 *  the coarse one stored with the project until then. */
export function peaksFor(asset: MediaAsset): PeakPyramid | null {
  if (typeof fetch !== "function") {
    return null;
  }
  const fine = entryFor(detailed, asset.id);
  if (isPyramid(fine)) {
    return fine;
  }
  if (asset.has_audio && fine === undefined) {
    fetchPeaks(detailed, asset.id, waveformUrl(asset.id), asset.duration_secs);
  }
  if (!asset.has_waveform) {
    return null;
  }
  const coarse = entryFor(overview, asset.id);
  if (coarse === undefined) {
    fetchPeaks(overview, asset.id, coarseWaveformUrl(asset.id), asset.duration_secs);
    return null;
  }
  return isPyramid(coarse) ? coarse : null;
}
