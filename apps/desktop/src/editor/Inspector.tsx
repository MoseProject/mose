import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { run, uiStore, useStore } from "../store";
import { F32KeyframeList, PositionKeyframeList } from "./KeyframeEditor";
import { ShapeInspector } from "./ShapeInspector";
import { TextInspector } from "./TextInspector";
import {
  activeTimeline,
  BLEND_MODES,
  type BlendMode,
  type ClipInstance,
  type ClipPatch,
  type MaskShape,
  type SessionView,
  type TransitionType,
} from "../types";

const MASK_SHAPES: { value: MaskShape; label: () => string }[] = [
  { value: "None", label: () => t("maskNone") },
  { value: "Circle", label: () => t("maskCircle") },
  { value: "RoundedRect", label: () => t("maskRoundedRect") },
];

const TRANSITION_TYPES: { value: TransitionType; label: () => string }[] = [
  { value: "None", label: () => t("transitionNone") },
  { value: "Fade", label: () => t("transitionFade") },
  { value: "HardCut", label: () => t("transitionHardCut") },
  { value: "Slide", label: () => t("transitionSlide") },
  { value: "Zoom", label: () => t("transitionZoom") },
];

export function Inspector({ session }: { session: SessionView }) {
  const { selectedClipIds } = useStore(uiStore);
  const timeline = activeTimeline(session.project);
  const clip =
    selectedClipIds.length === 1
      ? timeline.tracks.flatMap((track) => track.clips).find((c) => c.id === selectedClipIds[0])
      : undefined;
  const asset = clip && session.project.media_library.find((a) => a.id === clip.asset_id);
  const textClip = selectedClipIds.length === 1
    ? timeline.tracks.flatMap((track) => track.text_clips).find((item) => item.id === selectedClipIds[0])
    : undefined;
  const shapeClip = selectedClipIds.length === 1
    ? timeline.tracks.flatMap((track) => track.shape_clips).find((item) => item.id === selectedClipIds[0])
    : undefined;

  return (
    <section className="panel inspector">
      <div className="panel-title">{t("inspector")}</div>
      <div className="inspector-body">
        {clip ? (
          <div data-testid="inspector-clip">
            <div className="muted" title={asset?.source_path}>
              {t("source")}: {asset?.file_name ?? "—"}
            </div>
            <ClipControls key={clip.id} clip={clip} />
          </div>
        ) : textClip ? (
          <TextInspector key={textClip.id} clip={textClip} />
        ) : shapeClip ? (
          <ShapeInspector key={shapeClip.id} clip={shapeClip} />
        ) : (
          <div className="empty-state">{t("noClipSelected")}</div>
        )}
      </div>
    </section>
  );
}
function ClipControls({ clip }: { clip: ClipInstance }) {
  const commit = (patch: ClipPatch) => run(api.updateClip(clip.id, patch));
  const [region, setRegion] = useState({ centerX: 0.5, centerY: 0.5, width: 0.3, height: 0.3 });
  return (
    <>
      <Slider
        label={t("gain")}
        value={clip.gain_db}
        min={-24}
        max={24}
        step={0.5}
        format={(v) => `${v > 0 ? "+" : ""}${v.toFixed(1)} dB`}
        onCommit={(v) => commit({ gainDb: v })}
      />
      <Slider
        label={t("speed")}
        value={clip.speed_factor}
        min={0.25}
        max={4}
        step={0.05}
        format={(v) => `${v.toFixed(2)}×`}
        onCommit={(v) => commit({ speedFactor: v })}
      />
      <div className="field">
        <span>{t("speedRamp")}</span>
        <div className="export-actions">
          <button
            className="icon"
            onClick={() => run(api.applySpeedRampToClip(clip.id, 0.5, 2.0, 4))}
          >
            {t("speedRampSlowToFast")}
          </button>
          <button
            className="icon"
            onClick={() => run(api.applySpeedRampToClip(clip.id, 2.0, 0.5, 4))}
          >
            {t("speedRampFastToSlow")}
          </button>
        </div>
      </div>
      <Slider
        label={t("brightness")}
        value={clip.brightness}
        min={-1}
        max={1}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ brightness: v })}
      />
      <Slider
        label={t("contrast")}
        value={clip.contrast}
        min={0}
        max={2}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ contrast: v })}
      />
      <Slider
        label={t("saturation")}
        value={clip.saturation}
        min={0}
        max={2}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ saturation: v })}
      />
      <label className="check">
        <input
          type="checkbox"
          checked={clip.frozen}
          onChange={(e) => commit({ frozen: e.target.checked })}
        />
        {t("freeze")}
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={clip.flipped_h}
          onChange={(e) => commit({ flippedH: e.target.checked })}
        />
        {t("flip")}
      </label>
      <label className="check">
        <input
          type="checkbox"
          checked={clip.voice_cleanup_enabled}
          onChange={(e) => commit({ voiceCleanupEnabled: e.target.checked })}
        />
        {t("voiceCleanup")}
      </label>

      <div className="field">
        <label>{t("crop")}</label>
      </div>
      <Slider
        label="x"
        value={clip.crop_x}
        min={0}
        max={1}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ cropX: v })}
      />
      <Slider
        label="y"
        value={clip.crop_y}
        min={0}
        max={1}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ cropY: v })}
      />
      <Slider
        label="w"
        value={clip.crop_w}
        min={0.05}
        max={1}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ cropW: v })}
      />
      <Slider
        label="h"
        value={clip.crop_h}
        min={0.05}
        max={1}
        step={0.01}
        format={(v) => v.toFixed(2)}
        onCommit={(v) => commit({ cropH: v })}
      />
      <button className="icon" onClick={() => commit({ cropX: 0, cropY: 0, cropW: 1, cropH: 1 })}>
        {t("cropReset")}
      </button>

      <div className="field">
        <label>{t("mask")}</label>
        <select
          value={clip.mask_shape}
          onChange={(e) => commit({ maskShape: e.target.value as MaskShape })}
        >
          {MASK_SHAPES.map(({ value, label }) => (
            <option key={value} value={value}>
              {label()}
            </option>
          ))}
        </select>
      </div>
      {clip.mask_shape === "RoundedRect" && (
        <Slider
          label={t("maskCornerRadius")}
          value={clip.mask_corner_radius}
          min={0}
          max={1}
          step={0.01}
          format={(v) => v.toFixed(2)}
          onCommit={(v) => commit({ maskCornerRadius: v })}
        />
      )}

      <div className="field">
        <label>{t("transition")}</label>
        <select
          value={clip.transition_in}
          onChange={(e) => commit({ transitionIn: e.target.value as TransitionType })}
        >
          {TRANSITION_TYPES.map(({ value, label }) => (
            <option key={value} value={value}>
              {label()}
            </option>
          ))}
        </select>
      </div>
      {clip.transition_in !== "None" && (
        <Slider
          label={t("transitionDuration")}
          value={clip.transition_duration_secs}
          min={0.1}
          max={3}
          step={0.1}
          format={(v) => `${v.toFixed(1)}s`}
          onCommit={(v) => commit({ transitionDurationSecs: v })}
        />
      )}

      <div className="field">
        <label>{t("blendMode")}</label>
        <select
          value={clip.blend_mode}
          onChange={(e) => commit({ blendMode: e.target.value as BlendMode })}
        >
          {BLEND_MODES.map((mode) => (
            <option key={mode} value={mode}>
              {mode}
            </option>
          ))}
        </select>
      </div>

      <PositionKeyframeList
        label={t("positionKeyframes")}
        keyframes={clip.position_keyframes}
        onCommit={(next) => commit({ positionKeyframes: next })}
      />
      <F32KeyframeList
        label={t("scaleKeyframes")}
        keyframes={clip.scale_keyframes}
        min={1}
        max={3}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ scaleKeyframes: next })}
      />
      <F32KeyframeList
        label={t("rotationKeyframes")}
        keyframes={clip.rotation_keyframes}
        min={-180}
        max={180}
        step={1}
        defaultValue={0}
        onCommit={(next) => commit({ rotationKeyframes: next })}
      />
      <F32KeyframeList
        label={t("opacityKeyframes")}
        keyframes={clip.opacity_keyframes}
        min={0}
        max={1}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ opacityKeyframes: next })}
      />
      <F32KeyframeList
        label={t("gainKeyframes")}
        keyframes={clip.gain_keyframes}
        min={-24}
        max={24}
        step={0.5}
        defaultValue={0}
        onCommit={(next) => commit({ gainKeyframes: next })}
      />
      <F32KeyframeList
        label={t("brightnessKeyframes")}
        keyframes={clip.brightness_keyframes}
        min={-1}
        max={1}
        step={0.01}
        defaultValue={0}
        onCommit={(next) => commit({ brightnessKeyframes: next })}
      />
      <F32KeyframeList
        label={t("contrastKeyframes")}
        keyframes={clip.contrast_keyframes}
        min={0}
        max={2}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ contrastKeyframes: next })}
      />
      <F32KeyframeList
        label={t("saturationKeyframes")}
        keyframes={clip.saturation_keyframes}
        min={0}
        max={2}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ saturationKeyframes: next })}
      />
      <F32KeyframeList
        label={t("cropXKeyframes")}
        keyframes={clip.crop_x_keyframes}
        min={0}
        max={1}
        step={0.01}
        defaultValue={0}
        onCommit={(next) => commit({ cropXKeyframes: next })}
      />
      <F32KeyframeList
        label={t("cropYKeyframes")}
        keyframes={clip.crop_y_keyframes}
        min={0}
        max={1}
        step={0.01}
        defaultValue={0}
        onCommit={(next) => commit({ cropYKeyframes: next })}
      />
      <F32KeyframeList
        label={t("cropWKeyframes")}
        keyframes={clip.crop_w_keyframes}
        min={0.05}
        max={1}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ cropWKeyframes: next })}
      />
      <F32KeyframeList
        label={t("cropHKeyframes")}
        keyframes={clip.crop_h_keyframes}
        min={0.05}
        max={1}
        step={0.01}
        defaultValue={1}
        onCommit={(next) => commit({ cropHKeyframes: next })}
      />

      <div className="field">
        <label>{t("aiActions")}</label>
      </div>
      <div className="modal-row">
        <input
          type="number"
          step={0.05}
          min={0}
          max={1}
          value={region.centerX}
          title={t("centerX")}
          onChange={(e) => setRegion({ ...region, centerX: Number(e.target.value) })}
        />
        <input
          type="number"
          step={0.05}
          min={0}
          max={1}
          value={region.centerY}
          title={t("centerY")}
          onChange={(e) => setRegion({ ...region, centerY: Number(e.target.value) })}
        />
        <input
          type="number"
          step={0.05}
          min={0.05}
          max={0.6}
          value={region.width}
          title={t("regionWidth")}
          onChange={(e) => setRegion({ ...region, width: Number(e.target.value) })}
        />
        <input
          type="number"
          step={0.05}
          min={0.05}
          max={0.6}
          value={region.height}
          title={t("regionHeight")}
          onChange={(e) => setRegion({ ...region, height: Number(e.target.value) })}
        />
      </div>
      <button
        className="icon"
        onClick={() =>
          run(
            api.motionTrackClip(
              clip.id,
              region.centerX,
              region.centerY,
              region.width,
              region.height,
              0.08,
            ),
          )
        }
      >
        {t("trackMotion")}
      </button>
      <button className="icon" onClick={() => run(api.autoReframeClip(clip.id))}>
        {t("autoReframe")}
      </button>
      <button className="icon" onClick={() => run(api.dynamicAutoReframeClip(clip.id))}>
        {t("dynamicAutoReframe")}
      </button>
      <button className="icon" onClick={() => run(api.generateMatteForClip(clip.id))}>
        {t("generateMatte")}
      </button>
      <button
        className="icon"
        onClick={() =>
          run(
            api.applyPrivacyBlurToClip(
              clip.id,
              region.centerX,
              region.centerY,
              region.width,
              region.height,
            ),
          )
        }
      >
        {t("applyPrivacyBlur")}
      </button>
    </>
  );
}

/** A range input that previews locally while dragging and commits once on release. */
export function Slider(props: {
  label: string;
  value: number;
  min: number;
  max: number;
  step: number;
  format: (value: number) => string;
  onCommit: (value: number) => void;
}) {
  const [draft, setDraft] = useState(props.value);
  useEffect(() => setDraft(props.value), [props.value]);
  const commit = () => {
    if (draft !== props.value) props.onCommit(draft);
  };
  return (
    <div className="field">
      <label>{props.label}</label>
      <span className="mono">{props.format(draft)}</span>
      <input
        type="range"
        min={props.min}
        max={props.max}
        step={props.step}
        value={draft}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
    </div>
  );
}

