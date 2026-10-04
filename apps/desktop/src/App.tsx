import { useEffect, useSyncExternalStore } from "react";
import { localeSnapshot, subscribeLocale, t } from "./i18n";
import { CrashReviewModal } from "./screens/CrashReviewModal";
import { Editor } from "./screens/Editor";
import { Home } from "./screens/Home";
import { Prefs } from "./screens/Prefs";
import { WatchFolder } from "./screens/WatchFolder";
import { Icon } from "./editor/Icon";
import { ExportDialog } from "./editor/ExportDialog";
import { ExportPanel } from "./editor/ExportPanel";
import { TopBar } from "./TopBar";
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
      <TopBar session={session} />
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
        <div
          className="error-banner"
          role="alert"
        >
          <span>
            {t("errorPrefix")}: {ui.error}
          </span>
          <button
            className="icon"
            aria-label={t("close")}
            onClick={() => {
              return patchUi({ error: null });
            }}
          >
            <Icon name="close" />
          </button>
        </div>
      )}
      {session && ui.exportQueueOpen && <ExportPanel />}
      {session && ui.exportDialogOpen && <ExportDialog session={session} />}
      <CrashReviewModal />
    </div>
  );
}
