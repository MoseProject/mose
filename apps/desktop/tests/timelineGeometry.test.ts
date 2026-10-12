import { describe, expect, it } from "vitest";
import {
  RULER_HEIGHT,
  TRACK_HEIGHT,
  dissolveSpanSecs,
  effectTarget,
  findSpan,
  fitPxPerSec,
  hitTest,
  markerAtX,
  rulerLabel,
  rulerTicks,
  snap,
  snapCandidates,
  timeToX,
  trackIndexAtY,
  xToTime,
  zoomAround,
} from "../src/timelineGeometry";
import { NEUTRAL_GRADE, NEUTRAL_TRACK_MIX } from "../src/types";
import type { ClipInstance, Marker, ShapeClip, TextClip, Track } from "../src/types";

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
    crop_x: 0,
    crop_y: 0,
    crop_w: 1,
    crop_h: 1,
    mask_shape: "None",
    mask_corner_radius: 0,
    transition_in: "None",
    transition_duration_secs: 0.5,
    fade_in_secs: 0,
    fade_out_secs: 0,
    blend_mode: "Normal",
    position_keyframes: [],
    layer_scale_x: 1,
    layer_scale_y: 1,
    scale_keyframes: [],
    rotation_keyframes: [],
    opacity_keyframes: [],
    gain_keyframes: [],
    brightness_keyframes: [],
    contrast_keyframes: [],
    saturation_keyframes: [],
    crop_x_keyframes: [],
    crop_y_keyframes: [],
    crop_w_keyframes: [],
    crop_h_keyframes: [],
    grade: NEUTRAL_GRADE,
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
    muted: false,
    solo: false,
    color_label: null,
    locked: false,
    mix: NEUTRAL_TRACK_MIX,
  };
}

function textClip(id: number, start: number, duration: number): TextClip {
  return {
    id,
    start_secs: start,
    duration_secs: duration,
    text: "Text",
    font_size: 48,
    font_family: "lato",
    font_style: "Regular",
    font_weight: null,
    color_rgba: [255, 255, 255, 255],
    background_rgba: [0, 0, 0, 0],
    background_padding: 0,
    background_corner_radius: 0,
    direction: "Auto",
    pos_x: 0.1,
    pos_y: 0.85,
    pos_x_keyframes: [],
    pos_y_keyframes: [],
    scale_keyframes: [],
    rotation_keyframes: [],
    opacity_keyframes: [],
    words: [],
    highlight_enabled: false,
    highlight_color_rgba: [255, 220, 0, 255],
    text_align: "Auto",
  };
}

function shapeClip(id: number, start: number, duration: number): ShapeClip {
  return {
    id,
    start_secs: start,
    duration_secs: duration,
    shape_kind: "Ellipse",
    center_x: 0.5,
    center_y: 0.5,
    center_x_keyframes: [],
    center_y_keyframes: [],
    width: 0.3,
    height: 0.3,
    width_keyframes: [],
    height_keyframes: [],
    rotation_deg: 0,
    rotation_keyframes: [],
    stroke_thickness_px: 0,
    color_rgba: [255, 255, 255, 255],
  };
}

const rowY = (index: number) => {
  return RULER_HEIGHT + index * TRACK_HEIGHT + TRACK_HEIGHT / 2;
};

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
    expect(hitTest(tracks, 41, rowY(0), 40, 0)).toEqual({
      part: "start",
      clipId: 10,
      trackIndex: 0,
    });
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
    expect(hitTest([textTrack, shapeTrack], 41, rowY(0), 40, 0)).toEqual({
      part: "start",
      clipId: 31,
      trackIndex: 0,
    });
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
    expect(found).toEqual({
      span: { id: 5, start_secs: 3, duration_secs: 1.5, overlay: true },
      trackIndex: 1,
    });
    expect(findSpan([media, textTrack], 1)?.span).toEqual({
      id: 1,
      start_secs: 0,
      duration_secs: 2,
      overlay: false,
    });
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

