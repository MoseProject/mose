import { t } from "../../i18n";
import type { TextClip, TextClipPatch } from "../../types";
import { Section } from "../inspector/controls";

type Commit = (patch: TextClipPatch) => void;

export function TimingSection({ clip, commit }: { clip: TextClip; commit: Commit }) {
  return (
    <>
      <Section
        label={t("textTiming")}
        defaultOpen
        testId="section-text-timing"
      >
        <div className="field">
          <label>{t("overlayStart")}</label>
          <input
            type="number"
            min={0}
            step={0.1}
            defaultValue={clip.start_secs}
            onBlur={(e) => {
              return commit({ startSecs: Number(e.target.value) });
            }}
          />
        </div>
        <div className="field">
          <label>{t("overlayDuration")}</label>
          <input
            type="number"
            min={0.1}
            step={0.1}
            defaultValue={clip.duration_secs}
            onBlur={(e) => {
              return commit({ durationSecs: Number(e.target.value) });
            }}
          />
        </div>
      </Section>
    </>
  );
}
