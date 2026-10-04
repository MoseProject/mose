// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { buildPyramid, frameStepSecs, peakRange } from "./timelineMedia";

describe("frameStepSecs", () => {
  it("picks the coarsest step that still gives each tile its own frame", () => {
    expect(frameStepSecs(0.05)).toBe(0.1);
    expect(frameStepSecs(2.25)).toBe(2);
    expect(frameStepSecs(640)).toBe(600);
    expect(frameStepSecs(1e9)).toBe(3600);
  });
});

describe("peak pyramid", () => {
  // 64 buckets over 64 s: bucket i spans [i, i + 1) and peaks at +-(i / 64).
  const pairs = new Float32Array(128);
  for (let i = 0; i < 64; i += 1) {
    pairs[2 * i] = -i / 64;
    pairs[2 * i + 1] = i / 64;
  }
  const pyramid = buildPyramid(pairs, 64)!;
  const out: [number, number] = [0, 0];

  it("folds buckets into coarser levels", () => {
    expect(pyramid.bucketsPerSec).toBe(1);
    expect(pyramid.levels.length).toBeGreaterThan(1);
    expect(pyramid.levels[1].length >> 1).toBe(8);
  });

  it("reads exact buckets at a fine range", () => {
    expect(peakRange(pyramid, 10, 11, out)).toBe(true);
    expect(out).toEqual([-10 / 64, 10 / 64]);
  });

  it("covers the whole range when zoomed out", () => {
    expect(peakRange(pyramid, 0, 64, out)).toBe(true);
    expect(out[0]).toBeCloseTo(-63 / 64);
    expect(out[1]).toBeCloseTo(63 / 64);
  });

  it("reports no data past the end and builds nothing from nothing", () => {
    expect(peakRange(pyramid, 100, 101, out)).toBe(false);
    expect(buildPyramid(new Float32Array(0), 10)).toBeNull();
    expect(buildPyramid(pairs, 0)).toBeNull();
  });
});

describe("frame cache", () => {
  class FakeImage {
    static all: FakeImage[] = [];
    onload: (() => void) | null = null;
    onerror: (() => void) | null = null;
    src = "";
    constructor() {
      FakeImage.all.push(this);
    }
  }

  async function fresh() {
    vi.resetModules();
    vi.doMock("../ipc", () => {
      return {
        frameAtUrl: (id: number, millis: number) => {
          return `frame://${id}/${millis}`;
        },
        waveformUrl: (id: number) => {
          return `wave://${id}`;
        },
        coarseWaveformUrl: (id: number) => {
          return `coarse://${id}`;
        },
      };
    });
    FakeImage.all = [];
    vi.stubGlobal("Image", FakeImage);
    vi.stubGlobal("HTMLImageElement", FakeImage);
    return import("./timelineMedia");
  }
  const asset = (id: number, source_path: string) => {
    return { id, source_path } as never;
  };

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.doUnmock("../ipc");
  });

  it("asks again for a frame that failed, but only after the retry window", async () => {
    const media = await fresh();
    expect(media.frameAt(1, 2, 1)).toBeNull();
    expect(FakeImage.all).toHaveLength(1);
    FakeImage.all[0].onerror?.();

    expect(media.frameAt(1, 2, 1)).toBeNull();
    expect(FakeImage.all).toHaveLength(1);

    vi.setSystemTime(1_000_000 + 11_000);
    expect(media.frameAt(1, 2, 1)).toBeNull();
    expect(FakeImage.all).toHaveLength(2);
  });

  it("drops what was cached for an id once it names another file", async () => {
    const media = await fresh();
    media.syncAssets([asset(1, "C:/a.mp4")]);
    media.frameAt(1, 2, 1);
    FakeImage.all[0].onload?.();
    expect(media.frameAt(1, 2, 1)).toBe(FakeImage.all[0]);

    media.syncAssets([asset(1, "C:/b.mp4")]);
    expect(media.frameAt(1, 2, 1)).toBeNull();
    expect(FakeImage.all).toHaveLength(2);
  });

  it("keeps the cache when the same files are reported again", async () => {
    const media = await fresh();
    media.syncAssets([asset(1, "C:/a.mp4")]);
    media.frameAt(1, 2, 1);
    FakeImage.all[0].onload?.();
    media.syncAssets([asset(1, "C:/a.mp4"), asset(2, "C:/c.mp4")]);
    expect(media.frameAt(1, 2, 1)).toBe(FakeImage.all[0]);
    expect(FakeImage.all).toHaveLength(1);
  });

  it("ignores a frame that arrives after its project was replaced", async () => {
    const media = await fresh();
    media.syncAssets([asset(1, "C:/a.mp4")]);
    media.frameAt(1, 2, 1);
    const late = FakeImage.all[0];

    media.syncAssets([asset(1, "C:/b.mp4")]);
    late.onload?.();

    expect(media.frameAt(1, 2, 1)).toBeNull();
    expect(FakeImage.all).toHaveLength(2);
  });
});

