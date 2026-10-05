import { t } from "../../i18n";
import type { TextClip, TextClipPatch } from "../../types";
import { Section, Slider } from "../inspector/controls";
import { ColorField } from "../OverlayFields";

type Commit = (patch: TextClipPatch) => void;

export function BackgroundSection({ clip, commit }: { clip: TextClip; commit: Commit }) {
  const backgroundEnabled = clip.background_rgba[3] > 0;
  return (
    <>
      <Section
        label={t("textBackground")}
        defaultOpen={backgroundEnabled}
        testId="section-text-background"
      >
        <label className="check">
          <input
            type="checkbox"
            checked={backgroundEnabled}
            onChange={(e) => {
              return commit({
                backgroundRgba: [
                  clip.background_rgba[0],
                  clip.background_rgba[1],
                  clip.background_rgba[2],
                  e.target.checked ? 192 : 0,
                ],
              });
            }}
          />
          {t("backgroundEnabled")}
        </label>
        {backgroundEnabled && (
          <>
            <ColorField
              label={t("backgroundColor")}
              rgb={clip.background_rgba}
              onCommit={([r, g, b]) => {
                return commit({ backgroundRgba: [r, g, b, clip.background_rgba[3]] });
              }}
            />
            <Slider
              label={t("backgroundPadding")}
              value={clip.background_padding}
              min={0}
              max={64}
              step={1}
              format={(v) => {
                return `${v.toFixed(0)} px`;
              }}
              onCommit={(backgroundPadding) => {
                return commit({ backgroundPadding });
              }}
            />
            <Slider
              label={t("backgroundRadius")}
              value={clip.background_corner_radius}
              min={0}
              max={64}
              step={1}
              format={(v) => {
                return `${v.toFixed(0)} px`;
              }}
              onCommit={(backgroundCornerRadius) => {
                return commit({ backgroundCornerRadius });
              }}
            />
          </>
        )}
      </Section>
    </>
  );
}
