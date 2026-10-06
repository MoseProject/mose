import { describe, expect, it } from "vitest";
import { applyOps } from "../src/sessionPatch";

const base = () => {
  return {
    name: "P",
    sequences: [
      {
        timeline: {
          tracks: [
            { id: 1, clips: [{ id: 1 }, { id: 2 }] },
            { id: 2, clips: [{ id: 3 }] },
          ],
        },
      },
    ],
  };
};

const ids = (clip: { id: number }) => {
  return clip.id;
};

describe("applyOps", () => {
  it("sets a value and leaves the input alone", () => {
    const before = base();
    const after = applyOps(before, [{ p: ["name"], v: "Q" }]);
    expect(after.name).toBe("Q");
    expect(before.name).toBe("P");
  });

  it("keeps the identity of everything that did not change", () => {
    const before = base();
    const after = applyOps(before, [
      { p: ["sequences", 0, "timeline", "tracks", 0, "clips", 1], v: { id: 9 } },
    ]);
    const tracks = after.sequences[0].timeline.tracks;
    expect(tracks[0].clips[1]).toEqual({ id: 9 });
    expect(tracks[0].clips[0]).toBe(before.sequences[0].timeline.tracks[0].clips[0]);
    expect(tracks[1]).toBe(before.sequences[0].timeline.tracks[1]);
    expect(tracks[0]).not.toBe(before.sequences[0].timeline.tracks[0]);
  });

  it("appends at the length and cuts with len", () => {
    const before = base();
    const grown = applyOps(before, [
      { p: ["sequences", 0, "timeline", "tracks", 1, "clips", 1], v: { id: 4 } },
      { p: ["sequences", 0, "timeline", "tracks", 1, "clips", 2], v: { id: 5 } },
    ]);
    expect(grown.sequences[0].timeline.tracks[1].clips.map(ids)).toEqual([3, 4, 5]);
    const cut = applyOps(grown, [
      { p: ["sequences", 0, "timeline", "tracks", 1, "clips"], len: 1 },
    ]);
    expect(cut.sequences[0].timeline.tracks[1].clips.map(ids)).toEqual([3]);
    expect(grown.sequences[0].timeline.tracks[1].clips).toHaveLength(3);
  });

  it("applies many ops to one array without copying it for each", () => {
    const clips = Array.from({ length: 2000 }, (_, id) => {
      return { id };
    });
    const root = { tracks: [{ clips }] };
    const ops = clips.map((_, i) => {
      return { p: ["tracks", 0, "clips", i], v: { id: i + 1 } };
    });
    const started = performance.now();
    const after = applyOps(root, ops);
    expect(after.tracks[0].clips[1999]).toEqual({ id: 2000 });
    expect(performance.now() - started).toBeLessThan(200);
  });
});
