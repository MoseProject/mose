import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { run, uiStore, useStore } from "../store";
import { ShapeInspector } from "./ShapeInspector";
import { TextInspector } from "./TextInspector";
import {
  activeTimeline,
  type ClipInstance,
  type ClipPatch,
  type SessionView,
} from "../types";

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
          <>
            <div className="muted" title={asset?.source_path}>
              {t("source")}: {asset?.file_name ?? "—"}
            </div>
            <ClipControls key={clip.id} clip={clip} />
          </>
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

