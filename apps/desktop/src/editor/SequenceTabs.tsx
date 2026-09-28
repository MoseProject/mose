import { useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import type { MarkerKind, SessionView } from "../types";

const markerKinds: MarkerKind[] = ["Standard", "ToDo", "Chapter", "Highlight"];

export function SequenceTabs({ session }: { session: SessionView }) {
  const { project } = session;
  const [renaming, setRenaming] = useState<number | null>(null);
  return <div className="sequence-tabs">
    <div className="tab-list" role="tablist" aria-label={t("sequences")}>
      {project.sequences.map((sequence, index) => <div key={sequence.id} className={`sequence-tab ${index === project.active_sequence ? "active" : ""}`}>
        {renaming === sequence.id ? <input autoFocus defaultValue={sequence.name} aria-label={t("renameSequence")}
          onBlur={(e) => { setRenaming(null); if (e.target.value !== sequence.name) void run(api.renameSequence(sequence.id, e.target.value)); }}
          onKeyDown={(e) => { if (e.key === "Enter") e.currentTarget.blur(); if (e.key === "Escape") setRenaming(null); }} />
          : <button role="tab" aria-selected={index === project.active_sequence} onClick={() => run(api.selectSequence(index))}>{sequence.name}</button>}
        <button className="icon" title={t("duplicateSequence")} onClick={() => run(api.duplicateSequence(index, `${sequence.name} ${t("copy")}`))}>+</button>
        <button className="icon" title={t("renameSequence")} onClick={() => setRenaming(sequence.id)}>R</button>
        <button className="icon" title={t("removeSequence")} disabled={project.sequences.length === 1} onClick={() => run(api.removeSequence(sequence.id))}>x</button>
        <button className="icon" title={t("moveSequenceLeft")} disabled={index === 0} onClick={() => run(api.moveSequence(index, index - 1))}>‹</button>
        <button className="icon" title={t("moveSequenceRight")} disabled={index === project.sequences.length - 1} onClick={() => run(api.moveSequence(index, index + 1))}>›</button>
      </div>)}
      <button className="icon" title={t("addSequence")} onClick={() => run(api.addSequence(`${t("sequence")} ${project.sequences.length + 1}`))}>+</button>
    </div>
    <MarkerList session={session} />
  </div>;
}

function MarkerList({ session }: { session: SessionView }) {
  const markers = session.project.sequences[session.project.active_sequence].timeline.markers;
  return <details className="marker-list"><summary>{t("markers")}</summary>
    <button onClick={() => run(api.addMarker())}>{t("addMarker")}</button>
    {markers.slice().sort((a, b) => a.position_secs - b.position_secs).map((marker) => <div key={marker.id} className="marker-row">
      <button className="icon" title={t("seekMarker")} onClick={() => run(api.seek(marker.position_secs))}>{formatTime(marker.position_secs)}</button>
      <input defaultValue={marker.label} aria-label={t("markerLabel")} onBlur={(e) => { if (e.target.value !== marker.label) void run(api.updateMarker(marker.id, { label: e.target.value })); }} />
      <select value={marker.kind} aria-label={t("markerKind")} onChange={(e) => run(api.updateMarker(marker.id, { kind: e.target.value as MarkerKind }))}>{markerKinds.map((kind) => <option key={kind}>{kind}</option>)}</select>
      {marker.kind === "ToDo" && <input type="checkbox" checked={marker.completed} aria-label={t("markerCompleted")} onChange={(e) => run(api.updateMarker(marker.id, { completed: e.target.checked }))} />}
      <button className="icon" title={t("removeMarker")} onClick={() => run(api.removeMarker(marker.id))}>x</button>
    </div>)}
  </details>;
}
