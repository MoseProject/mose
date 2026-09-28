// Pure timeline math: coordinate mapping, hit testing, snapping and ruler ticks. Kept free of
// React and canvas so it is unit-testable, mirroring how the egui timeline keeps its geometry
// in plain functions (`timeline_panel/draw.rs` takes `(rect, data, px_per_sec)`).

import { clipDuration, type Marker, type Track } from "./types";

export const RULER_HEIGHT = 24;
export const TRACK_HEIGHT = 56;
export const HEADER_WIDTH = 132;
/** Grab width of a clip's trim handles, in px. */
export const EDGE_GRAB_PX = 6;
/** Snap distance, in px (converted to seconds at the current zoom). */
export const SNAP_PX = 8;

export function timeToX(secs: number, pxPerSec: number, scrollSecs: number): number {
  return (secs - scrollSecs) * pxPerSec;
}

export function xToTime(x: number, pxPerSec: number, scrollSecs: number): number {
  return x / pxPerSec + scrollSecs;
}

/** Marker nearest a ruler coordinate within its visible tick width. */
export function markerAtX(markers: Marker[], x: number, pxPerSec: number, scrollSecs: number, radius = 6): Marker | undefined {
  return markers.find((marker) => Math.abs(timeToX(marker.position_secs, pxPerSec, scrollSecs) - x) <= radius);
}

/** Track row under a y coordinate in the track area (below the ruler), or -1. */
export function trackIndexAtY(y: number, trackCount: number): number {
  const index = Math.floor((y - RULER_HEIGHT) / TRACK_HEIGHT);
  return y >= RULER_HEIGHT && index < trackCount ? index : -1;
}

export type ClipHit = {
  part: "body" | "start" | "end";
  clipId: number;
  trackIndex: number;
};

/** Timeline geometry of any clip kind: media clips, text overlays and shape overlays. */
export interface TimelineSpan {
  id: number;
  start_secs: number;
  duration_secs: number;
  /** Text/shape clips move and trim through the overlay commands, media clips through the
   *  clip commands. */
  overlay: boolean;
}

/** Every clip on `track` as a span. A track only ever holds one kind, so order is stable. */
export function trackSpans(track: Track): TimelineSpan[] {
  const spans: TimelineSpan[] = track.clips.map((clip) => ({
    id: clip.id,
    start_secs: clip.start_secs,
    duration_secs: clipDuration(clip),
    overlay: false,
  }));
  for (const clip of [...track.text_clips, ...track.shape_clips]) {
    spans.push({ id: clip.id, start_secs: clip.start_secs, duration_secs: clip.duration_secs, overlay: true });
  }
  return spans;
}

/** The span with `clipId` and the index of its track, across every track. */
export function findSpan(tracks: Track[], clipId: number): { span: TimelineSpan; trackIndex: number } | null {
  for (let trackIndex = 0; trackIndex < tracks.length; trackIndex += 1) {
    const span = trackSpans(tracks[trackIndex]).find((s) => s.id === clipId);
    if (span) return { span, trackIndex };
  }
  return null;
}

/** Which clip part sits under (x, y) in lane coordinates (x = 0 at the lane's left edge). */
export function hitTest(
  tracks: Track[],
  x: number,
  y: number,
  pxPerSec: number,
  scrollSecs: number,
): ClipHit | null {
  const trackIndex = trackIndexAtY(y, tracks.length);
  if (trackIndex < 0) return null;
  for (const clip of trackSpans(tracks[trackIndex])) {
    const left = timeToX(clip.start_secs, pxPerSec, scrollSecs);
    const right = timeToX(clip.start_secs + clip.duration_secs, pxPerSec, scrollSecs);
    if (x < left || x > right) continue;
    // Narrow clips keep a body zone: handles never cover more than a third each.
    const grab = Math.min(EDGE_GRAB_PX, (right - left) / 3);
    const part = x - left <= grab ? "start" : right - x <= grab ? "end" : "body";
    return { part, clipId: clip.id, trackIndex };
  }
  return null;
}

/** Every clip edge on the timeline plus `extra` points (playhead), minus `excludeClipId`. */
export function snapCandidates(tracks: Track[], excludeClipId: number, extra: number[]): number[] {
  const points = [0, ...extra];
  for (const track of tracks) {
    for (const clip of trackSpans(track)) {
      if (clip.id === excludeClipId) continue;
      points.push(clip.start_secs, clip.start_secs + clip.duration_secs);
    }
  }
  return points;
}

/** The candidate nearest to `secs` within `thresholdSecs`, else `secs` unchanged. */
export function snap(secs: number, candidates: number[], thresholdSecs: number): number {
  let best = secs;
  let bestDistance = thresholdSecs;
  for (const candidate of candidates) {
    const distance = Math.abs(candidate - secs);
    if (distance <= bestDistance) {
      best = candidate;
      bestDistance = distance;
    }
  }
  return best;
}

const TICK_STEPS = [0.1, 0.25, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300, 600];
const MIN_TICK_SPACING_PX = 80;

/** Labelled ruler tick times covering `[scrollSecs, scrollSecs + width / pxPerSec]`. */
export function rulerTicks(pxPerSec: number, scrollSecs: number, width: number): number[] {
  const step =
    TICK_STEPS.find((s) => s * pxPerSec >= MIN_TICK_SPACING_PX) ??
    TICK_STEPS[TICK_STEPS.length - 1];
  const first = Math.ceil(scrollSecs / step);
  const end = scrollSecs + width / pxPerSec;
  const ticks: number[] = [];
  for (let i = first; i * step <= end; i++) {
    ticks.push(Number((i * step).toFixed(3)));
  }
  return ticks;
}

/** Zooms around an anchor so the time under the pointer stays put. */
export function zoomAround(
  anchorX: number,
  pxPerSec: number,
  scrollSecs: number,
  factor: number,
  minPxPerSec: number,
  maxPxPerSec: number,
): { pxPerSec: number; scrollSecs: number } {
  const anchorSecs = xToTime(anchorX, pxPerSec, scrollSecs);
  const next = Math.min(maxPxPerSec, Math.max(minPxPerSec, pxPerSec * factor));
  return { pxPerSec: next, scrollSecs: Math.max(0, anchorSecs - anchorX / next) };
}
