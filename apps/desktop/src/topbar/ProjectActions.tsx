import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./topbar.module.css";
import { withShortcut } from "../commands";
import { Icon } from "../editor/chrome/Icon";
import { t } from "../i18n";
import { api } from "../ipc";
import { patchUi, run } from "../store";
import type { SessionView } from "../types";
import { Readout } from "./Readout";

/** Undo, redo, save, the export queue toggle and the export button. */
export function ProjectActions({
  session,
  exportQueueOpen,
  activeExports,
}: {
  session: SessionView;
  exportQueueOpen: boolean;
  activeExports: number;
}) {
  return (
    <>
      <Readout session={session} />
      <button
        data-testid="undo"
        className={cx(styles, baseStyles, "icon")}
        aria-label={t("undo")}
        title={withShortcut(t("undo"), "undo")}
        disabled={!session.canUndo}
        onClick={() => {
          return run(api.undo());
        }}
      >
        <Icon
          name="undo"
          size={16}
        />
      </button>
      <button
        data-testid="redo"
        className={cx(styles, baseStyles, "icon")}
        aria-label={t("redo")}
        title={withShortcut(t("redo"), "redo")}
        disabled={!session.canRedo}
        onClick={() => {
          return run(api.redo());
        }}
      >
        <Icon
          name="redo"
          size={16}
        />
      </button>
      <button
        data-testid="save-project"
        className={cx(styles, baseStyles, "icon")}
        aria-label={t("save")}
        title={withShortcut(t("save"), "save")}
        onClick={() => {
          return run(api.saveProject(false));
        }}
      >
        <Icon
          name="save"
          size={16}
        />
      </button>
      <button
        data-testid="export-queue-toggle"
        className={cx(styles, baseStyles, "icon " + (exportQueueOpen ? "active" : ""))}
        aria-label={t("exportQueue")}
        title={t("exportQueue")}
        aria-expanded={exportQueueOpen}
        onClick={() => {
          return patchUi({ exportQueueOpen: !exportQueueOpen });
        }}
      >
        <Icon
          name="queue"
          size={16}
        />
        {activeExports > 0 && <span className={cx(styles, baseStyles, "badge")}>{activeExports}</span>}
      </button>
      <button
        data-testid="export"
        className={cx(styles, baseStyles, "primary export")}
        title={withShortcut(t("export"), "export")}
        onClick={() => {
          patchUi({ exportDialogOpen: true });
        }}
      >
        <Icon
          name="upload"
          size={14}
        />{" "}
        {t("export")}
      </button>
    </>
  );
}
