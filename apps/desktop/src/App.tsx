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
