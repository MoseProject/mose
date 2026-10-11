import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider } from "./controls";
import { animatedRow } from "./keyframeRow";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function CropSection({
  clip,
  commit,
  fraction,
}: {
  clip: ClipInstance;
  commit: Commit;
  /** The playhead inside the clip (0..1), null outside it. */
  fraction: number | null;
}) {
  const edge = (
    list: ClipInstance["crop_x_keyframes"],
    value: number,
    listKey: "cropXKeyframes" | "cropYKeyframes" | "cropWKeyframes" | "cropHKeyframes",
    staticKey: "cropX" | "cropY" | "cropW" | "cropH",
  ) => {
    return animatedRow(
      list,
      fraction,
      value,
      (next) => {
        return commit({ [listKey]: next });
      },
      (v) => {
        return commit({ [staticKey]: v });
      },
    );
  };
  const x = edge(clip.crop_x_keyframes, clip.crop_x, "cropXKeyframes", "cropX");
  const y = edge(clip.crop_y_keyframes, clip.crop_y, "cropYKeyframes", "cropY");
  const w = edge(clip.crop_w_keyframes, clip.crop_w, "cropWKeyframes", "cropW");
  const h = edge(clip.crop_h_keyframes, clip.crop_h, "cropHKeyframes", "cropH");
  return (
    <>
      <Section
        label={t("crop")}
        defaultOpen={
          clip.crop_x !== 0 || clip.crop_y !== 0 || clip.crop_w !== 1 || clip.crop_h !== 1
        }
        onReset={() => {
          void commit({
            cropX: 0,
            cropY: 0,
            cropW: 1,
            cropH: 1,
            cropXKeyframes: [],
            cropYKeyframes: [],
            cropWKeyframes: [],
            cropHKeyframes: [],
          });
        }}
      >
        <Slider
          label="x"
          value={x.shown}
          keyframe={x.diamond}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return x.edit(v);
          }}
        />
        <Slider
          label="y"
          value={y.shown}
          keyframe={y.diamond}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return y.edit(v);
          }}
        />
        <Slider
          label="w"
          value={w.shown}
          keyframe={w.diamond}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return w.edit(v);
          }}
        />
        <Slider
          label="h"
          value={h.shown}
          keyframe={h.diamond}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return h.edit(v);
          }}
        />
        <button
          className={cx(styles, baseStyles, "icon")}
          onClick={() => {
            return commit({
              cropX: 0,
              cropY: 0,
              cropW: 1,
              cropH: 1,
              cropXKeyframes: [],
              cropYKeyframes: [],
              cropWKeyframes: [],
              cropHKeyframes: [],
            });
          }}
        >
          {t("cropReset")}
        </button>
      </Section>
    </>
  );
}
