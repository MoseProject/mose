import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider } from "./controls";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function SpeedSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  return (
    <>
      <Section
        label={t("speed")}
        defaultOpen={clip.speed_factor !== 1}
        onReset={() => {
          void commit({ speedFactor: 1 });
        }}
      >
        <Slider
          label={t("speed")}
          value={clip.speed_factor}
          min={0.25}
          max={4}
          step={0.05}
          format={(v) => {
            return `${v.toFixed(2)}×`;
          }}
          onCommit={(v) => {
            return commit({ speedFactor: v });
          }}
        />
        <div className={cx(styles, baseStyles, "field")}>
          <span>{t("speedRamp")}</span>
          <div className={cx(styles, baseStyles, "export-actions")}>
            <button
              className={cx(styles, baseStyles, "icon")}
              onClick={() => {
                return run(api.applySpeedRampToClip(clip.id, 0.5, 2.0, 4));
              }}
            >
              {t("speedRampSlowToFast")}
            </button>
            <button
              className={cx(styles, baseStyles, "icon")}
              onClick={() => {
                return run(api.applySpeedRampToClip(clip.id, 2.0, 0.5, 4));
              }}
            >
              {t("speedRampFastToSlow")}
            </button>
          </div>
        </div>
      </Section>
    </>
  );
}
