import exportStyles from "../editor/export/export.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./home.module.css";
import { useEffect, useState } from "react";
import { Dialog } from "../editor/chrome/Dialog";
import { Icon } from "../editor/chrome/Icon";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { uniqueFunnyName } from "../projectNames";
import { askConfirm } from "../notifications";
import { patchUi, run, sessionStore, useStore } from "../store";
import type { RecentProject, RecoverySession } from "../types";

type Sort = "recent" | "modified" | "name";

/** The projects to show: filtered by `query` on name and folder, then ordered. `recent` keeps
 *  the order the backend returns (last opened first). Pure, for testing. */
export function visibleProjects(
  projects: RecentProject[],
  query: string,
  sort: Sort,
): RecentProject[] {
  const needle = query.trim().toLowerCase();
  const matching = projects.filter((project) => {
    return (
      needle === "" ||
      project.name.toLowerCase().includes(needle) ||
      project.folder.toLowerCase().includes(needle)
    );
  });
  if (sort === "modified") {
    return [...matching].sort((a, b) => {
      return b.modifiedUnixSecs - a.modifiedUnixSecs;
    });
  }
  if (sort === "name") {
    return [...matching].sort((a, b) => {
      return a.name.localeCompare(b.name);
    });
  }
  return matching;
}

/** The projects screen: every project that was created or opened, a way to start a new one, and
 *  the settings. Opening one enters the editor. */
