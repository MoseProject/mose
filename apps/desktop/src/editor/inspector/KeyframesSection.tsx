import { t } from "../../i18n";
import { F32KeyframeList, PositionKeyframeList } from "./KeyframeEditor";
import { type ClipInstance, type ClipPatch } from "../../types";
import { Section } from "./controls";

type Commit = (patch: ClipPatch) => Promise<unknown>;

export function KeyframesSection({ clip, commit }: { clip: ClipInstance; commit: Commit }) {
  return (
    <>
      <Section
        label={t("keyframes")}
        defaultOpen={
          clip.position_keyframes.length > 0 ||
          clip.scale_keyframes.length > 0 ||
          clip.rotation_keyframes.length > 0 ||
          clip.opacity_keyframes.length > 0 ||
          clip.gain_keyframes.length > 0 ||
          clip.brightness_keyframes.length > 0 ||
          clip.contrast_keyframes.length > 0 ||
          clip.saturation_keyframes.length > 0 ||
          clip.crop_x_keyframes.length > 0 ||
          clip.crop_y_keyframes.length > 0 ||
          clip.crop_w_keyframes.length > 0 ||
          clip.crop_h_keyframes.length > 0
        }
      >
        <PositionKeyframeList
          label={t("positionKeyframes")}
          keyframes={clip.position_keyframes}
          onCommit={(next) => {
            return commit({ positionKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("scaleKeyframes")}
          keyframes={clip.scale_keyframes}
          min={1}
          max={3}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ scaleKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("rotationKeyframes")}
          keyframes={clip.rotation_keyframes}
          min={-180}
          max={180}
          step={1}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ rotationKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("opacityKeyframes")}
          keyframes={clip.opacity_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ opacityKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("gainKeyframes")}
          keyframes={clip.gain_keyframes}
          min={-24}
          max={24}
          step={0.5}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ gainKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("brightnessKeyframes")}
          keyframes={clip.brightness_keyframes}
          min={-1}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ brightnessKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("contrastKeyframes")}
          keyframes={clip.contrast_keyframes}
          min={0}
          max={2}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ contrastKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("saturationKeyframes")}
          keyframes={clip.saturation_keyframes}
          min={0}
          max={2}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ saturationKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropXKeyframes")}
          keyframes={clip.crop_x_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ cropXKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropYKeyframes")}
          keyframes={clip.crop_y_keyframes}
          min={0}
          max={1}
          step={0.01}
          defaultValue={0}
          onCommit={(next) => {
            return commit({ cropYKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropWKeyframes")}
          keyframes={clip.crop_w_keyframes}
          min={0.05}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ cropWKeyframes: next });
          }}
        />
        <F32KeyframeList
          label={t("cropHKeyframes")}
          keyframes={clip.crop_h_keyframes}
          min={0.05}
          max={1}
          step={0.01}
          defaultValue={1}
          onCommit={(next) => {
            return commit({ cropHKeyframes: next });
          }}
        />
      </Section>
    </>
  );
}
