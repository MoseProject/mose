import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import { hasKeyframeAt, valueAt, withKeyframe, withoutKeyframe } from "../../keyframeAt";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider, type SectionKeyframe } from "./controls";
import { animatedRow } from "./keyframeRow";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function ColorSection({
  clip,
  commit,
  fraction,
}: {
  clip: ClipInstance;
  commit: Commit;
  fraction: number | null;
}) {
  const colorDiamond: SectionKeyframe = {
    active:
      fraction !== null &&
      (hasKeyframeAt(clip.brightness_keyframes, fraction) ||
        hasKeyframeAt(clip.contrast_keyframes, fraction) ||
        hasKeyframeAt(clip.saturation_keyframes, fraction)),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction === null) {
        return;
      }
      const lists = [
        ["brightnessKeyframes", clip.brightness_keyframes, clip.brightness],
        ["contrastKeyframes", clip.contrast_keyframes, clip.contrast],
        ["saturationKeyframes", clip.saturation_keyframes, clip.saturation],
      ] as const;
      const remove = lists.some(([, list]) => {
        return hasKeyframeAt(list, fraction);
      });
      const patch: ClipPatch = {};
      for (const [key, list, base] of lists) {
        patch[key] = remove
          ? withoutKeyframe(list, fraction)
          : withKeyframe(list, fraction, valueAt(list, fraction, base));
      }
      void commit(patch);
    },
  };
  const brightness = animatedRow(
    clip.brightness_keyframes,
    fraction,
    clip.brightness,
    (next) => {
      return commit({ brightnessKeyframes: next });
    },
    (value) => {
      return commit({ brightness: value });
    },
  );
  const contrast = animatedRow(
    clip.contrast_keyframes,
    fraction,
    clip.contrast,
    (next) => {
      return commit({ contrastKeyframes: next });
    },
    (value) => {
      return commit({ contrast: value });
    },
  );
  const saturation = animatedRow(
    clip.saturation_keyframes,
    fraction,
    clip.saturation,
    (next) => {
      return commit({ saturationKeyframes: next });
    },
    (value) => {
      return commit({ saturation: value });
    },
  );
  return (
    <>
      <Section
        group="effects"
        label={t("colorAdjust")}
        keyframe={colorDiamond}
        defaultOpen={clip.brightness !== 0 || clip.contrast !== 1 || clip.saturation !== 1}
        onReset={() => {
          void commit({
            brightness: 0,
            contrast: 1,
            saturation: 1,
            brightnessKeyframes: [],
            contrastKeyframes: [],
            saturationKeyframes: [],
          });
        }}
      >
        <Slider
          label={t("brightness")}
          value={brightness.shown}
          keyframe={brightness.diamond}
          min={-1}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return brightness.edit(v);
          }}
        />
        <Slider
          label={t("contrast")}
          value={contrast.shown}
          keyframe={contrast.diamond}
          min={0}
          max={2}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return contrast.edit(v);
          }}
        />
        <Slider
          label={t("saturation")}
          value={saturation.shown}
          keyframe={saturation.diamond}
          min={0}
          max={2}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return saturation.edit(v);
          }}
        />
      </Section>
      <div
        className={cx(styles, baseStyles, "clip-checks")}
        data-group="inspector"
      >
        <label className={cx(styles, baseStyles, "check")}>
          <input
            type="checkbox"
            checked={clip.frozen}
            onChange={(e) => {
              return commit({ frozen: e.target.checked });
            }}
          />
          {t("freeze")}
        </label>
        <label className={cx(styles, baseStyles, "check")}>
          <input
            type="checkbox"
            checked={clip.flipped_h}
            onChange={(e) => {
              return commit({ flippedH: e.target.checked });
            }}
          />
          {t("flip")}
        </label>
      </div>
      <div
        className={cx(styles, baseStyles, "clip-checks")}
        data-group="audio"
      >
        <label className={cx(styles, baseStyles, "check")}>
          <input
            type="checkbox"
            checked={clip.voice_cleanup_enabled}
            onChange={(e) => {
              return commit({ voiceCleanupEnabled: e.target.checked });
            }}
          />
          {t("voiceCleanup")}
        </label>
      </div>
    </>
  );
}
