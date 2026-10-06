// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  TIMELINE_ACTION_EVENT,
  allClipIds,
  editPoints,
  neighbourEditPoint,
  timelineAction,
  timelineDuration,
} from "../src/editing";
import { makeSession } from "./support/testSupport";
import type { SessionView } from "../src/types";

const clip = (id: number, start: number, length: number) => {
  return { id, start_secs: start, source_in_secs: 0, source_out_secs: length, speed_factor: 1 };
};
const overlay = (id: number, start: number, length: number) => {
  return { id, start_secs: start, duration_secs: length };
};

const session = (): SessionView => {
  return makeSession([
    {
      id: 1,
      name: "Main",
      tracks: [
        { id: 1, clips: [clip(10, 2, 3), clip(11, 8, 4)], text_clips: [], shape_clips: [] },
        {
          id: 2,
          clips: [],
          text_clips: [overlay(20, 1, 2)],
          shape_clips: [overlay(30, 11, 3)],
        },
      ],
    },
  ]);
};

afterEach(() => {
  vi.restoreAllMocks();
});

describe("timeline measures", () => {
  it("ends at the last clip or overlay", () => {
    expect(timelineDuration(session())).toBe(14);
  });

  it("is zero for an empty sequence", () => {
    expect(timelineDuration(makeSession([{ id: 1, name: "E", tracks: [] }]))).toBe(0);
  });

  it("lists every start and end once, sorted, including zero", () => {
    expect(editPoints(session())).toEqual([0, 1, 2, 3, 5, 8, 11, 12, 14]);
  });

  it("lists every clip and overlay id", () => {
    expect(
      allClipIds(session()).sort((a, b) => {
        return a - b;
      }),
    ).toEqual([10, 11, 20, 30]);
  });
});

describe("neighbourEditPoint", () => {
  const points = [0, 2, 5, 8];

  it("finds the next and previous cut strictly beyond the playhead", () => {
    expect(neighbourEditPoint(points, 2, 1)).toBe(5);
    expect(neighbourEditPoint(points, 2, -1)).toBe(0);
    expect(neighbourEditPoint(points, 3, 1)).toBe(5);
    expect(neighbourEditPoint(points, 3, -1)).toBe(2);
  });

  it("treats a playhead within a few milliseconds of a cut as being on it", () => {
    expect(neighbourEditPoint(points, 2.003, -1)).toBe(0);
    expect(neighbourEditPoint(points, 1.998, 1)).toBe(5);
  });

  it("has nothing past either end", () => {
    expect(neighbourEditPoint(points, 8, 1)).toBeNull();
    expect(neighbourEditPoint(points, 0, -1)).toBeNull();
  });
});

describe("timelineAction", () => {
  it("is delivered to listeners as a window event", () => {
    const seen: string[] = [];
    const listener = (e: Event) => {
      seen.push((e as CustomEvent<string>).detail);
    };
    window.addEventListener(TIMELINE_ACTION_EVENT, listener);
    timelineAction("zoomFit");
    timelineAction("renameSequence");
    window.removeEventListener(TIMELINE_ACTION_EVENT, listener);
    expect(seen).toEqual(["zoomFit", "renameSequence"]);
  });
});
