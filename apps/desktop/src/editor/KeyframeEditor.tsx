import { useEffect, useState } from "react";
import { t } from "../i18n";
import type { Keyframe, Position } from "../types";

/** Shared "whole list on change" shape every keyframe editor uses -- the caller clones the
 *  current list out, this component lets the user add/remove/edit rows locally, and calls
 *  `onCommit` with the edited list on add/remove immediately, or on drag release for a value
 *  edit (mirrors `Inspector.tsx`'s `Slider`: preview locally, commit once per gesture). */
function useKeyframeDraft<T>(keyframes: Keyframe<T>[], onCommit: (next: Keyframe<T>[]) => void) {
  const [draft, setDraft] = useState(keyframes);
  useEffect(() => setDraft(keyframes), [keyframes]);
  const commitNow = (next: Keyframe<T>[]) => {
    setDraft(next);
    onCommit(next);
  };
  const removeAt = (index: number) => commitNow(draft.filter((_, i) => i !== index));
  return { draft, setDraft, commitNow, removeAt };
}

export function F32KeyframeList(props: {
  label: string;
  keyframes: Keyframe<number>[];
  min: number;
  max: number;
  step: number;
  defaultValue: number;
  onCommit: (next: Keyframe<number>[]) => void;
}) {
  const { draft, setDraft, commitNow, removeAt } = useKeyframeDraft(props.keyframes, props.onCommit);
  const commitDraft = () => props.onCommit(draft);
  return (
    <div className="keyframe-list">
      <label>{props.label}</label>
      {draft.map((kf, index) => (
        <div className="keyframe-row" key={index}>
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={kf.time_fraction}
            title={t("keyframeTime")}
            onChange={(e) => {
              const next = [...draft];
              next[index] = { ...kf, time_fraction: Number(e.target.value) };
              setDraft(next);
            }}
            onPointerUp={commitDraft}
            onBlur={commitDraft}
          />
          <input
            type="range"
            min={props.min}
            max={props.max}
            step={props.step}
            value={kf.value}
            onChange={(e) => {
              const next = [...draft];
              next[index] = { ...kf, value: Number(e.target.value) };
              setDraft(next);
            }}
            onPointerUp={commitDraft}
            onBlur={commitDraft}
          />
          <button className="icon" onClick={() => removeAt(index)}>
            🗑
          </button>
        </div>
      ))}
      <button
        className="icon"
        onClick={() => commitNow([...draft, { time_fraction: 1, value: props.defaultValue }])}
      >
        {t("addKeyframe")}
      </button>
    </div>
  );
}

export function PositionKeyframeList(props: {
  label: string;
  keyframes: Keyframe<Position>[];
  onCommit: (next: Keyframe<Position>[]) => void;
}) {
  const { draft, setDraft, commitNow, removeAt } = useKeyframeDraft(props.keyframes, props.onCommit);
  const commitDraft = () => props.onCommit(draft);
  return (
    <div className="keyframe-list">
      <label>{props.label}</label>
      {draft.map((kf, index) => (
        <div className="keyframe-row" key={index}>
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={kf.time_fraction}
            title={t("keyframeTime")}
            onChange={(e) => {
              const next = [...draft];
              next[index] = { ...kf, time_fraction: Number(e.target.value) };
              setDraft(next);
            }}
            onPointerUp={commitDraft}
            onBlur={commitDraft}
          />
          <input
            type="range"
            min={-1}
            max={1}
            step={0.01}
            value={kf.value.x}
            title={t("keyframeX")}
            onChange={(e) => {
              const next = [...draft];
              next[index] = { ...kf, value: { ...kf.value, x: Number(e.target.value) } };
              setDraft(next);
            }}
            onPointerUp={commitDraft}
            onBlur={commitDraft}
          />
          <input
            type="range"
            min={-1}
            max={1}
            step={0.01}
            value={kf.value.y}
            title={t("keyframeY")}
            onChange={(e) => {
              const next = [...draft];
              next[index] = { ...kf, value: { ...kf.value, y: Number(e.target.value) } };
              setDraft(next);
            }}
            onPointerUp={commitDraft}
            onBlur={commitDraft}
          />
          <button className="icon" onClick={() => removeAt(index)}>
            🗑
          </button>
        </div>
      ))}
      <button
        className="icon"
        onClick={() => commitNow([...draft, { time_fraction: 1, value: { x: 0, y: 0 } }])}
      >
        {t("addKeyframe")}
      </button>
    </div>
  );
}
