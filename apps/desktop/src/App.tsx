import { useEffect, useSyncExternalStore } from "react";
import { formatTime, localeSnapshot, subscribeLocale, t } from "./i18n";
import { api, exportActiveSequence } from "./ipc";
import { CrashReviewModal } from "./screens/CrashReviewModal";
import { Editor } from "./screens/Editor";
import { Home } from "./screens/Home";
import { Prefs } from "./screens/Prefs";
import { WatchFolder } from "./screens/WatchFolder";
import logo from "./assets/logo.png";
import { Icon } from "./editor/Icon";
import { ExportPanel } from "./editor/ExportPanel";
import type { SessionView } from "./types";
import { connect, exportsStore, patchUi, previewStore, run, sessionStore, uiStore, useStore } from "./store";

export function App() {
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  const exportJobs = useStore(exportsStore);
  const sequenceName = session?.project.sequences[session.project.active_sequence]?.name;
  const activeExports = exportJobs.filter((job) => job.status === "Queued" || (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))).length;
  useSyncExternalStore(subscribeLocale, localeSnapshot);

  useEffect(() => {
    void run(connect());
  }, []);

  return (
    <div className="app">
      <header className="topbar">
        <img className="brand" src={logo} alt="oca" width={22} height={22} />
        <nav className="nav">
          <button data-testid="nav-home" className={ui.screen === "home" ? "active" : ""} aria-current={ui.screen === "home" ? "page" : undefined} onClick={() => patchUi({ screen: "home" })}>{t("home")}</button>
          <button data-testid="nav-prefs" className={ui.screen === "prefs" ? "active" : ""} aria-current={ui.screen === "prefs" ? "page" : undefined} onClick={() => patchUi({ screen: "prefs" })}>{t("preferences")}</button>
          <button data-testid="nav-watch" className={ui.screen === "watch" ? "active" : ""} aria-current={ui.screen === "watch" ? "page" : undefined} onClick={() => patchUi({ screen: "watch" })}>{t("watchFolder")}</button>
        </nav>
        {session && (
          <>
            <span className="crumbs">
              <span className="project-name">{session.project.name}</span>
              {session.dirty && <span className="dirty" title={t("unsaved")} aria-label={t("unsaved")} />}
              {sequenceName && <><span className="crumb-sep">/</span><span className="muted">{sequenceName}</span></>}
            </span>
            <span className="spacer" />
            <Readout session={session} />
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

/** Playhead timecode plus the source's frame rate and resolution, read-only, centered in the bar. */
function Readout({ session }: { session: SessionView }) {
  const tick = useStore(previewStore);
  const source = session.project.media_library.find((asset) => asset.kind === "Video" && asset.resolution);
  return (
    <div className="readout" aria-label={t("timecode")}>
      <span className="timecode mono">{formatTime(tick.playheadSecs)}</span>
      {source?.fps != null && <span className="muted mono">{Math.round(source.fps)} FPS</span>}
      {source?.resolution && <span className="muted mono">{source.resolution[0]}×{source.resolution[1]}</span>}
    </div>
  );
}
