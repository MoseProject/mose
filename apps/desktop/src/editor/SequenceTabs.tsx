import editorStyles from "../screens/editor.module.css";
import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./timeline/timeline.module.css";
import { useEffect, useState } from "react";
import { TIMELINE_ACTION_EVENT } from "../editing";
import { formatTime, t } from "../i18n";
import { api } from "../ipc";
import { run } from "../store";
import { Icon } from "./chrome/Icon";
import { Menu } from "./chrome/Menu";
import type { MarkerKind, SessionView } from "../types";

const markerKinds: MarkerKind[] = ["Standard", "ToDo", "Chapter", "Highlight"];

export function SequenceTabs({ session }: { session: SessionView }) {
  const { project } = session;
  const [renaming, setRenaming] = useState<number | null>(null);
  const activeId = project.sequences[project.active_sequence]?.id;
  useEffect(() => {
    const onAction = (e: Event) => {
      if ((e as CustomEvent<string>).detail === "renameSequence" && activeId !== undefined) {
        setRenaming(activeId);
      }
    };
    window.addEventListener(TIMELINE_ACTION_EVENT, onAction);
    return () => {
      window.removeEventListener(TIMELINE_ACTION_EVENT, onAction);
    };
  }, [activeId]);
  return (
    <div className={cx(styles, baseStyles, editorStyles, "sequence-tabs")}>
      <div
        className={cx(styles, baseStyles, editorStyles, "tab-list")}
        role="tablist"
        aria-label={t("sequences")}
      >
        {project.sequences.map((sequence, index) => {
          return (
            <div
              key={sequence.id}
              className={cx(styles, baseStyles, `sequence-tab ${index === project.active_sequence ? "active" : ""}`)}
            >
              {renaming === sequence.id ? (
                <input
                  autoFocus
                  defaultValue={sequence.name}
                  aria-label={t("renameSequence")}
                  onBlur={(e) => {
                    setRenaming(null);
                    if (e.target.value !== sequence.name) {
                      void run(api.renameSequence(sequence.id, e.target.value));
                    }
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.currentTarget.blur();
                    }
                    if (e.key === "Escape") {
                      setRenaming(null);
                    }
                  }}
                />
              ) : (
                <button
                  role="tab"
                  aria-selected={index === project.active_sequence}
                  onClick={() => {
                    return run(api.selectSequence(index));
                  }}
                >
                  {sequence.name}
                </button>
              )}
              <Menu
                label={<Icon name="more" />}
                ariaLabel={`${t("sequenceActions")}: ${sequence.name}`}
              >
                <button
                  onClick={() => {
                    return run(api.duplicateSequence(index, `${sequence.name} ${t("copy")}`));
                  }}
                >
                  {t("duplicateSequence")}
                </button>
                <button
                  onClick={() => {
                    return setRenaming(sequence.id);
                  }}
                >
                  {t("renameSequence")}
                </button>
                <button
                  disabled={index === 0}
                  onClick={() => {
                    return run(api.moveSequence(index, index - 1));
                  }}
                >
                  {t("moveSequenceLeft")}
                </button>
                <button
                  disabled={index === project.sequences.length - 1}
                  onClick={() => {
                    return run(api.moveSequence(index, index + 1));
                  }}
                >
                  {t("moveSequenceRight")}
                </button>
                <button
                  disabled={index === project.active_sequence}
                  onClick={() => {
                    return run(api.insertSequenceAsCompoundClip(sequence.id));
                  }}
                >
                  {t("insertAsCompoundClip")}
                </button>
                <button
                  disabled={project.sequences.length === 1}
                  onClick={() => {
                    return run(api.removeSequence(sequence.id));
                  }}
                >
                  {t("removeSequence")}
                </button>
              </Menu>
            </div>
          );
        })}
        <button
          className={cx(styles, baseStyles, "icon")}
          title={t("addSequence")}
          aria-label={t("addSequence")}
          onClick={() => {
            return run(api.addSequence(`${t("sequence")} ${project.sequences.length + 1}`));
          }}
        >
          <Icon name="plus" />
        </button>
      </div>
      <MarkerList session={session} />
    </div>
  );
}

function MarkerList({ session }: { session: SessionView }) {
  const markers = session.project.sequences[session.project.active_sequence].timeline.markers;
  return (
    <details className={cx(styles, baseStyles, editorStyles, "marker-list")}>
      <summary>{t("markers")}</summary>
      <div className={cx(styles, baseStyles, editorStyles, "marker-pop")}>
        <button
          onClick={() => {
            return run(api.addMarker());
          }}
        >
          {t("addMarker")}
        </button>
        {markers
          .slice()
          .sort((a, b) => {
            return a.position_secs - b.position_secs;
          })
          .map((marker) => {
            return (
              <div
                key={marker.id}
                className={cx(styles, baseStyles, editorStyles, "marker-row")}
              >
                <button
                  className={cx(styles, baseStyles, "icon")}
                  title={t("seekMarker")}
                  onClick={() => {
                    return run(api.seek(marker.position_secs));
                  }}
                >
                  {formatTime(marker.position_secs)}
                </button>
                <input
                  defaultValue={marker.label}
                  aria-label={t("markerLabel")}
                  onBlur={(e) => {
                    if (e.target.value !== marker.label) {
                      void run(
                        api.updateMarker(marker.id, {
                          label: e.target.value,
                        }),
                      );
                    }
                  }}
                />
                <select
                  value={marker.kind}
                  aria-label={t("markerKind")}
                  onChange={(e) => {
                    return run(
                      api.updateMarker(marker.id, {
                        kind: e.target.value as MarkerKind,
                      }),
                    );
                  }}
                >
                  {markerKinds.map((kind) => {
                    return <option key={kind}>{kind}</option>;
                  })}
                </select>
                {marker.kind === "ToDo" && (
                  <input
                    type="checkbox"
                    checked={marker.completed}
                    aria-label={t("markerCompleted")}
                    onChange={(e) => {
                      return run(
                        api.updateMarker(marker.id, {
                          completed: e.target.checked,
                        }),
                      );
                    }}
                  />
                )}
                <button
                  className={cx(styles, baseStyles, "icon")}
                  title={t("removeMarker")}
                  aria-label={t("removeMarker")}
                  onClick={() => {
                    return run(api.removeMarker(marker.id));
                  }}
                >
                  <Icon name="close" />
                </button>
              </div>
            );
          })}
      </div>
    </details>
  );
}
