import { useEffect } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { exportsStore, patchUi, run, useStore } from "../store";
import { Icon } from "./Icon";
import type { ExportJobView } from "../types";

const LOUDNESS = [-14, -16, -23];
const label = (job: ExportJobView) => typeof job.status === "string" ? job.status : Object.keys(job.status)[0];

export function ExportPanel() {
  const jobs = useStore(exportsStore);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") patchUi({ exportQueueOpen: false });
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  return <section className="export-panel" role="dialog" aria-label={t("exportQueue")}>
    <div className="panel-title">
      <span>{t("exportQueue")}</span>
      <button className="icon" aria-label={t("close")} title={t("close")} onClick={() => patchUi({ exportQueueOpen: false })}><Icon name="close" /></button>
    </div>
    <div className="export-body">
      {jobs.filter((job) => label(job) === "Queued").length >= 2 && <div className="export-actions">{LOUDNESS.map((target) => <button key={target} onClick={() => run(api.matchExportLoudness(target))}>{target} LUFS</button>)}</div>}
      {jobs.length === 0 ? <div className="empty-state">{t("noExports")}</div> : jobs.map((job) => <Job key={job.id} job={job} />)}
      <div className="export-actions"><button onClick={() => run(api.exportActiveSequenceSrt())}>{t("exportSrt")}</button></div>
    </div>
  </section>;
}
function Job({ job }: { job: ExportJobView }) {
  const current = label(job);
  return <div className="job" title={job.outputPath}><span>{job.title}</span><span className="muted">{current} {job.percent > 0 && `${job.percent}%`}</span>{(current === "Rendering" || current === "Paused") && <><progress max={100} value={job.percent} /><span className="muted">{formatTime(job.elapsedSecs ?? 0)} {t("elapsed")}</span></>}<div className="export-actions">{current === "Rendering" && <><button onClick={() => run(api.pauseExport(job.id))}>{t("pauseExport")}</button><button onClick={() => run(api.cancelExport(job.id))}>{t("cancel")}</button></>}{current === "Paused" && <><button onClick={() => run(api.resumeExport(job.id))}>{t("resumeExport")}</button><button onClick={() => run(api.cancelExport(job.id))}>{t("cancel")}</button></>}{(current === "Queued" || current === "Done" || current === "Failed") && <button onClick={() => run(api.removeExport(job.id))}>{t("removeExport")}</button>}</div></div>;
}
