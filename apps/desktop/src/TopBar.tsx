import shellStyles from "./App.module.css";
import { cx } from "./cx";
import baseStyles from "./styles/base.module.css";
import styles from "./topbar/topbar.module.css";
import logo from "./assets/logo.png";
import { withShortcut } from "./commands";
import { Icon } from "./editor/chrome/Icon";
import { t } from "./i18n";
import { keymapStore } from "./keymap";
import { exportsStore, patchUi, uiStore, useStore } from "./store";
import type { SessionView } from "./types";
import { MenuBar } from "./topbar/MenuBar";
import { ProjectActions } from "./topbar/ProjectActions";
import { ProjectCrumbs } from "./topbar/ProjectCrumbs";

/** Brand, project/sequence, the application menus, the playhead readout and the global actions. */
export function TopBar({ session }: { session: SessionView | null }) {
  const ui = useStore(uiStore);
  useStore(keymapStore); // shortcut hints follow the user's changes
  const jobs = useStore(exportsStore);
  const activeExports = jobs.filter((job) => {
    return (
      job.status === "Queued" ||
      (typeof job.status === "object" && ("Rendering" in job.status || "Paused" in job.status))
    );
  }).length;
  const selected = ui.selectedClipIds;
  const editing = session !== null && ui.screen === "editor";

  return (
    <header className={cx(styles, baseStyles, shellStyles, "topbar")}>
      <div className={cx(styles, baseStyles, "brand-box")}>
        <img
          className={cx(styles, baseStyles, "brand")}
          src={logo}
          alt="oca"
          width={24}
          height={24}
        />
      </div>
      {session && editing && <ProjectCrumbs session={session} />}
      <MenuBar
        session={session}
        screen={ui.screen}
        selected={selected}
      />
      <span className={cx(styles, baseStyles, shellStyles, "spacer")} />
      {session && editing && (
        <ProjectActions
          session={session}
          exportQueueOpen={ui.exportQueueOpen}
          activeExports={activeExports}
        />
      )}
      <button
        className={cx(styles, baseStyles, "icon")}
        aria-label={t("preferences")}
        title={withShortcut(t("preferences"), "settings")}
        onClick={() => {
          return patchUi({ settingsOpen: true });
        }}
      >
        <Icon
          name="settings"
          size={17}
        />
      </button>
    </header>
  );
}
