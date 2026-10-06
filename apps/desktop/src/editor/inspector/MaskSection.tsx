import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import { type ClipInstance, type ClipPatch, type MaskShape } from "../../types";
import { Section, Slider } from "./controls";

const MASK_SHAPES: { value: MaskShape; label: () => string }[] = [
  {
    value: "None",
    label: () => {
      return t("maskNone");
    },
  },
  {
    value: "Circle",
    label: () => {
      return t("maskCircle");
    },
  },
  {
    value: "RoundedRect",
    label: () => {
      return t("maskRoundedRect");
    },
  },
];

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function MaskSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  return (
    <>
      <Section
        label={t("mask")}
        defaultOpen={clip.mask_shape !== "None"}
        group="effects"
      >
        <div className={cx(styles, baseStyles, "field")}>
          <label>{t("mask")}</label>
          <select
            value={clip.mask_shape}
            onChange={(e) => {
              return commit({ maskShape: e.target.value as MaskShape });
            }}
          >
            {MASK_SHAPES.map(({ value, label }) => {
              return (
                <option
                  key={value}
                  value={value}
                >
                  {label()}
                </option>
              );
            })}
          </select>
        </div>
        {clip.mask_shape === "RoundedRect" && (
          <Slider
            label={t("maskCornerRadius")}
            value={clip.mask_corner_radius}
            min={0}
            max={1}
            step={0.01}
            format={(v) => {
              return v.toFixed(2);
            }}
            onCommit={(v) => {
              return commit({ maskCornerRadius: v });
            }}
          />
        )}
      </Section>
    </>
  );
}
