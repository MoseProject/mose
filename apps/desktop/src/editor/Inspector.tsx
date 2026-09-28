import { useEffect, useState } from "react";
import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { exportsStore, run, uiStore, useStore } from "../store";
import {
  activeTimeline,
  type ClipInstance,
  type ClipPatch,
  type ExportAspectRatio,
  type ExportStatus,
  type SessionView,
} from "../types";

/** Same presets as `ui::app::LUFS_PROFILES`. */
const LUFS_PROFILES: [string, number][] = [
  ["-14 LUFS | YouTube", -14],
  ["-16 LUFS | Podcast", -16],
  ["-23 LUFS | Broadcast", -23],
];

const ASPECTS: [ExportAspectRatio, TextKey][] = [
  ["Original", "aspectOriginal"],
  ["Landscape", "aspectLandscape"],
  ["Portrait", "aspectPortrait"],
  ["Square", "aspectSquare"],
];

const STATUS_TEXT: Record<ExportStatus, TextKey> = {
  rendering: "statusRendering",
  done: "statusDone",
  failed: "statusFailed",
  cancelled: "statusCancelled",
};

export function Inspector({ session }: { session: SessionView }) {
  const { selectedClipIds } = useStore(uiStore);
  const timeline = activeTimeline(session.project);
  const clip =
    selectedClipIds.length === 1
      ? timeline.tracks.flatMap((track) => track.clips).find((c) => c.id === selectedClipIds[0])
      : undefined;
  const asset = clip && session.project.media_library.find((a) => a.id === clip.asset_id);

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
        ) : (
          <div className="empty-state">{t("noClipSelected")}</div>
        )}
        <ExportSection session={session} />
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
function Slider(props: {
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

function ExportSection({ session }: { session: SessionView }) {
  const jobs = useStore(exportsStore);
  const sequence = session.project.sequences[session.project.active_sequence];
  const settings = sequence.export_settings;

  return (
    <>
      <div className="panel-title" style={{ padding: "8px 0" }}>
        {t("export")}
      </div>
      <div className="field">
        <label>{t("exportTarget")}</label>
        <select
          value={settings.target_lufs}
          onChange={(e) =>
            run(api.setExportSettings(Number(e.target.value), settings.aspect_ratio))
          }
        >
          {LUFS_PROFILES.map(([label, lufs]) => (
            <option key={lufs} value={lufs}>
              {label}
            </option>
          ))}
        </select>
      </div>
      <div className="field">
        <label>{t("aspectRatio")}</label>
        <select
          value={settings.aspect_ratio}
          onChange={(e) =>
            run(
              api.setExportSettings(settings.target_lufs, e.target.value as ExportAspectRatio),
            )
          }
        >
          {ASPECTS.map(([value, key]) => (
            <option key={value} value={value}>
              {t(key)}
            </option>
          ))}
        </select>
      </div>
      <div className="panel-title" style={{ padding: "8px 0" }}>
        {t("exportQueue")}
      </div>
      <div className="jobs">
        {jobs.length === 0 && <span className="muted">{t("noExports")}</span>}
        {jobs.map((job) => (
          <div className="job" key={job.id} title={job.message ?? job.outputPath}>
            <span>{job.title}</span>
            <span className="muted">
              {t(STATUS_TEXT[job.status])}
              {job.status === "rendering" && ` ${job.percent}%`}
              {job.status === "rendering" && (
                <button className="icon" onClick={() => run(api.cancelExport(job.id))}>
                  {t("cancel")}
                </button>
              )}
            </span>
            <progress max={100} value={job.percent} />
          </div>
        ))}
      </div>
    </>
  );
}
