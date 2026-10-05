import { t } from "../../i18n";
import type { TextClip, TextClipPatch } from "../../types";
import { F32KeyframeList } from "../KeyframeEditor";
import { Section } from "../inspector/controls";
import { ColorField } from "../OverlayFields";

type Commit = (patch: TextClipPatch) => void;

export function KeyframesSection({ clip, commit }: { clip: TextClip; commit: Commit }) {
  return (
    <>
      <Section
        label={t("keyframes")}
        defaultOpen={
          clip.pos_x_keyframes.length > 0 ||
          clip.pos_y_keyframes.length > 0 ||
          clip.scale_keyframes.length > 0 ||
          clip.rotation_keyframes.length > 0 ||
          clip.opacity_keyframes.length > 0
        }
        testId="section-text-keyframes"
      >
        <F32KeyframeList
          label={t("posXKeyframes")}
          keyframes={clip.pos_x_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.pos_x}
          onCommit={(posXKeyframes) => {
            return commit({ posXKeyframes });
          }}
        />
        <F32KeyframeList
          label={t("posYKeyframes")}
          keyframes={clip.pos_y_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={clip.pos_y}
          onCommit={(posYKeyframes) => {
            return commit({ posYKeyframes });
          }}
        />
        <F32KeyframeList
          label={t("scaleKeyframes")}
          keyframes={clip.scale_keyframes}
          min={1}
          max={3}
          step={0.01}
          defaultValue={1}
          onCommit={(scaleKeyframes) => {
            return commit({ scaleKeyframes });
          }}
        />
        <F32KeyframeList
          label={t("rotationKeyframes")}
          keyframes={clip.rotation_keyframes}
          min={-180}
          max={180}
          step={1}
          defaultValue={0}
          onCommit={(rotationKeyframes) => {
            return commit({ rotationKeyframes });
          }}
        />
        <F32KeyframeList
          label={t("opacityKeyframes")}
          keyframes={clip.opacity_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(opacityKeyframes) => {
            return commit({ opacityKeyframes });
          }}
        />
      </Section>

      {clip.words.length > 0 && (
        <Section
          label={t("textHighlight")}
          defaultOpen={clip.highlight_enabled}
          testId="section-text-highlight"
        >
          <label className="check">
            <input
              type="checkbox"
              checked={clip.highlight_enabled}
              onChange={(e) => {
                return commit({ highlightEnabled: e.target.checked });
              }}
            />
            {t("wordHighlight")}
          </label>
          <ColorField
            label={t("highlightColor")}
            rgb={clip.highlight_color_rgba}
            onCommit={([r, g, b]) => {
              return commit({
                highlightColorRgba: [r, g, b, clip.highlight_color_rgba[3]],
              });
            }}
          />
        </Section>
      )}
    </>
  );
}
