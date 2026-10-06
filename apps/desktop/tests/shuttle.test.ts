import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../src/ipc", () => {
  return {
    api: {
      seek: vi.fn().mockResolvedValue(undefined),
      setPlaying: vi.fn().mockResolvedValue(undefined),
    },
  };
});

const { api } = await import("../src/ipc");
const { nextShuttle, playRange, shuttle, shuttleStore, stopRange, stopShuttle } =
  await import("../src/shuttle");
const { previewStore, sessionStore, uiStore } = await import("../src/store");
const { makeSession } = await import("./support/testSupport");

describe("nextShuttle", () => {
  it("starts at 1x in a new direction", () => {
    expect(nextShuttle({ direction: 0, level: 1 }, 1)).toEqual({ direction: 1, level: 1 });
    expect(nextShuttle({ direction: 1, level: 4 }, -1)).toEqual({ direction: -1, level: 1 });
  });

  it("doubles with each repeat and stops climbing at 8x", () => {
    let state = nextShuttle({ direction: 0, level: 1 }, -1);
    const levels = [state.level];
    for (let i = 0; i < 4; i += 1) {
      state = nextShuttle(state, -1);
      levels.push(state.level);
    }
    expect(levels).toEqual([1, 2, 4, 8, 8]);
  });
});

describe("shuttle", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.clearAllMocks();
    stopShuttle();
    sessionStore.set(
      makeSession([
        {
          id: 1,
          name: "S",
          tracks: [
            {
              id: 1,
              clips: [
                { id: 1, start_secs: 0, source_in_secs: 0, source_out_secs: 10, speed_factor: 1 },
              ],
              text_clips: [],
              shape_clips: [],
            },
          ],
        },
      ]),
    );
    previewStore.set({ ...previewStore.get(), playheadSecs: 5, playing: false });
  });
  afterEach(() => {
    stopShuttle();
    vi.useRealTimers();
  });

  it("plays for real at forward 1x", async () => {
    await shuttle(1);
    expect(api.setPlaying).toHaveBeenCalledWith(true);
    vi.advanceTimersByTime(500);
    expect(api.seek).not.toHaveBeenCalled();
  });

  it("walks the playhead backwards with timed seeks", async () => {
    await shuttle(-1);
    expect(api.setPlaying).toHaveBeenCalledWith(false);
    vi.advanceTimersByTime(160);
    expect(api.seek).toHaveBeenCalledTimes(2);
    expect(vi.mocked(api.seek).mock.calls[1][0]).toBeCloseTo(4.84);
  });

  it("moves faster on each repeat", async () => {
    await shuttle(1);
    await shuttle(1);
    expect(shuttleStore.get()).toEqual({ direction: 1, level: 2 });
    vi.advanceTimersByTime(80);
    expect(vi.mocked(api.seek).mock.calls[0][0]).toBeCloseTo(5.16);
  });

  it("stops by itself at the start of the timeline", async () => {
    previewStore.set({ ...previewStore.get(), playheadSecs: 0.1 });
    await shuttle(-1);
    vi.advanceTimersByTime(400);
    expect(shuttleStore.get().direction).toBe(0);
    const calls = vi.mocked(api.seek).mock.calls.length;
    vi.advanceTimersByTime(400);
    expect(api.seek).toHaveBeenCalledTimes(calls);
    expect(vi.mocked(api.seek).mock.calls.at(-1)![0]).toBe(0);
  });

  it("ends the shuttle when a seek fails instead of repeating the error", async () => {
    vi.mocked(api.seek).mockRejectedValue("seek failed");
    await shuttle(-1);
    vi.advanceTimersByTime(80);
    await Promise.resolve();
    await Promise.resolve();
    expect(shuttleStore.get().direction).toBe(0);
    vi.advanceTimersByTime(400);
    expect(api.seek).toHaveBeenCalledTimes(1);
    expect(uiStore.get().error).toBe("seek failed");
    vi.mocked(api.seek).mockResolvedValue(undefined);
  });

  it("stops walking once stopShuttle is called", async () => {
    await shuttle(-1);
    vi.advanceTimersByTime(80);
    stopShuttle();
    vi.advanceTimersByTime(400);
    expect(api.seek).toHaveBeenCalledTimes(1);
    expect(shuttleStore.get().direction).toBe(0);
  });
});

describe("playRange", () => {
  const tickAt = (playheadSecs: number, playing: boolean) => {
    previewStore.set({ ...previewStore.get(), playheadSecs, playing });
  };

  beforeEach(() => {
    vi.clearAllMocks();
    stopShuttle();
    stopRange();
    tickAt(0, false);
    uiStore.set({ ...uiStore.get(), markIn: 2, markOut: 6 });
  });
  afterEach(() => {
    stopRange();
  });

  it("does nothing without a valid range", async () => {
    uiStore.set({ ...uiStore.get(), markIn: 6, markOut: 2 });
    await playRange();
    expect(api.seek).not.toHaveBeenCalled();
    expect(api.setPlaying).not.toHaveBeenCalled();
  });

  it("seeks to the in mark and plays, then pauses when the out mark is reached", async () => {
    await playRange();
    expect(api.seek).toHaveBeenCalledWith(2);
    expect(api.setPlaying).toHaveBeenCalledWith(true);

    tickAt(2, true);
    tickAt(4, true);
    expect(api.setPlaying).toHaveBeenCalledTimes(1);
    tickAt(6.02, true);
    expect(api.setPlaying).toHaveBeenLastCalledWith(false);
  });

  it("ignores ticks from before the playback request took effect", async () => {
    tickAt(20, false);
    await playRange();
    tickAt(20, false);
    expect(api.setPlaying).toHaveBeenCalledTimes(1);
  });

  it("stops watching after a pause by hand, so later playback runs past the out mark", async () => {
    await playRange();
    tickAt(3, true);
    tickAt(3, false);
    tickAt(6.1, true);
    tickAt(6.2, true);
    expect(api.setPlaying).toHaveBeenCalledTimes(1);
  });
});
