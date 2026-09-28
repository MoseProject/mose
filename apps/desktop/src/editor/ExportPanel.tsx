import { useEffect, useState } from "react";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { exportsStore, run, useStore } from "../store";
import type { ExportJobView } from "../types";

const LOUDNESS = [-14, -16, -23];
const label = (job: ExportJobView) => typeof job.status === "string" ? job.status : Object.keys(job.status)[0];

export function ExportPanel() {
  const jobs = useStore(exportsStore);
  const [gpu, setGpu] = useState("auto");
  useEffect(() => { void run(api.getGpuEncoderPreference()).then((value) => value && setGpu(value)); }, []);
  return <section className="export-panel">
    <div className="panel-title">{t("exportQueue")}</div>
    <div className="export-body">
      <div className="export-actions"><button className="primary" onClick={() => run(api.exportActiveSequence())}>{t("export")}</button><button onClick={() => run(api.exportActiveSequenceSrt())}>{t("exportSrt")}</button></div>
      <label className="field"><span>{t("gpuEncoder")}</span><select value={gpu} onChange={(e) => { setGpu(e.target.value); void run(api.setGpuEncoderPreference(e.target.value)); }}><option value="auto">Auto</option><option value="cpu">CPU</option><option value="nvenc">NVENC</option><option value="quick_sync">Quick Sync</option><option value="amf">AMF</option><option value="vaapi">VAAPI</option><option value="video_toolbox">VideoToolbox</option></select></label>
      {jobs.filter((job) => label(job) === "Queued").length >= 2 && <div className="export-actions">{LOUDNESS.map((target) => <button key={target} onClick={() => run(api.matchExportLoudness(target))}>{target} LUFS</button>)}</div>}
      {jobs.length === 0 ? <div className="empty-state">{t("noExports")}</div> : jobs.map((job) => <Job key={job.id} job={job} />)}
    </div>
  </section>;
}
function Job({ job }: { job: ExportJobView }) {
  const current = label(job);
  return <div className="job" title={job.outputPath}><span>{job.title}</span><span className="muted">{current} {job.percent > 0 && `${job.percent}%`}</span>{(current === "Rendering" || current === "Paused") && <><progress max={100} value={job.percent} /><span className="muted">{formatTime(job.elapsedSecs ?? 0)} {t("elapsed")}</span></>}<div className="export-actions">{current === "Rendering" && <><button onClick={() => run(api.pauseExport(job.id))}>{t("pauseExport")}</button><button onClick={() => run(api.cancelExport(job.id))}>{t("cancel")}</button></>}{current === "Paused" && <><button onClick={() => run(api.resumeExport(job.id))}>{t("resumeExport")}</button><button onClick={() => run(api.cancelExport(job.id))}>{t("cancel")}</button></>}{(current === "Queued" || current === "Done" || current === "Failed") && <button onClick={() => run(api.removeExport(job.id))}>{t("removeExport")}</button>}</div></div>;
}
