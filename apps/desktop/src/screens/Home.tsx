import { t } from "../i18n";
import { api } from "../ipc";
import { patchUi, run, sessionStore, useStore } from "../store";

export function Home() {
  const session = useStore(sessionStore);

  const createProject = async () => {
    const created = await run(
      api.newProject(t("untitledProject"), t("defaultSequenceName")).then(() => true),
    );
    if (created) patchUi({ screen: "editor", selectedClipIds: [] });
  };

  const openProject = async () => {
    if (await run(api.openProject())) patchUi({ screen: "editor", selectedClipIds: [] });
  };

  return (
    <main className="home">
      <h1>oca</h1>
      <p className="muted">{t("homeSubtitle")}</p>
      <div className="actions">
        <button className="primary" onClick={createProject}>
          {t("newProject")}
        </button>
        <button onClick={openProject}>{t("openProject")}</button>
        {session && (
          <button onClick={() => patchUi({ screen: "editor" })}>{t("backToEditor")}</button>
        )}
      </div>
    </main>
  );
}
