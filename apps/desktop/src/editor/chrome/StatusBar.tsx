import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./chrome.module.css";
import { t } from "../../i18n";
import {
  exportsStore,
  patchUi,
  sessionStore,
  transcribingAssetIdStore,
  useStore,
} from "../../store";
import type { ExportJobView } from "../../types";

interface Task {
  key: string;
  label: string;
  /** 0-100 when known; otherwise the bar is indeterminate. */
  percent: number | null;
}

function exportPercent(job: ExportJobView): number | null {
  if (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status)) {
    return job.percent;
  }
  return null;
}

function isActive(job: ExportJobView): boolean {
  return (
    job.status === "Queued" ||
    (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))
  );
}

/** Background work in one place: media analysis after an import, transcription and exports. A
 *  slim bar along the bottom that only exists while something is running. */
export function StatusBar() {
  const session = useStore(sessionStore);
  const exportJobs = useStore(exportsStore);
  const transcribing = useStore(transcribingAssetIdStore);

  const tasks: Task[] = [];
  const analyzing =
    session?.project.media_library.filter((asset) => {
      return asset.loudness === null;
    }) ?? [];
  for (const asset of analyzing) {
    tasks.push({
      key: "analyze-" + asset.id,
      label: t("taskAnalyzing") + " " + asset.file_name,
      percent: null,
    });
  }
  if (transcribing !== null) {
    const name = session?.project.media_library.find((asset) => {
      return asset.id === transcribing;
    })?.file_name;
    tasks.push({
      key: "transcribe",
      label: t("transcribing") + (name ? " " + name : ""),
      percent: null,
    });
  }
  for (const job of exportJobs.filter(isActive)) {
    tasks.push({
      key: "export-" + job.id,
      label: t("export") + " " + job.title,
      percent: exportPercent(job),
    });
  }

  if (tasks.length === 0) {
    return null;
  }
  return (
    <div
      className={cx(styles, baseStyles, "status-bar")}
      role="status"
      aria-label={t("tasks")}
    >
      {tasks.slice(0, 4).map((task) => {
        return (
          <button
            key={task.key}
            className={cx(styles, baseStyles, "task")}
            onClick={() => {
              if (task.key.startsWith("export-")) {
                patchUi({ exportQueueOpen: true });
              }
            }}
          >
            <span
              className={cx(
                styles,
                baseStyles,
                "task-bar" + (task.percent === null ? " indeterminate" : ""),
              )}
              aria-hidden="true"
            >
              <span style={task.percent === null ? undefined : { width: task.percent + "%" }} />
            </span>
            <span className={cx(styles, baseStyles, "task-label")}>
              {task.label}
              {task.percent !== null ? " " + Math.round(task.percent) + "%" : ""}
            </span>
          </button>
        );
      })}
      {tasks.length > 4 && (
        <span className={cx(styles, baseStyles, "muted")}>+{tasks.length - 4}</span>
      )}
    </div>
  );
}
