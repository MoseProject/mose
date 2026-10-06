import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./topbar.module.css";
import { t } from "../i18n";
import type { SessionView } from "../types";

/** Project name, save state and the active sequence. */
export function ProjectCrumbs({ session }: { session: SessionView }) {
  const sequenceName = session.project.sequences[session.project.active_sequence]?.name;
  return (
    <div className={cx(styles, baseStyles, "crumbs")}>
      <span className={cx(styles, baseStyles, "project-name")}>
        {t("projectLabel")}: {session.project.name}
      </span>
      {session.dirty && !session.filePath && (
        <span
          className={cx(styles, baseStyles, "dirty")}
          title={t("unsaved")}
          aria-label={t("unsaved")}
        />
      )}
      {session.dirty && session.filePath && (
        <span
          className={cx(styles, baseStyles, "save-state saving")}
          role="status"
        >
          {t("savingProject")}
        </span>
      )}
      {!session.dirty && session.filePath && (
        <span
          className={cx(styles, baseStyles, "save-state")}
          role="status"
          title={session.filePath}
        >
          {t("projectSaved")}
        </span>
      )}
      {sequenceName && (
        <span className={cx(styles, baseStyles, "muted")}>
          / {t("sequence")}: {sequenceName}
        </span>
      )}
    </div>
  );
}
