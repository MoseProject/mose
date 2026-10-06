import { t } from "../../../i18n";
import type { TextClip, TextClipPatch } from "../../../types";
import { Section, Slider } from "../controls";

type Commit = (patch: TextClipPatch) => void;

export function PositionSection({ clip, commit }: { clip: TextClip; commit: Commit }) {
  return (
    <>
      <Section
        label={t("textPosition")}
        defaultOpen
        testId="section-text-position"
      >
        <Slider
          label={t("positionX")}
          value={clip.pos_x}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return `${(v * 100).toFixed(0)}%`;
          }}
          onCommit={(posX) => {
            return commit({ posX });
          }}
        />
        <Slider
          label={t("positionY")}
          value={clip.pos_y}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return `${(v * 100).toFixed(0)}%`;
          }}
          onCommit={(posY) => {
            return commit({ posY });
          }}
        />
      </Section>
    </>
  );
}
