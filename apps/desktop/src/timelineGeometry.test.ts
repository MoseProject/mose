import { describe, expect, it } from "vitest";
import {
  RULER_HEIGHT,
  TRACK_HEIGHT,
  findSpan,
  hitTest,
  markerAtX,
  rulerTicks,
  snap,
  snapCandidates,
  timeToX,
  trackIndexAtY,
  xToTime,
  zoomAround,
} from "./timelineGeometry";
import type { ClipInstance, Marker, ShapeClip, TextClip, Track } from "./types";

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

function textClip(id: number, start: number, duration: number): TextClip {
  return { id, start_secs: start, duration_secs: duration, text: "Text", font_size: 48, font_family: "lato", font_style: "Regular", color_rgba: [255, 255, 255, 255], pos_x: 0.1, pos_y: 0.85, words: [], highlight_enabled: false, highlight_color_rgba: [255, 220, 0, 255], text_align: "Auto" };
}

function shapeClip(id: number, start: number, duration: number): ShapeClip {
  return { id, start_secs: start, duration_secs: duration, shape_kind: "Ellipse", center_x: 0.5, center_y: 0.5, width: 0.3, height: 0.3, color_rgba: [255, 255, 255, 255] };
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

  it("finds text and shape clip handles using their overlay duration", () => {
    const textTrack = track(1, []);
    textTrack.kind = "Text";
    textTrack.text_clips = [textClip(31, 1, 2)];
    const shapeTrack = track(2, []);
    shapeTrack.kind = "Shape";
    shapeTrack.shape_clips = [shapeClip(32, 2, 1)];
    expect(hitTest([textTrack, shapeTrack], 41, rowY(0), 40, 0)).toEqual({ part: "start", clipId: 31, trackIndex: 0 });
    expect(hitTest([textTrack, shapeTrack], 119, rowY(0), 40, 0)?.part).toBe("end");
    expect(hitTest([textTrack, shapeTrack], 100, rowY(1), 40, 0)?.clipId).toBe(32);
  });
});

describe("clip spans", () => {
  it("flags text and shape clips as overlays and finds them across tracks", () => {
    const media = track(1, [clip(1, 0, 2, 2)]);
    const textTrack = track(2, []);
    textTrack.kind = "Text";
    textTrack.text_clips = [textClip(5, 3, 1.5)];
    const found = findSpan([media, textTrack], 5);
    expect(found).toEqual({ span: { id: 5, start_secs: 3, duration_secs: 1.5, overlay: true }, trackIndex: 1 });
    expect(findSpan([media, textTrack], 1)?.span).toEqual({ id: 1, start_secs: 0, duration_secs: 2, overlay: false });
    expect(findSpan([media, textTrack], 99)).toBeNull();
  });
});

describe("marker hit testing", () => {
  const markers: Marker[] = [
    { id: 1, position_secs: 2, label: "A", kind: "Standard", completed: false },
  ];

  it("finds ruler markers within their click target", () => {
    expect(markerAtX(markers, 83, 40, 0)?.id).toBe(1);
    expect(markerAtX(markers, 90, 40, 0)).toBeUndefined();
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

  it("includes text and shape edges as snap targets", () => {
    const textTrack = track(1, []);
    textTrack.kind = "Text";
    textTrack.text_clips = [textClip(3, 4, 2)];
    const shapeTrack = track(2, []);
    shapeTrack.kind = "Shape";
    shapeTrack.shape_clips = [shapeClip(4, 8, 1)];
    expect(snapCandidates([textTrack, shapeTrack], 99, [])).toEqual([0, 4, 6, 8, 9]);
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
