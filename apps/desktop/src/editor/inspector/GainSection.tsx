import { t } from "../../i18n";
import { hasKeyframeAt, toggleKeyframe, valueAt } from "../../keyframeAt";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider, type SectionKeyframe } from "./controls";
import { normalizeGainDb } from "./normalizeGain";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function GainSection({
  clip,
  commit,
  fraction,
  measuredLufs,
  targetLufs,
}: {
  clip: ClipInstance;
  commit: Commit;
  fraction: number | null;
  /** Integrated loudness of the clip's source; null until it has been measured. */
  measuredLufs: number | null;
  targetLufs: number;
}) {
  const normalizeTo = measuredLufs === null ? null : normalizeGainDb(measuredLufs, targetLufs);
  const gainDiamond: SectionKeyframe = {
    active: fraction !== null && hasKeyframeAt(clip.gain_keyframes, fraction),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction !== null) {
        void commit({
          gainKeyframes: toggleKeyframe(
            clip.gain_keyframes,
            fraction,
            valueAt(clip.gain_keyframes, fraction, clip.gain_db),
          ),
        });
      }
    },
  };
  return (
    <>
      <Section
        label={t("gain")}
        defaultOpen={clip.gain_db !== 0}
        testId="section-gain"
        group="audio"
        keyframe={gainDiamond}
        onReset={() => {
          void commit({ gainDb: 0 });
        }}
      >
        <Slider
          label={t("gain")}
          value={clip.gain_db}
          min={-24}
          max={24}
          step={0.5}
          format={(v) => {
            return `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`;
          }}
          onCommit={(v) => {
            return commit({ gainDb: v });
          }}
        />
        <button
          disabled={normalizeTo === null || normalizeTo === clip.gain_db}
          title={t("normalizeClipNowHint")}
          data-testid="normalize-clip-now"
          onClick={() => {
            if (normalizeTo !== null) {
              void commit({ gainDb: normalizeTo });
            }
          }}
        >
          {t("normalizeClipNow")}
        </button>
      </Section>
    </>
  );
}
