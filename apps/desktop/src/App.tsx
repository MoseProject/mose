import { useEffect, useSyncExternalStore } from "react";
import { localeSnapshot, subscribeLocale, t } from "./i18n";
import { api, exportActiveSequence } from "./ipc";
import { Editor } from "./screens/Editor";
import { Home } from "./screens/Home";
import { Prefs } from "./screens/Prefs";
import { connect, patchUi, run, sessionStore, uiStore, useStore } from "./store";

export function App() {
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  useSyncExternalStore(subscribeLocale, localeSnapshot);

  useEffect(() => {
    void run(connect());
  }, []);

  return (
    <div className="app">
      <header className="topbar">
        <button
          className={`icon ${ui.screen === "home" ? "active" : ""}`}
          onClick={() => patchUi({ screen: "home" })}
        >
          {t("home")}
        </button>
        <button className={`icon ${ui.screen === "prefs" ? "active" : ""}`} onClick={() => patchUi({ screen: "prefs" })}>{t("preferences")}</button>
        {session && (
          <>
            <span className="project-name">{session.project.name}</span>
            {session.dirty && <span className="muted">• {t("unsaved")}</span>}
            <span className="spacer" />
            <button className="icon" disabled={!session.canUndo} onClick={() => run(api.undo())}>
              {t("undo")}
            </button>
            <button className="icon" disabled={!session.canRedo} onClick={() => run(api.redo())}>
              {t("redo")}
            </button>
            <button onClick={() => run(api.saveProject(false))}>{t("save")}</button>
            <button className="primary" onClick={() => run(exportActiveSequence(() => window.confirm(t("exportOverwriteConfirm"))))}>
              {t("export")}
            </button>
          </>
        )}
      </header>
      {ui.screen === "home" || !session ? <Home /> : ui.screen === "prefs" ? <Prefs /> : <Editor session={session} />}
      {ui.error && (
        <div className="error-banner" role="alert">
          <span>
            {t("errorPrefix")}: {ui.error}
          </span>
          <button className="icon" onClick={() => patchUi({ error: null })}>
            ✕
          </button>
        </div>
      )}
    </div>
  );
}
