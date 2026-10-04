import { useEffect, useSyncExternalStore } from "react";
import { localeSnapshot, subscribeLocale, t } from "./i18n";
import { COMMANDS } from "./commands";
import { loadKeymap } from "./keymap";
import { CommandPalette } from "./editor/CommandPalette";
import { Dialog } from "./editor/Dialog";
import { ExportDialog } from "./editor/ExportDialog";
import { ExportPanel } from "./editor/ExportPanel";
import { ConfirmHost, ToastHost } from "./editor/Notifications";
import { ShortcutsDialog } from "./editor/ShortcutsDialog";
import { StatusBar } from "./editor/StatusBar";
import { CrashReviewModal } from "./screens/CrashReviewModal";
import { Editor } from "./screens/Editor";
import { Home } from "./screens/Home";
import { Prefs } from "./screens/Prefs";
import { WatchFolder } from "./screens/WatchFolder";
import { TopBar } from "./TopBar";
import { api, onCloseRequested } from "./ipc";
import { askChoice, askConfirm } from "./notifications";
import { connect, patchUi, run, sessionStore, uiStore, useStore } from "./store";

export function App() {
  const session = useStore(sessionStore);
  const ui = useStore(uiStore);
  useSyncExternalStore(subscribeLocale, localeSnapshot);

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
    <div className="app">
      <TopBar session={session} />
      {ui.screen === "home" || !session ? <Home /> : <Editor session={session} />}
      <StatusBar />
      {session && ui.exportQueueOpen && <ExportPanel />}
      {session && ui.exportDialogOpen && <ExportDialog session={session} />}
      {ui.settingsOpen && (
        <Dialog
          title={t("preferences")}
          wide
          onClose={() => {
            patchUi({ settingsOpen: false });
          }}
        >
          <Prefs />
        </Dialog>
      )}
      <WatchFolder
        open={ui.watchOpen}
        onClose={() => {
          patchUi({ watchOpen: false });
        }}
      />
      {ui.shortcutsOpen && <ShortcutsDialog />}
      {ui.paletteOpen && <CommandPalette />}
      <ConfirmHost />
      <ToastHost />
      <CrashReviewModal />
    </div>
  );
}
