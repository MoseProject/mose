import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api, onWatchFolder } from "../ipc";
import { run } from "../store";
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
export function WatchFolder() {
  const [watchPath, setWatchPath] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [rows, setRows] = useState<Row[]>([]);

  useEffect(() => {
    const unlisten = onWatchFolder((event) => {
      setRows((prev) => {
        const index = prev.findIndex((r) => r.path === event.path);
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
              return { status: "done", percent: 100, before: event.before, after: event.after };
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
      void unlisten.then((stop) => stop());
    };
  }, []);

  const choose = () => void run(api.pickWatchFolder()).then((path) => path && setWatchPath(path));
  const start = () => {
    if (!watchPath) return;
    setRunning(true);
    setRows([]);
    void run(api.startWatchingFolder(watchPath));
  };
  const stop = () => {
    setRunning(false);
    void run(api.stopWatchingFolder());
  };

  return (
    <main className="prefs">
      <h1>{t("watchFolder")}</h1>
      <label>
        {watchPath ?? t("watchFolderChoose")}
        <button className="icon" onClick={choose} disabled={running}>
          {t("watchFolderChoose")}
        </button>
      </label>
      <button className={running ? "icon" : "icon primary"} disabled={!watchPath || running} onClick={start}>
        {t("watchFolderStart")}
      </button>
      {running && (
        <button className="icon" onClick={stop}>
          {t("watchFolderStop")}
        </button>
      )}
      <div style={{ maxHeight: 400, overflowY: "auto" }}>
        {rows.length === 0 ? (
          <p className="muted">{t("watchFolderEmpty")}</p>
        ) : (
          rows.map((row) => (
            <div className="modal-row" key={row.path}>
              <div>
                <div>{row.path.split(/[/\\]/).pop()}</div>
                <div className="muted">
                  {row.status === "stabilizing" && t("watchFolderStabilizing")}
                  {row.status === "processing" && `${t("watchFolderProcessing")} ${row.percent}%`}
                  {row.status === "done" &&
                    row.before &&
                    row.after &&
                    `${t("watchFolderDone")} · ${row.before.integrated_lufs.toFixed(1)} → ${row.after.integrated_lufs.toFixed(1)} LUFS`}
                  {row.status === "error" && `${t("watchFolderError")}: ${row.error}`}
                </div>
              </div>
              {row.status === "done" && !row.addedToProject && watchPath && (
                <button
                  className="icon"
                  onClick={() => {
                    void run(api.addWatchedFileToProject(watchPath, row.path));
                    setRows((prev) =>
                      prev.map((r) => (r.path === row.path ? { ...r, addedToProject: true } : r)),
                    );
                  }}
                >
                  {t("watchFolderAddToProject")}
                </button>
              )}
            </div>
          ))
        )}
      </div>
    </main>
  );
}
