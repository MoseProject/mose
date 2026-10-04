import { useEffect, useSyncExternalStore } from "react";
import { localeSnapshot, subscribeLocale, t } from "./i18n";
import { api, exportActiveSequence } from "./ipc";
import { CrashReviewModal } from "./screens/CrashReviewModal";
import { Editor } from "./screens/Editor";
import { Home } from "./screens/Home";
import { Prefs } from "./screens/Prefs";
import { WatchFolder } from "./screens/WatchFolder";
import { Icon } from "./editor/Icon";
import { ExportPanel } from "./editor/ExportPanel";
import { connect, exportsStore, patchUi, run, sessionStore, uiStore, useStore } from "./store";

export function App() {
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  const exportJobs = useStore(exportsStore);
  const activeExports = exportJobs.filter((job) => job.status === "Queued" || (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))).length;
  useSyncExternalStore(subscribeLocale, localeSnapshot);

  useEffect(() => {
    void run(connect());
  }, []);

  return (
    <div className="app">
      <header className="topbar">
        <button
          data-testid="nav-home"
          className={`icon ${ui.screen === "home" ? "active" : ""}`}
          onClick={() => patchUi({ screen: "home" })}
        >
          {t("home")}
        </button>
        <button data-testid="nav-prefs" className={`icon ${ui.screen === "prefs" ? "active" : ""}`} onClick={() => patchUi({ screen: "prefs" })}>{t("preferences")}</button>
        <button data-testid="nav-watch" className={`icon ${ui.screen === "watch" ? "active" : ""}`} onClick={() => patchUi({ screen: "watch" })}>{t("watchFolder")}</button>
        {session && (
          <>
            <span className="project-name">{session.project.name}</span>
            {session.dirty && <span className="muted">• {t("unsaved")}</span>}
            <span className="spacer" />
            <button data-testid="undo" className="icon" disabled={!session.canUndo} onClick={() => run(api.undo())}>
              {t("undo")}
            </button>
            <button data-testid="redo" className="icon" disabled={!session.canRedo} onClick={() => run(api.redo())}>
              {t("redo")}
            </button>
            <button data-testid="save-project" onClick={() => run(api.saveProject(false))}>{t("save")}</button>
            <button
              data-testid="export-queue-toggle"
              className={`icon ${ui.exportQueueOpen ? "active" : ""}`}
              aria-expanded={ui.exportQueueOpen}
              onClick={() => patchUi({ exportQueueOpen: !ui.exportQueueOpen })}
            >
              {t("exportQueue")}{activeExports > 0 && <span className="badge">{activeExports}</span>}
            </button>
            <button data-testid="export" className="primary" onClick={() => { patchUi({ exportQueueOpen: true }); void run(exportActiveSequence(() => window.confirm(t("exportOverwriteConfirm")))); }}>
              {t("export")}
            </button>
          </>
        )}
      </header>
      {ui.screen === "watch" ? (
        <WatchFolder />
      ) : ui.screen === "home" || !session ? (
        <Home />
      ) : ui.screen === "prefs" ? (
        <Prefs />
      ) : (
        <Editor session={session} />
      )}
      {ui.error && (
        <div className="error-banner" role="alert">
          <span>
            {t("errorPrefix")}: {ui.error}
          </span>
          <button className="icon" aria-label={t("close")} onClick={() => patchUi({ error: null })}>
            <Icon name="close" />
          </button>
        </div>
      )}
      {session && ui.exportQueueOpen && <ExportPanel />}
      <CrashReviewModal />
    </div>
  );
}
