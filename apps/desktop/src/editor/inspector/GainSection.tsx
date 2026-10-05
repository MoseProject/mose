import { t } from "../../i18n";
import { hasKeyframeAt, toggleKeyframe, valueAt } from "../../keyframeAt";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider, type SectionKeyframe } from "./controls";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function GainSection({
  clip,
  commit,
  fraction,
}: {
  clip: ClipInstance;
  commit: Commit;
  fraction: number | null;
}) {
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
      </Section>
    </>
  );
}
