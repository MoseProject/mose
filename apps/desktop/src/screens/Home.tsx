import { useEffect, useState } from "react";
import { t } from "../i18n";
import { api } from "../ipc";
import { askConfirm } from "../notifications";
import { patchUi, run, sessionStore, useStore } from "../store";
import type { RecentProject } from "../types";

export function Home() {
  const session = useStore(sessionStore);
  const [recent, setRecent] = useState<RecentProject[]>([]);
  const loadRecent = () => {
    return void run(api.getRecentProjects()).then((items) => {
      return items && setRecent(items);
    });
  };

  useEffect(() => {
    loadRecent();
  }, []);
  const confirmDiscard = () => {
    if (!session?.dirty) {
      return Promise.resolve(true);
    }
    return askConfirm({
      title: t("confirmDiscardTitle"),
      message: t("unsavedConfirm"),
      confirmLabel: t("confirmDiscardAction"),
      cancelLabel: t("cancel"),
      danger: true,
    });
  };

  const createProject = async () => {
    if (!(await confirmDiscard())) {
      return;
    }
    const created = await run(
      api.newProject(t("untitledProject"), t("defaultSequenceName")).then(() => {
        return true;
      }),
    );
    if (created) {
      patchUi({ screen: "editor", selectedClipIds: [] });
    }
  };

  const openProject = async () => {
    if (!(await confirmDiscard())) {
      return;
    }
    if (await run(api.openProject())) {
      patchUi({ screen: "editor", selectedClipIds: [] });
    }
  };

  const openRecent = async (path: string) => {
    if (!(await confirmDiscard())) {
      return;
    }
    if (
      await run(
        api.openProjectPath(path).then(() => {
          return true;
        }),
      )
    ) {
      patchUi({ screen: "editor", selectedClipIds: [] });
    }
  };

  return (
    <main
      className="home"
      data-testid="screen-home"
    >
      <h1>oca</h1>
      <p className="muted">{t("homeSubtitle")}</p>
      <div className="actions">
        <button
          data-testid="new-project"
          className={session ? "" : "primary"}
          onClick={createProject}
        >
          {t("newProject")}
        </button>
        <button onClick={openProject}>{t("openProject")}</button>
        {session && (
          <button
            data-testid="back-to-editor"
            className="primary"
            onClick={() => {
              return patchUi({ screen: "editor" });
            }}
          >
            {t("backToEditor")}
          </button>
        )}
      </div>
      {recent.length > 0 && (
        <section className="recent-projects">
          <h2>{t("homeTitle")}</h2>
          {recent.map((project) => {
            return (
              <article
                key={project.path}
                className="recent-project"
              >
                <button
                  className="recent-open"
                  onClick={() => {
                    return void openRecent(project.path);
                  }}
                >
                  <strong>{project.name}</strong>
                  <span className="muted">
                    {new Date(project.modifiedUnixSecs * 1000).toLocaleDateString()}
                  </span>
                </button>
                {project.autosavePath && (
                  <>
                    <button
                      onClick={() => {
                        return run(
                          api.restoreAutosave(project.path).then(() => {
                            return true;
                          }),
                        ).then((ok) => {
                          return ok && patchUi({ screen: "editor" });
                        });
                      }}
                    >
                      {t("recoverAutosave")}
                    </button>
                    <button
                      onClick={() => {
                        return run(api.discardAutosave(project.path)).then(loadRecent);
                      }}
                    >
                      {t("discardAutosave")}
                    </button>
                  </>
                )}
                <button
                  className="icon"
                  title={t("removeRecent")}
                  onClick={() => {
                    return run(api.removeRecentProject(project.path)).then(loadRecent);
                  }}
                >
                  ×
                </button>
              </article>
            );
          })}
        </section>
      )}
    </main>
  );
}
