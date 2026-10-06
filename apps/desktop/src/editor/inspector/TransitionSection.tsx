import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./inspector.module.css";
import { t } from "../../i18n";
import {
  BLEND_MODES,
  type BlendMode,
  type ClipInstance,
  type ClipPatch,
  type TransitionType,
} from "../../types";
import { Section, Slider } from "./controls";

const TRANSITION_TYPES: { value: TransitionType; label: () => string }[] = [
  {
    value: "None",
    label: () => {
      return t("transitionNone");
    },
  },
  {
    value: "Fade",
    label: () => {
      return t("transitionFade");
    },
  },
  {
    value: "HardCut",
    label: () => {
      return t("transitionHardCut");
    },
  },
  {
    value: "Slide",
    label: () => {
      return t("transitionSlide");
    },
  },
  {
    value: "Zoom",
    label: () => {
      return t("transitionZoom");
    },
  },
  {
    value: "Dissolve",
    label: () => {
      return t("transitionDissolve");
    },
  },
];

type Commit = (patch: ClipPatch) => Promise<unknown>;

function formatSeconds(v: number): string {
  return `${v.toFixed(1)}s`;
}

export function TransitionSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  return (
    <>
      <Section
        label={t("transition")}
        defaultOpen={
          clip.transition_in !== "None" || clip.fade_in_secs > 0 || clip.fade_out_secs > 0
        }
        group="effects"
      >
        <div className={cx(styles, baseStyles, "field")}>
          <label>{t("transition")}</label>
          <select
            value={clip.transition_in}
            onChange={(e) => {
              return commit({ transitionIn: e.target.value as TransitionType });
            }}
          >
            {TRANSITION_TYPES.map(({ value, label }) => {
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
        {clip.transition_in !== "None" && (
          <Slider
            label={t("transitionDuration")}
            value={clip.transition_duration_secs}
            min={0.1}
            max={3}
            step={0.1}
            format={(v) => {
              return `${v.toFixed(1)}s`;
            }}
            onCommit={(v) => {
              return commit({ transitionDurationSecs: v });
            }}
          />
        )}
        <Slider
          label={t("fadeIn")}
          value={clip.fade_in_secs}
          min={0}
          max={3}
          step={0.1}
          format={formatSeconds}
          onCommit={(v) => {
            return commit({ fadeInSecs: v });
          }}
        />
        <Slider
          label={t("fadeOut")}
          value={clip.fade_out_secs}
          min={0}
          max={3}
          step={0.1}
          format={formatSeconds}
          onCommit={(v) => {
            return commit({ fadeOutSecs: v });
          }}
        />
      </Section>

      <div
        className={cx(styles, baseStyles, "field")}
        data-group="inspector"
      >
        <label>{t("blendMode")}</label>
        <select
          value={clip.blend_mode}
          onChange={(e) => {
            return commit({ blendMode: e.target.value as BlendMode });
          }}
        >
          {BLEND_MODES.map((mode) => {
            return (
              <option
                key={mode}
                value={mode}
              >
                {mode}
              </option>
            );
          })}
        </select>
      </div>
    </>
  );
}
