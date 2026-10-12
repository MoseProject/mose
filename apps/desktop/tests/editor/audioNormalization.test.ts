import { describe, expect, it } from "vitest";
import { alreadyNormalized } from "../../src/editor/export/audioNormalization";
import type { SessionView } from "../../src/types";

interface ClipSpec {
  id: number;
  asset: number;
  gain?: number;
  keyframes?: number;
  nested?: number | null;
}

const session = (
  clips: ClipSpec[],
  assets: { id: number; lufs: number | null; audio?: boolean }[],
  trackOver: Record<string, unknown> = {},
) => {
  return {
    project: {
      active_sequence: 0,
      media_library: assets.map((asset) => {
        return {
          id: asset.id,
          has_audio: asset.audio ?? true,
          loudness: asset.lufs === null ? null : { integrated_lufs: asset.lufs },
        };
      }),
      sequences: [
        {
          timeline: {
            tracks: [
              {
                kind: "Video",
                muted: false,
                clips: clips.map((clip) => {
                  return {
                    id: clip.id,
                    asset_id: clip.asset,
                    gain_db: clip.gain ?? 0,
                    gain_keyframes: new Array(clip.keyframes ?? 0).fill({}),
                    nested_sequence_id: clip.nested ?? null,
                  };
                }),
                ...trackOver,
              },
            ],
          },
        },
      ],
    },
  } as unknown as SessionView;
};

describe("alreadyNormalized", () => {
  it("is true when every clip's loudness plus its gain is at the target", () => {
    const s = session(
      [
        { id: 1, asset: 1, gain: 4 },
        { id: 2, asset: 2, gain: -2 },
      ],
      [
        { id: 1, lufs: -18 },
        { id: 2, lufs: -12 },
      ],
    );
    expect(alreadyNormalized(s, -14)).toBe(true);
  });

  it("allows one LU of slack, no more", () => {
    const s = (gain: number) => {
      return session([{ id: 1, asset: 1, gain }], [{ id: 1, lufs: -18 }]);
    };
    expect(alreadyNormalized(s(3.5), -14)).toBe(true);
    expect(alreadyNormalized(s(2.5), -14)).toBe(false);
  });

  it("is false as soon as one clip is off", () => {
    const s = session(
      [
        { id: 1, asset: 1, gain: 4 },
        { id: 2, asset: 2, gain: 0 },
      ],
      [
        { id: 1, lufs: -18 },
        { id: 2, lufs: -20 },
      ],
    );
    expect(alreadyNormalized(s, -14)).toBe(false);
  });

  it("can look at just some clips", () => {
    const s = session(
      [
        { id: 1, asset: 1, gain: 4 },
        { id: 2, asset: 2, gain: 0 },
      ],
      [
        { id: 1, lufs: -18 },
        { id: 2, lufs: -20 },
      ],
    );
    expect(alreadyNormalized(s, -14, [1])).toBe(true);
    expect(alreadyNormalized(s, -14, [2])).toBe(false);
  });

  it("cannot tell without a measurement, with animated gain or a compound clip", () => {
    expect(
      alreadyNormalized(session([{ id: 1, asset: 1 }], [{ id: 1, lufs: null }]), -14),
    ).toBeNull();
    expect(
      alreadyNormalized(session([{ id: 1, asset: 1, keyframes: 2 }], [{ id: 1, lufs: -14 }]), -14),
    ).toBeNull();
    expect(
      alreadyNormalized(session([{ id: 1, asset: 1, nested: 5 }], [{ id: 1, lufs: -14 }]), -14),
    ).toBeNull();
  });

  it("ignores media without sound and muted tracks, and has no answer for silence or a bad target", () => {
    const silent = session([{ id: 1, asset: 1 }], [{ id: 1, lufs: null, audio: false }]);
    expect(alreadyNormalized(silent, -14)).toBeNull();
    const muted = session([{ id: 1, asset: 1 }], [{ id: 1, lufs: -30 }], { muted: true });
    expect(alreadyNormalized(muted, -14)).toBeNull();
    expect(
      alreadyNormalized(session([{ id: 1, asset: 1 }], [{ id: 1, lufs: -14 }]), Number.NaN),
    ).toBeNull();
  });
});
