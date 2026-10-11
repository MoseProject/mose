import libraryStyles from "../library/library.module.css";
import inspectorStyles from "../inspector/inspector.module.css";
import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./export.module.css";
import { useEffect } from "react";
import { formatTime, t } from "../../i18n";
import { api } from "../../ipc";
import { exportsStore, patchUi, run, useStore } from "../../store";
import { Icon } from "../chrome/Icon";
import type { ExportJobView } from "../../types";

const LOUDNESS = [-14, -16, -23];
const label = (job: ExportJobView) => {
  return typeof job.status === "string" ? job.status : Object.keys(job.status)[0];
};

export function ExportPanel() {
  const jobs = useStore(exportsStore);
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        patchUi({ exportQueueOpen: false });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      return window.removeEventListener("keydown", onKey);
    };
  }, []);
  return (
    <section
      className={cx(styles, baseStyles, libraryStyles, inspectorStyles, "export-panel")}
      role="dialog"
      aria-label={t("exportQueue")}
    >
      <div className={cx(styles, baseStyles, "panel-title")}>
        <span>{t("exportQueue")}</span>
        <button
          className={cx(styles, baseStyles, "icon")}
          aria-label={t("close")}
          title={t("close")}
          onClick={() => {
            return patchUi({ exportQueueOpen: false });
          }}
        >
          <Icon name="close" />
        </button>
      </div>
      <div className={cx(styles, baseStyles, inspectorStyles, "export-body")}>
        {jobs.filter((job) => {
          return label(job) === "Queued";
        }).length >= 2 && (
          <div className={cx(styles, baseStyles, inspectorStyles, "export-actions")}>
            {LOUDNESS.map((target) => {
              return (
                <button
                  key={target}
                  onClick={() => {
                    return run(api.matchExportLoudness(target));
                  }}
                >
                  {target} LUFS
                </button>
              );
            })}
          </div>
        )}
        {jobs.length === 0 ? (
          <div className={cx(styles, baseStyles, libraryStyles, "empty-state")}>
            {t("noExports")}
          </div>
        ) : (
          jobs.map((job) => {
            return (
              <Job
                key={job.id}
                job={job}
              />
            );
          })
        )}
        <div className={cx(styles, baseStyles, inspectorStyles, "export-actions")}>
          <button
            onClick={() => {
              return run(api.exportActiveSequenceSrt());
            }}
          >
            {t("exportSrt")}
          </button>
        </div>
      </div>
    </section>
  );
}
/** The encoders that run on a graphics chip rather than the processor. */
const HARDWARE_ENCODERS = ["nvenc", "qsv", "amf", "vaapi", "videotoolbox"];

/** What a finished job says about how it ran: the encoder (CPU or GPU), how long it took and how
 *  many times faster than the video's own length. Null until the job has a report. */
export function exportSummary(job: ExportJobView): string | null {
  const report = job.report;
  if (!report) {
    return null;
  }
  const hardware = report.encoder
    ? HARDWARE_ENCODERS.some((name) => {
        return report.encoder!.includes(name);
      })
    : false;
  const parts = [
    report.encoder
      ? `${report.encoder} (${hardware ? t("encoderGpu") : t("encoderCpu")})`
      : t("encoderUnknown"),
    formatTime(report.tookSecs),
  ];
  if (report.tookSecs > 0 && report.videoSecs > 0) {
    parts.push(`${(report.videoSecs / report.tookSecs).toFixed(1)}x`);
  }
  return parts.join(" · ");
}

/** Seconds left, extrapolated from progress so far; null until the estimate is steady. */
export function remainingSecs(job: ExportJobView): number | null {
  const elapsed = job.elapsedSecs ?? 0;
  if (job.percent < 3 || job.percent >= 100 || elapsed <= 0) {
    return null;
  }
  return (elapsed * (100 - job.percent)) / job.percent;
}

function Job({ job }: { job: ExportJobView }) {
  const current = label(job);
  const remaining = current === "Rendering" ? remainingSecs(job) : null;
  const summary = exportSummary(job);
  return (
    <div
      className={cx(styles, baseStyles, inspectorStyles, "job")}
      title={job.outputPath}
    >
      <span>{job.title}</span>
      <span className={cx(styles, baseStyles, "muted")}>
        {current} {job.percent > 0 && `${job.percent}%`}
      </span>
      {current === "Done" && summary && (
        <span
          className={cx(styles, baseStyles, "muted")}
          data-testid="export-summary"
        >
          {summary}
        </span>
      )}
      {(current === "Rendering" || current === "Paused") && (
        <>
          <progress
            max={100}
            value={job.percent}
          />
          <span className={cx(styles, baseStyles, "muted")}>
            {formatTime(job.elapsedSecs ?? 0)} {t("elapsed")}
            {remaining !== null && " · " + formatTime(remaining) + " " + t("etaRemaining")}
          </span>
        </>
      )}
      <div className={cx(styles, baseStyles, inspectorStyles, "export-actions")}>
        {current === "Rendering" && (
          <>
            <button
              onClick={() => {
                return run(api.pauseExport(job.id));
              }}
            >
              {t("pauseExport")}
            </button>
            <button
              onClick={() => {
                return run(api.cancelExport(job.id));
              }}
            >
              {t("cancel")}
            </button>
          </>
        )}
        {current === "Paused" && (
          <>
            <button
              onClick={() => {
                return run(api.resumeExport(job.id));
              }}
            >
              {t("resumeExport")}
            </button>
            <button
              onClick={() => {
                return run(api.cancelExport(job.id));
              }}
            >
              {t("cancel")}
            </button>
          </>
        )}
        {current === "Done" && (
          <button
            onClick={() => {
              void run(api.revealInFolder(job.outputPath));
            }}
          >
            {t("openExportFolder")}
          </button>
        )}
        {(current === "Queued" || current === "Done" || current === "Failed") && (
          <button
            onClick={() => {
              return run(api.removeExport(job.id));
            }}
          >
            {t("removeExport")}
          </button>
        )}
      </div>
    </div>
  );
}
