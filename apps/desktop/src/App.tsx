import { cx } from "./cx";
import baseStyles from "./styles/base.module.css";
import styles from "./App.module.css";
import { lazy, Suspense, useEffect, useState } from "react";
import { useI18n } from "./i18n";
import { COMMANDS } from "./commands";
import { loadKeymap } from "./keymap";
import { Dialog } from "./editor/chrome/Dialog";
import { ConfirmHost, ToastHost } from "./editor/chrome/Notifications";
import { StatusBar } from "./editor/chrome/StatusBar";
import { Home } from "./screens/Home";
import { CrashReviewModal } from "./screens/CrashReviewModal";
import { TopBar } from "./TopBar";
import { api, onCloseRequested } from "./ipc";
import { askChoice, askConfirm } from "./notifications";
import { connect, patchUi, run, sessionStore, uiStore, useStore } from "./store";

const Editor = lazy(async () => {
  const module = await import("./screens/Editor");
  return { default: module.Editor };
});
const ExportDialog = lazy(async () => {
  const module = await import("./editor/export/ExportDialog");
  return { default: module.ExportDialog };
});
const ExportPanel = lazy(async () => {
  const module = await import("./editor/export/ExportPanel");
  return { default: module.ExportPanel };
});
const Prefs = lazy(async () => {
  const module = await import("./screens/Prefs");
  return { default: module.Prefs };
});
const ShortcutsDialog = lazy(async () => {
  const module = await import("./editor/chrome/ShortcutsDialog");
  return { default: module.ShortcutsDialog };
});
const WatchFolder = lazy(async () => {
  const module = await import("./screens/WatchFolder");
  return { default: module.WatchFolder };
});
const CommandPalette = lazy(async () => {
  const module = await import("./editor/chrome/CommandPalette");
  return { default: module.CommandPalette };
});

export function App() {
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  const { gettext: t } = useI18n();
  const [watchFolderRequested, setWatchFolderRequested] = useState(ui.watchOpen);

  useEffect(() => {
    if (ui.watchOpen) {
      setWatchFolderRequested(true);
    }
  }, [ui.watchOpen]);

  useEffect(() => {
    void run(connect()).then(() => {
      return loadKeymap(
        new Set(
          COMMANDS.map((command) => {
            return command.id;
          }),
        ),
      );
    });
  }, []);

  // Closing the window with unsaved changes or exports still running asks first. With unsaved
  // changes the user can save and quit, quit without saving, or stay.
  useEffect(() => {
    const quit = () => {
      void run(api.quitApp());
    };
    const stop = onCloseRequested((request) => {
      const reasons = [
        request.dirty ? t("confirmQuitDirty") : "",
        request.exporting ? t("confirmQuitExporting") : "",
      ].filter(Boolean);
      const base = {
        title: t("confirmQuitTitle"),
        message: reasons.join(" "),
        cancelLabel: t("cancel"),
      };
      if (!request.dirty) {
        void askConfirm({ ...base, confirmLabel: t("confirmQuitAction") }).then((confirmed) => {
          if (confirmed) {
            quit();
          }
        });
        return;
      }
      void askChoice({
        ...base,
        confirmLabel: t("confirmQuitSave"),
        extraLabel: t("confirmQuitAction"),
      }).then(async (choice) => {
        if (choice === "extra") {
          quit();
        } else if (choice === "confirm") {
          // A cancelled save dialog means the user changed their mind about quitting.
          if (await run(api.saveProject(false))) {
            quit();
          }
        }
      });
    });
    return () => {
      void stop.then((unlisten) => {
        unlisten();
      });
    };
  }, []);

  return (
    <div className={cx(styles, baseStyles, "app")}>
      <TopBar session={session} />
      {ui.screen === "home" || !session ? (
        <Home />
      ) : (
        <Suspense
          fallback={
            <div
              className={cx(styles, baseStyles, "app-loading")}
              aria-busy="true"
              role="status"
            />
          }
        >
          <Editor session={session} />
        </Suspense>
      )}
      <StatusBar />
      {session && ui.exportQueueOpen && (
        <Suspense fallback={null}>
          <ExportPanel />
        </Suspense>
      )}
      {session && ui.exportDialogOpen && (
        <Suspense fallback={null}>
          <ExportDialog session={session} />
        </Suspense>
      )}
      {ui.settingsOpen && (
        <Dialog
          title={t("preferences")}
          wide
          onClose={() => {
            patchUi({ settingsOpen: false });
          }}
        >
          <Suspense fallback={null}>
            <Prefs />
          </Suspense>
        </Dialog>
      )}
      {watchFolderRequested && (
        <Suspense fallback={null}>
          <WatchFolder
            open={ui.watchOpen}
            onClose={() => {
              patchUi({ watchOpen: false });
            }}
          />
        </Suspense>
      )}
      {ui.shortcutsOpen && (
        <Suspense fallback={null}>
          <ShortcutsDialog />
        </Suspense>
      )}
      {ui.paletteOpen && (
        <Suspense fallback={null}>
          <CommandPalette />
        </Suspense>
      )}
      <ConfirmHost />
      <ToastHost />
      <CrashReviewModal />
    </div>
  );
}
