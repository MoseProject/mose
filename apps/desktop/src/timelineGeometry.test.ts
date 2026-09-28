import { describe, expect, it } from "vitest";
import {
  RULER_HEIGHT,
  TRACK_HEIGHT,
  hitTest,
  rulerTicks,
  snap,
  snapCandidates,
  timeToX,
  trackIndexAtY,
  xToTime,
  zoomAround,
} from "./timelineGeometry";
import type { ClipInstance, Track } from "./types";

function clip(id: number, start: number, duration: number, speed = 1): ClipInstance {
  return {
    id,
    asset_id: 1,
    start_secs: start,
    source_in_secs: 0,
    source_out_secs: duration * speed,
    nested_sequence_id: null,
    color_label: null,
    gain_db: 0,
    frozen: false,
    speed_factor: speed,
    speed_ramp_end_factor: null,
    flipped_h: false,
    brightness: 0,
    contrast: 1,
    saturation: 1,
    voice_cleanup_enabled: false,
  };
}

function track(id: number, clips: ClipInstance[]): Track {
  return {
    id,
    name: `V${id}`,
    kind: "Video",
    clips,
    text_clips: [],
    shape_clips: [],
    visible: true,
    audio_role: "Unspecified",
    color_label: null,
    locked: false,
  };
}

const rowY = (index: number) => RULER_HEIGHT + index * TRACK_HEIGHT + TRACK_HEIGHT / 2;

describe("coordinate mapping", () => {
  it("round-trips time and x at any zoom and scroll", () => {
    const x = timeToX(12.5, 40, 3);
    expect(x).toBe(380);
    expect(xToTime(x, 40, 3)).toBe(12.5);
  });

  it("maps y to track rows below the ruler only", () => {
    expect(trackIndexAtY(RULER_HEIGHT - 1, 3)).toBe(-1);
    expect(trackIndexAtY(rowY(0), 3)).toBe(0);
    expect(trackIndexAtY(rowY(2), 3)).toBe(2);
    expect(trackIndexAtY(rowY(3), 3)).toBe(-1);
  });
});

describe("hitTest", () => {
  const tracks = [track(1, [clip(10, 1, 2)]), track(2, [clip(20, 0, 4, 2)])];

  it("distinguishes trim handles from the body", () => {
    expect(hitTest(tracks, 41, rowY(0), 40, 0)).toEqual({ part: "start", clipId: 10, trackIndex: 0 });
    expect(hitTest(tracks, 80, rowY(0), 40, 0)?.part).toBe("body");
    expect(hitTest(tracks, 119, rowY(0), 40, 0)?.part).toBe("end");
  });

  it("uses speed-adjusted duration", () => {
    expect(hitTest(tracks, 150, rowY(1), 40, 0)?.clipId).toBe(20);
    expect(hitTest(tracks, 170, rowY(1), 40, 0)).toBeNull();
  });

  it("keeps a body zone on very narrow clips", () => {
    const narrow = [track(1, [clip(1, 0, 0.3)])];
    expect(hitTest(narrow, 6, rowY(0), 40, 0)?.part).toBe("body");
  });
});

describe("snapping", () => {
  it("snaps to the nearest candidate within the threshold", () => {
    expect(snap(4.95, [0, 5, 10], 0.2)).toBe(5);
    expect(snap(4.5, [0, 5, 10], 0.2)).toBe(4.5);
  });

  it("excludes the dragged clip's own edges", () => {
    const tracks = [track(1, [clip(1, 0, 2), clip(2, 5, 1)])];
    expect(snapCandidates(tracks, 2, [3.3])).toEqual([0, 3.3, 0, 2]);
  });
});

describe("ruler and zoom", () => {
  it("spaces ticks at least 80px apart", () => {
    const ticks = rulerTicks(40, 0, 400);
    expect(ticks[1] - ticks[0]).toBe(2);
    expect(ticks[0]).toBe(0);
  });

  it("keeps the time under the anchor fixed while zooming", () => {
    const before = xToTime(200, 40, 10);
    const zoomed = zoomAround(200, 40, 10, 2, 2, 600);
    expect(zoomed.pxPerSec).toBe(80);
    expect(xToTime(200, zoomed.pxPerSec, zoomed.scrollSecs)).toBeCloseTo(before);
  });

  it("clamps zoom to its bounds", () => {
    expect(zoomAround(0, 500, 0, 4, 2, 600).pxPerSec).toBe(600);
  });
});
