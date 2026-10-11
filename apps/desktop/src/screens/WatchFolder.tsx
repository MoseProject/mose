import homeStyles from "./home.module.css";
import settingsStyles from "./settings.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "../editor/chrome/chrome.module.css";
import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api, onWatchFolder } from "../ipc";
import { run } from "../store";
import { Dialog } from "../editor/chrome/Dialog";
import type { LoudnessMetrics, WatchFolderStatus } from "../types";

interface Row {
  path: string;
  status: WatchFolderStatus;
  percent: number;
  error: string | null;
  before: LoudnessMetrics | null;
  after: LoudnessMetrics | null;
  addedToProject: boolean;
}

/** CF-02: watch a folder for new gameplay recordings, normalize each once it stabilizes, and
 *  offer to import the cleaned-up result into the open project -- mirrors `ui::App`'s
 *  `watch_folder.rs` (Limpeza screen). */
export function WatchFolder({ open = true, onClose }: { open?: boolean; onClose?: () => void }) {
  const [watchPath, setWatchPath] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    const unlisten = onWatchFolder((event) => {
      setRows((prev) => {
        const index = prev.findIndex((r) => {
          return r.path === event.path;
        });
        const patch: Partial<Row> = (() => {
          switch (event.type) {
            case "detected":
              return { status: "stabilizing", percent: 0, error: null };
            case "stabilizing":
              return { status: "stabilizing" };
            case "processing":
              return { status: "processing", percent: 0 };
            case "progress":
              return { percent: event.percent };
            case "done":
              return {
                status: "done",
                percent: 100,
                before: event.before,
                after: event.after,
              };
            case "failed":
              return { status: "error", error: event.message };
          }
        })();
        if (index === -1) {
          return [
            {
              path: event.path,
              status: "stabilizing",
              percent: 0,
              error: null,
              before: null,
              after: null,
              addedToProject: false,
              ...patch,
            },
            ...prev,
          ];
        }
        const next = [...prev];
        next[index] = { ...next[index], ...patch };
        return next;
      });
    });
    return () => {
      void unlisten.then((stop) => {
        return stop();
      });
    };
  }, []);

  const choose = () => {
    return void run(api.pickWatchFolder()).then((path) => {
      return path && setWatchPath(path);
    });
  };
  const start = () => {
    if (!watchPath) {
      return;
    }
    setRunning(true);
    setRows([]);
    void run(api.startWatchingFolder(watchPath));
  };
  const stop = () => {
    setRunning(false);
    void run(api.stopWatchingFolder());
  };

  if (!open) {
    return null;
  }
  return (
    <Dialog
      title={t("watchFolder")}
      wide
      onClose={() => {
        onClose?.();
      }}
    >
      <div
        className={cx(styles, baseStyles, homeStyles, settingsStyles, "prefs")}
        data-testid="screen-watch"
      >
        <label>
          <span className={cx(styles, baseStyles, watchPath ? "" : "muted")}>
            {watchPath ?? t("watchFolderNone")}
          </span>
          <button
            onClick={choose}
            disabled={running}
          >
            {t("watchFolderChoose")}
          </button>
        </label>
        <button
          className={cx(styles, baseStyles, running ? "" : "primary")}
          disabled={!watchPath || running}
          onClick={start}
        >
          {t("watchFolderStart")}
        </button>
        {running && <button onClick={stop}>{t("watchFolderStop")}</button>}
        <div style={{ maxHeight: 400, overflowY: "auto" }}>
          {rows.length === 0 ? (
            <p className={cx(styles, baseStyles, "muted")}>{t("watchFolderEmpty")}</p>
          ) : (
            rows.map((row) => {
              return (
                <div
                  className={cx(styles, baseStyles, "modal-row")}
                  key={row.path}
                >
                  <div>
                    <div>{row.path.split(/[/\\]/).pop()}</div>
                    <div className={cx(styles, baseStyles, "muted")}>
                      {row.status === "stabilizing" && t("watchFolderStabilizing")}
                      {row.status === "processing" &&
                        `${t("watchFolderProcessing")} ${row.percent}%`}
                      {row.status === "done" &&
                        row.before &&
                        row.after &&
                        `${t("watchFolderDone")} · ${row.before.integrated_lufs.toFixed(1)} → ${row.after.integrated_lufs.toFixed(1)} LUFS`}
                      {row.status === "error" && `${t("watchFolderError")}: ${row.error}`}
                    </div>
                  </div>
                  {row.status === "done" && !row.addedToProject && watchPath && (
                    <button
                      className={cx(styles, baseStyles, "icon")}
                      onClick={() => {
                        void run(api.addWatchedFileToProject(watchPath, row.path));
                        setRows((prev) => {
                          return prev.map((r) => {
                            return r.path === row.path ? { ...r, addedToProject: true } : r;
                          });
                        });
                      }}
                    >
                      {t("watchFolderAddToProject")}
                    </button>
                  )}
                </div>
              );
            })
          )}
        </div>
      </div>
    </Dialog>
  );
}