export function Home() {
  const session = useStore(sessionStore);
  const [projects, setProjects] = useState<RecentProject[] | null>(null);
  const [query, setQuery] = useState("");
  const [sort, setSort] = useState<Sort>("recent");
  const [creating, setCreating] = useState(false);
  const [recoveries, setRecoveries] = useState<RecoverySession[]>([]);

  const loadProjects = () => {
    return run(api.getRecentProjects()).then((items) => {
      if (items) {
        setProjects(items);
      }
    });
  };

  const loadRecoveries = () => {
    return run(api.listRecoveries()).then((items) => {
      if (items) {
        setRecoveries(items);
      }
    });
  };

  useEffect(() => {
    void loadProjects();
    void loadRecoveries();
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

  const enterEditor = () => {
    patchUi({ screen: "editor", selectedClipIds: [] });
  };

  const createProject = async (name: string) => {
    if (!(await confirmDiscard())) {
      return;
    }
    if (await run(api.createProject(name, t("defaultSequenceName")))) {
      setCreating(false);
      enterEditor();
    }
  };

  const openProject = async () => {
    if (!(await confirmDiscard())) {
      return;
    }
    if (await run(api.openProject())) {
      enterEditor();
    }
  };

  const openRecent = async (path: string) => {
    if (!(await confirmDiscard())) {
      return;
    }
    const opened = await run(
      api.openProjectPath(path).then(() => {
        return true;
      }),
    );
    if (opened) {
      enterEditor();
    } else {
      // A file that is gone is dropped from the list by the backend.
      void loadProjects();
    }
  };

  const shown = visibleProjects(projects ?? [], query, sort);
  const hasProjects = (projects?.length ?? 0) > 0;

  return (
    <main
      className={cx(styles, baseStyles, exportStyles, "home")}
      data-testid="screen-home"
    >
      <header className={cx(styles, baseStyles, "home-head")}>
        <div>
          <h1>{t("projectsTitle")}</h1>
          <p className={cx(styles, baseStyles, "muted")}>{t("projectsSubtitle")}</p>
        </div>
        <div className={cx(styles, baseStyles, "home-actions")}>
          <button
            data-testid="new-project"
            className={cx(styles, baseStyles, "primary")}
            onClick={() => {
              setCreating(true);
            }}
          >
            <Icon name="plus" /> {t("newProject")}
          </button>
          <button
            data-testid="open-project"
            onClick={() => {
              void openProject();
            }}
          >
            <Icon name="folder" /> {t("openFile")}
          </button>
          <button
            data-testid="home-watch"
            onClick={() => {
              patchUi({ watchOpen: true });
            }}
          >
            {t("watchFolder")}
          </button>
          <button
            data-testid="home-settings"
            onClick={() => {
              patchUi({ settingsOpen: true });
            }}
          >
            <Icon name="settings" /> {t("preferences")}
          </button>
        </div>
      </header>

      {session && (session.filePath || session.dirty) && (
        <section
          className={cx(styles, baseStyles, "home-current")}
          aria-label={t("continueEditing")}
        >
          <div>
            <span className={cx(styles, baseStyles, "muted")}>{t("continueEditing")}</span>
            <strong>{session.project.name}</strong>
            {session.dirty && <span className={cx(styles, baseStyles, "muted")}> · {t("unsaved")}</span>}
          </div>
          <button
            data-testid="back-to-editor"
            className={cx(styles, baseStyles, "primary")}
            onClick={() => {
              patchUi({ screen: "editor" });
            }}
          >
            {t("backToEditor")}
          </button>
        </section>
      )}

      {hasProjects && (
        <div className={cx(styles, baseStyles, "home-tools")}>
          <label className={cx(styles, baseStyles, "home-search")}>
            <Icon name="search" />
            <input
              type="search"
              aria-label={t("searchProjects")}
              placeholder={t("searchProjects")}
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
              }}
            />
          </label>
          <select
            aria-label={t("sortProjects")}
            value={sort}
            onChange={(e) => {
              setSort(e.target.value as Sort);
            }}
          >
            <option value="recent">{t("sortRecent")}</option>
            <option value="modified">{t("sortModified")}</option>
            <option value="name">{t("sortName")}</option>
          </select>
          <span className={cx(styles, baseStyles, "muted")}>
            {shown.length} {t("projectCount")}
          </span>
        </div>
      )}

      {recoveries.length > 0 && (
        <section
          className={cx(styles, baseStyles, "recovery-list")}
          aria-label={t("unsavedWorkFound")}
          data-testid="unsaved-work"
        >
          <p>{t("unsavedWorkFound")}</p>
          {recoveries.map((recovery) => {
            return (
              <div
                key={recovery.id}
                className={cx(styles, baseStyles, "project-recovery")}
              >
                <strong>{recovery.projectName}</strong>
                <span className={cx(styles, baseStyles, "muted")}>
                  {t("unsavedWorkSavedAt")}{" "}
                  {new Date(recovery.savedUnixSecs * 1000).toLocaleString()}
                </span>
                <button
                  onClick={() => {
                    void (async () => {
                      if (!(await confirmDiscard())) {
                        return;
                      }
                      const ok = await run(
                        api.recoverSession(recovery.id).then(() => {
                          return true;
                        }),
                      );
                      if (ok) {
                        enterEditor();
                      } else {
                        void loadRecoveries();
                      }
                    })();
                  }}
                >
                  {t("recoverUnsavedWork")}
                </button>
                <button
                  onClick={() => {
                    void (async () => {
                      const confirmed = await askConfirm({
                        title: t("discardUnsavedWork"),
                        message: t("discardUnsavedWorkConfirm"),
                        confirmLabel: t("discardUnsavedWork"),
                        cancelLabel: t("cancel"),
                        danger: true,
                      });
                      if (confirmed) {
                        await run(api.discardRecovery(recovery.id));
                        void loadRecoveries();
                      }
                    })();
                  }}
                >
                  {t("discardUnsavedWork")}
                </button>
              </div>
            );
          })}
        </section>
      )}

      {projects !== null && !hasProjects && (
        <div
          className={cx(styles, baseStyles, "home-empty")}
          data-testid="projects-empty"
        >
          <h2>{t("projectsEmptyTitle")}</h2>
          <p className={cx(styles, baseStyles, "muted")}>{t("projectsEmptyHint")}</p>
          <button
            className={cx(styles, baseStyles, "primary")}
            onClick={() => {
              setCreating(true);
            }}
          >
            <Icon name="plus" /> {t("newProject")}
          </button>
        </div>
      )}

      {hasProjects && shown.length === 0 && <p className={cx(styles, baseStyles, "muted")}>{t("projectsNoMatches")}</p>}

      {shown.length > 0 && (
        <ul
          className={cx(styles, baseStyles, "project-list")}
          aria-label={t("projectsTitle")}
        >
          {shown.map((project) => {
            return (
              <li
                key={project.path}
                className={cx(styles, baseStyles, "project-row")}
              >
                <button
                  className={cx(styles, baseStyles, "project-open")}
                  title={project.path}
                  aria-label={t("openProjectNamed") + " " + project.name}
                  onClick={() => {
                    void openRecent(project.path);
                  }}
                >
                  <strong>{project.name}</strong>
                  <span className={cx(styles, baseStyles, "muted project-folder")}>{project.folder}</span>
                  <span className={cx(styles, baseStyles, "muted project-meta")}>
                    {new Date(project.modifiedUnixSecs * 1000).toLocaleString()}
                    {" · "}
                    {formatTime(project.durationSecs)}
                    {" · "}
                    {project.sequenceCount} {t("projectSequences")}
                    {" · "}
                    {project.mediaCount} {t("projectMediaCount")}
                  </span>
                </button>
                <div className={cx(styles, baseStyles, "project-actions")}>
                  <button
                    className={cx(styles, baseStyles, "icon")}
                    title={t("showInFolder")}
                    aria-label={t("showInFolder") + ": " + project.name}
                    onClick={() => {
                      void run(api.revealInFolder(project.path));
                    }}
                  >
                    <Icon name="folder" />
                  </button>
                  <button
                    className={cx(styles, baseStyles, "icon")}
                    title={t("removeRecent")}
                    aria-label={t("removeRecent") + ": " + project.name}
                    onClick={() => {
                      void run(api.removeRecentProject(project.path)).then(loadProjects);
                    }}
                  >
                    <Icon name="close" />
                  </button>
                </div>
                {project.autosavePath && (
                  <div className={cx(styles, baseStyles, "project-recovery")}>
                    <span className={cx(styles, baseStyles, "muted")}>{t("autosaveAvailable")}</span>
                    <button
                      onClick={() => {
                        void run(
                          api.restoreAutosave(project.path).then(() => {
                            return true;
                          }),
                        ).then((ok) => {
                          if (ok) {
                            patchUi({ screen: "editor" });
                          }
                        });
                      }}
                    >
                      {t("recoverAutosave")}
                    </button>
                    <button
                      onClick={() => {
                        void run(api.discardAutosave(project.path)).then(loadProjects);
                      }}
                    >
                      {t("discardAutosave")}
                    </button>
                  </div>
                )}
              </li>
            );
          })}
        </ul>
      )}

      {creating && (
        <NewProjectDialog
          taken={(projects ?? []).map((project) => {
            return project.name;
          })}
          onCreate={(name) => {
            void createProject(name);
          }}
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
    </main>
  );
}

/** Asks for the project's name; the file location comes next, from the save dialog. */
function NewProjectDialog({
  taken,
  onCreate,
  onClose,
}: {
  /** Names of the projects that exist, which the suggested name avoids. */
  taken: string[];
  onCreate: (name: string) => void;
  onClose: () => void;
}) {
  const [name, setName] = useState(() => {
    return uniqueFunnyName(taken);
  });
  const valid = name.trim() !== "";
  const submit = () => {
    if (valid) {
      onCreate(name.trim());
    }
  };
  return (
    <Dialog
      title={t("newProjectTitle")}
      onClose={onClose}
      actions={
        <>
          <button
            className={cx(styles, baseStyles, "icon")}
            onClick={onClose}
          >
            {t("cancel")}
          </button>
          <button
            className={cx(styles, baseStyles, "primary")}
            disabled={!valid}
            onClick={submit}
          >
            {t("createProject")}
          </button>
        </>
      }
    >
      <div className={cx(styles, baseStyles, "name-row")}>
        <label className={cx(styles, baseStyles, "dialog-field")}>
          <span>{t("projectName")}</span>
          <input
            type="text"
            value={name}
            aria-invalid={!valid}
            onChange={(e) => {
              setName(e.target.value);
            }}
            onFocus={(e) => {
              e.currentTarget.select();
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                submit();
              }
            }}
          />
        </label>
        <button
          type="button"
          className={cx(styles, baseStyles, "icon")}
          title={t("anotherName")}
          aria-label={t("anotherName")}
          onClick={() => {
            setName(uniqueFunnyName(taken));
          }}
        >
          <Icon name="reset" />
        </button>
      </div>
      {!valid && (
        <p
          className={cx(styles, baseStyles, "dialog-error")}
          role="alert"
        >
          {t("projectNameEmpty")}
        </p>
      )}
    </Dialog>
  );
}
