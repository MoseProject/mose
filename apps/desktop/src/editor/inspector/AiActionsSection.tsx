import { useState } from "react";
import { t } from "../../i18n";
import { api } from "../../ipc";
import { run } from "../../store";
import { type ClipInstance } from "../../types";
import { Section } from "./controls";

export function AiActionsSection({ clip }: { clip: ClipInstance }) {
  const [region, setRegion] = useState({ centerX: 0.5, centerY: 0.5, width: 0.3, height: 0.3 });
  return (
    <>
      <Section
        label={t("aiActions")}
        defaultOpen={false}
        group="effects"
      >
        <div className="region-row">
          <input
            type="number"
            step={0.05}
            min={0}
            max={1}
            value={region.centerX}
            title={t("centerX")}
            onChange={(e) => {
              return setRegion({ ...region, centerX: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0}
            max={1}
            value={region.centerY}
            title={t("centerY")}
            onChange={(e) => {
              return setRegion({ ...region, centerY: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0.05}
            max={0.6}
            value={region.width}
            title={t("regionWidth")}
            onChange={(e) => {
              return setRegion({ ...region, width: Number(e.target.value) });
            }}
          />
          <input
            type="number"
            step={0.05}
            min={0.05}
            max={0.6}
            value={region.height}
            title={t("regionHeight")}
            onChange={(e) => {
              return setRegion({ ...region, height: Number(e.target.value) });
            }}
          />
        </div>
        <button
          onClick={() => {
            return run(
              api.motionTrackClip(
                clip.id,
                region.centerX,
                region.centerY,
                region.width,
                region.height,
                0.08,
              ),
            );
          }}
        >
          {t("trackMotion")}
        </button>
        <button
          onClick={() => {
            return run(api.autoReframeClip(clip.id));
          }}
        >
          {t("autoReframe")}
        </button>
        <button
          onClick={() => {
            return run(api.dynamicAutoReframeClip(clip.id));
          }}
        >
          {t("dynamicAutoReframe")}
        </button>
        <button
          onClick={() => {
            return run(api.generateMatteForClip(clip.id));
          }}
        >
          {t("generateMatte")}
        </button>
        <button
          onClick={() => {
            return run(
              api.applyPrivacyBlurToClip(
                clip.id,
                region.centerX,
                region.centerY,
                region.width,
                region.height,
              ),
            );
          }}
        >
          {t("applyPrivacyBlur")}
        </button>
      </Section>
    </>
  );
}