describe("waveform cache", () => {
  const audio = (id: number, source_path: string) => {
    return { id, source_path, has_audio: true, duration_secs: 4, has_waveform: false } as never;
  };
  const flush = async () => {
    for (let i = 0; i < 6; i += 1) {
      await Promise.resolve();
    }
  };

  async function fresh(fetchImpl: ReturnType<typeof vi.fn>) {
    vi.resetModules();
    vi.doMock("../ipc", () => {
      return {
        frameAtUrl: () => {
          return "x";
        },
        waveformUrl: (id: number) => {
          return `wave://${id}`;
        },
        coarseWaveformUrl: (id: number) => {
          return `coarse://${id}`;
        },
      };
    });
    vi.stubGlobal("fetch", fetchImpl);
    return import("./timelineMedia");
  }

  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
    vi.doUnmock("../ipc");
  });

  it("retries a failed waveform fetch after the window and not before", async () => {
    const fetchMock = vi.fn().mockResolvedValue({ ok: false, status: 404 });
    const media = await fresh(fetchMock);
    media.peaksFor(audio(1, "C:/a.mp4"));
    await flush();
    media.peaksFor(audio(1, "C:/a.mp4"));
    expect(fetchMock).toHaveBeenCalledTimes(1);

    vi.setSystemTime(1_000_000 + 11_000);
    media.peaksFor(audio(1, "C:/a.mp4"));
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("serves the detailed waveform once fetched and refetches when the id changes file", async () => {
    const pairs = new Float32Array([-0.5, 0.5, -0.25, 0.25]);
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      arrayBuffer: () => {
        return Promise.resolve(pairs.buffer);
      },
    });
    const media = await fresh(fetchMock);
    media.syncAssets([audio(1, "C:/a.mp4")]);
    media.peaksFor(audio(1, "C:/a.mp4"));
    await flush();
    expect(media.peaksFor(audio(1, "C:/a.mp4"))).not.toBeNull();
    expect(fetchMock).toHaveBeenCalledTimes(1);

    media.syncAssets([audio(1, "C:/other.mp4")]);
    media.peaksFor(audio(1, "C:/other.mp4"));
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("shows the coarse overview at once while the detailed waveform is still being decoded", async () => {
    const overviewBytes = new Float32Array([-0.5, 0.5, -0.25, 0.25]);
    const fetchMock = vi.fn((url: string) => {
      if (url.startsWith("coarse://")) {
        return Promise.resolve({
          ok: true,
          arrayBuffer: () => {
            return Promise.resolve(overviewBytes.buffer);
          },
        });
      }
      return new Promise(() => {
        return undefined;
      });
    });
    const media = await fresh(fetchMock);
    const withOverview = { ...(audio(1, "C:/a.mp4") as object), has_waveform: true } as never;
    expect(media.peaksFor(withOverview)).toBeNull();
    await flush();
    expect(media.peaksFor(withOverview)).not.toBeNull();
    const urls = fetchMock.mock.calls.map((call) => {
      return call[0];
    });
    expect(urls.sort()).toEqual(["coarse://1", "wave://1"]);
  });

  it("does not ask for an overview an asset does not have", async () => {
    const fetchMock = vi.fn().mockReturnValue(
      new Promise(() => {
        return undefined;
      }),
    );
    const media = await fresh(fetchMock);
    media.peaksFor(audio(1, "C:/a.mp4"));
    const urls = fetchMock.mock.calls.map((call) => {
      return call[0];
    });
    expect(urls).toEqual(["wave://1"]);
  });
});
