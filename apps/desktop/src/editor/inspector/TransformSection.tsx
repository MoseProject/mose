import { t } from "../../i18n";
import { hasKeyframeAt, valueAt, withKeyframe, withoutKeyframe } from "../../keyframeAt";
import { type ClipInstance, type ClipPatch } from "../../types";
import { NumberField, Section, type SectionKeyframe } from "./controls";

/** Position, scale and rotation as plain numbers. Position is the layer's top-left corner in
 *  pixels of the output canvas, stored as a fraction of it as the on-canvas handle moves it; an animated value (several keyframes)
 *  is edited in the Keyframes section instead. */
export function TransformSection({
  clip,
  commit,
  canvas,
  fraction,
}: {
  clip: ClipInstance;
  commit: (patch: ClipPatch) => Promise<unknown>;
  /** Output canvas size in pixels; positions are stored as fractions of it. */
  canvas: [number, number];
  /** The playhead inside the clip (0..1), null outside it. */
  fraction: number | null;
}) {
  const canvasW = Math.max(1, canvas[0]);
  const canvasH = Math.max(1, canvas[1]);
  const position = clip.position_keyframes[0]?.value ?? { x: 0, y: 0 };
  const rotation = clip.rotation_keyframes[0]?.value ?? 0;
  const positionAnimated = clip.position_keyframes.length > 1;
  const rotationAnimated = clip.rotation_keyframes.length > 1;
  const diamond: SectionKeyframe = {
    active:
      fraction !== null &&
      (hasKeyframeAt(clip.position_keyframes, fraction) ||
        hasKeyframeAt(clip.rotation_keyframes, fraction)),
    disabled: fraction === null,
    onToggle: () => {
      if (fraction === null) {
        return;
      }
      const remove =
        hasKeyframeAt(clip.position_keyframes, fraction) ||
        hasKeyframeAt(clip.rotation_keyframes, fraction);
      const xs = clip.position_keyframes.map((keyframe) => {
        return { time_fraction: keyframe.time_fraction, value: keyframe.value.x };
      });
      const ys = clip.position_keyframes.map((keyframe) => {
        return { time_fraction: keyframe.time_fraction, value: keyframe.value.y };
      });
      const here = {
        x: valueAt(xs, fraction, 0),
        y: valueAt(ys, fraction, 0),
      };
      const angle = valueAt(clip.rotation_keyframes, fraction, 0);
      void commit({
        positionKeyframes: remove
          ? withoutKeyframe(clip.position_keyframes, fraction)
          : withKeyframe(clip.position_keyframes, fraction, here),
        rotationKeyframes: remove
          ? withoutKeyframe(clip.rotation_keyframes, fraction)
          : withKeyframe(clip.rotation_keyframes, fraction, angle),
      });
    },
  };
  const setPosition = (x: number, y: number) => {
    // The monitor drag keeps a layer within one canvas of the frame; typed values get the same
    // bound so a stray digit cannot throw the layer out of reach.
    const bound = (value: number) => {
      return Math.min(2, Math.max(-1, value));
    };
    void commit({ positionKeyframes: [{ time_fraction: 0, value: { x: bound(x), y: bound(y) } }] });
  };
  return (
    <Section
      label={t("transform")}
      defaultOpen
      testId="section-transform"
      keyframe={diamond}
      onReset={() => {
        void commit({
          positionKeyframes: [],
          layerScaleX: 1,
          layerScaleY: 1,
          rotationKeyframes: [],
        });
      }}
    >
      <div className="field-row">
        <NumberField
          label={t("positionX")}
          value={Math.round(position.x * canvasW)}
          step={1}
          suffix="px"
          disabled={positionAnimated}
          onCommit={(value) => {
            setPosition(value / canvasW, position.y);
          }}
        />
        <NumberField
          label={t("positionY")}
          value={Math.round(position.y * canvasH)}
          step={1}
          suffix="px"
          disabled={positionAnimated}
          onCommit={(value) => {
            setPosition(position.x, value / canvasH);
          }}
        />
      </div>
      <NumberField
        label={t("scale")}
        value={clip.layer_scale_x * 100}
        step={1}
        suffix="%"
        onCommit={(value) => {
          const scale = Math.max(1, value) / 100;
          void commit({ layerScaleX: scale, layerScaleY: scale });
        }}
      />
      <NumberField
        label={t("rotation")}
        value={rotation}
        step={1}
        suffix="°"
        disabled={rotationAnimated}
        onCommit={(value) => {
          void commit({ rotationKeyframes: [{ time_fraction: 0, value }] });
        }}
      />
      <p className="muted">
        {positionAnimated || rotationAnimated ? t("transformAnimated") : t("transformHint")}
      </p>
    </Section>
  );
}
