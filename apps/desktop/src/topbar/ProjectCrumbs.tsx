import { t } from "../i18n";
import type { SessionView } from "../types";

/** Project name, save state and the active sequence. */
export function ProjectCrumbs({ session }: { session: SessionView }) {
  const sequenceName = session.project.sequences[session.project.active_sequence]?.name;
  return (
    <div className="crumbs">
      <span className="project-name">
        {t("projectLabel")}: {session.project.name}
      </span>
      {session.dirty && !session.filePath && (
        <span
          className="dirty"
          title={t("unsaved")}
          aria-label={t("unsaved")}
        />
      )}
      {session.dirty && session.filePath && (
        <span
          className="save-state saving"
          role="status"
        >
          {t("savingProject")}
        </span>
      )}
      {!session.dirty && session.filePath && (
        <span
          className="save-state"
          role="status"
          title={session.filePath}
        >
          {t("projectSaved")}
        </span>
      )}
      {sequenceName && (
        <span className="muted">
          / {t("sequence")}: {sequenceName}
        </span>
      )}
    </div>
  );
}