describe("long timelines", () => {
  it("spaces ruler ticks by whole hours when zoomed far out", () => {
    const ticks = rulerTicks(0.02, 0, 1000);
    expect(ticks[1] - ticks[0]).toBe(7200);
    expect(rulerTicks(0.01, 0, 1000)[1]).toBe(14400);
  });

  it("labels seconds, minutes and hours", () => {
    expect(rulerLabel(3)).toBe("0:03");
    expect(rulerLabel(2.5)).toBe("0:02.50");
    expect(rulerLabel(754)).toBe("12:34");
    expect(rulerLabel(7200)).toBe("2:00:00");
    expect(rulerLabel(9915)).toBe("2:45:15");
  });

  it("fits a two-hour timeline into the window", () => {
    const px = fitPxPerSec(7200, 1224, 0.01, 600);
    expect(px).toBeCloseTo(1200 / 7200);
    expect(px * 7200).toBeLessThanOrEqual(1224);
  });

  it("clamps the fit and copes with an empty timeline", () => {
    expect(fitPxPerSec(1e9, 1000, 0.01, 600)).toBe(0.01);
    expect(fitPxPerSec(0.1, 1000, 0.01, 600)).toBe(600);
    expect(fitPxPerSec(0, 1000, 0.01, 600)).toBe(0.01);
    expect(fitPxPerSec(10, 0, 0.01, 600)).toBe(0.01);
  });

  it("zooms out far enough to see ten hours in a normal window", () => {
    let px = 40;
    for (let i = 0; i < 60; i += 1) {
      px = zoomAround(500, px, 0, 1 / 1.5, 0.01, 600).pxPerSec;
    }
    expect(px).toBe(0.01);
    expect(36000 * px).toBeLessThan(500);
  });
});

describe("dissolveSpanSecs", () => {
  const trackWith = (clips: ClipInstance[]): Track => {
    return track(1, clips);
  };

  it("is the real overlap with the clip before, cut to the transition duration", () => {
    const second = { ...clip(2, 9, 10), transition_in: "Dissolve" as const };
    second.transition_duration_secs = 1.5;
    const track = trackWith([clip(1, 0, 10), second]);
    expect(dissolveSpanSecs(track, second)).toBe(1);
    second.transition_duration_secs = 0.5;
    expect(dissolveSpanSecs(track, second)).toBe(0.5);
  });

  it("is zero without an overlap or for other transitions", () => {
    const apart = { ...clip(2, 10, 10), transition_in: "Dissolve" as const };
    expect(dissolveSpanSecs(trackWith([clip(1, 0, 10), apart]), apart)).toBe(0);
    const fade = { ...clip(2, 9, 10), transition_in: "Fade" as const };
    expect(dissolveSpanSecs(trackWith([clip(1, 0, 10), fade]), fade)).toBe(0);
  });
});

describe("effectTarget", () => {
  const video = track(1, [clip(1, 0, 10)]);
  const overlayTrack = { ...track(2, []), kind: "Text" as const, text_clips: [textClip(5, 0, 4)] };
  const locked = { ...track(3, [clip(7, 0, 10)]), locked: true };
  const lane = (index: number) => {
    return RULER_HEIGHT + index * TRACK_HEIGHT + TRACK_HEIGHT / 2;
  };

  it("finds the video clip under the point and its track", () => {
    const found = effectTarget([video], 200, lane(0), 40, 0);
    expect(found?.span.id).toBe(1);
    expect(found?.trackIndex).toBe(0);
  });

  it("is null over the ruler, an empty stretch, text overlays and locked tracks", () => {
    const tracks = [video, overlayTrack, locked];
    expect(effectTarget(tracks, 200, RULER_HEIGHT - 4, 40, 0)).toBeNull();
    expect(effectTarget(tracks, 900, lane(0), 40, 0)).toBeNull();
    expect(effectTarget(tracks, -3, lane(0), 40, 0)).toBeNull();
    expect(effectTarget(tracks, 40, lane(1), 40, 0)).toBeNull();
    expect(effectTarget(tracks, 200, lane(2), 40, 0)).toBeNull();
  });
});
