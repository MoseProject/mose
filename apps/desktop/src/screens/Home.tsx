import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { patchUi, run, sessionStore, useStore } from "../store";
import type { RecentProject } from "../types";

export function Home() {
  const session = useStore(sessionStore);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const loadRecent = () => void run(api.getRecentProjects()).then((items) => items && setRecent(items));

  useEffect(() => { loadRecent(); }, []);
  const confirmDiscard = () => !session?.dirty || window.confirm(t("unsavedConfirm"));

  const createProject = async () => {
    if (!confirmDiscard()) return;
    const created = await run(
      api.newProject(t("untitledProject"), t("defaultSequenceName")).then(() => true),
    );
    if (created) patchUi({ screen: "editor", selectedClipIds: [] });
  };

  const openProject = async () => {
    if (!confirmDiscard()) return;
    if (await run(api.openProject())) patchUi({ screen: "editor", selectedClipIds: [] });
  };

  const openRecent = async (path: string) => {
    if (!confirmDiscard()) return;
    if (await run(api.openProjectPath(path).then(() => true))) patchUi({ screen: "editor", selectedClipIds: [] });
  };

  return (
    <main className="home" data-testid="screen-home">
      <h1>oca</h1>
      <p className="muted">{t("homeSubtitle")}</p>
      <div className="actions">
        <button data-testid="new-project" className={session ? "" : "primary"} onClick={createProject}>
          {t("newProject")}
        </button>
        <button onClick={openProject}>{t("openProject")}</button>
        {session && (
          <button data-testid="back-to-editor" className="primary" onClick={() => patchUi({ screen: "editor" })}>{t("backToEditor")}</button>
        )}
      </div>
      {recent.length > 0 && <section className="recent-projects">
        <h2>{t("homeTitle")}</h2>
        {recent.map((project) => <article key={project.path} className="recent-project">
          <button className="recent-open" onClick={() => void openRecent(project.path)}>
            <strong>{project.name}</strong><span className="muted">{new Date(project.modifiedUnixSecs * 1000).toLocaleDateString()}</span>
          </button>
          {project.autosavePath && <><button onClick={() => run(api.restoreAutosave(project.path).then(() => true)).then((ok) => ok && patchUi({ screen: "editor" }))}>{t("recoverAutosave")}</button><button onClick={() => run(api.discardAutosave(project.path)).then(loadRecent)}>{t("discardAutosave")}</button></>}
          <button className="icon" title={t("removeRecent")} onClick={() => run(api.removeRecentProject(project.path)).then(loadRecent)}>×</button>
        </article>)}
      </section>}
    </main>
  );
}
