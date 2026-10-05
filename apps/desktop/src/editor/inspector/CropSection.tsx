import { t } from "../../i18n";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section, Slider } from "./controls";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function CropSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  return (
    <>
      <Section
        label={t("crop")}
        defaultOpen={
          clip.crop_x !== 0 || clip.crop_y !== 0 || clip.crop_w !== 1 || clip.crop_h !== 1
        }
        onReset={() => {
          void commit({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 });
        }}
      >
        <Slider
          label="x"
          value={clip.crop_x}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropX: v });
          }}
        />
        <Slider
          label="y"
          value={clip.crop_y}
          min={0}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropY: v });
          }}
        />
        <Slider
          label="w"
          value={clip.crop_w}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropW: v });
          }}
        />
        <Slider
          label="h"
          value={clip.crop_h}
          min={0.05}
          max={1}
          step={0.01}
          format={(v) => {
            return v.toFixed(2);
          }}
          onCommit={(v) => {
            return commit({ cropH: v });
          }}
        />
        <button
          className="icon"
          onClick={() => {
            return commit({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 });
          }}
        >
          {t("cropReset")}
        </button>
      </Section>
    </>
  );
}
