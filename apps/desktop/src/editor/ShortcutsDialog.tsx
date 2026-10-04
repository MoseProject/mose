import { useEffect, useState } from "react";
import { COMMANDS, GROUP_LABELS, bindables, keysOf, type CommandGroup } from "../commands";
import { t, type TextKey } from "../i18n";
import {
  applyAdd,
  applyRemove,
  applyReset,
  describeKeys,
  keymapStore,
  planAdd,
  saveOverrides,
  specFromEvent,
  type Bindable,
} from "../keymap";
import { askConfirm } from "../notifications";
import { formatKeys, type KeySpec } from "../shortcuts";
import { patchUi, useStore } from "../store";
import { Dialog } from "./Dialog";
import { Icon } from "./Icon";

const GROUP_ORDER: CommandGroup[] = ["playback", "edit", "timeline", "tools", "file", "view"];

function normalize(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

interface Pending {
  id: string;
  spec: KeySpec;
  conflicts: Bindable[];
}

/** The keyboard shortcuts, generated from the command table, and where they are edited: record a
 *  new binding by pressing it, remove one, resolve a conflict, or restore the defaults. */
export function ShortcutsDialog() {
  const overrides = useStore(keymapStore);
  const [query, setQuery] = useState("");
  const [recording, setRecording] = useState<string | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [problem, setProblem] = useState<TextKey | null>(null);
  const needle = normalize(query.trim());

  useEffect(() => {
    if (recording === null) {
      return;
    }
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopImmediatePropagation();
      if (e.key === "Escape") {
        setRecording(null);
        return;
      }
      const spec = specFromEvent(e);
      if (!spec) {
        return;
      }
      setRecording(null);
      const rows = bindables();
      const plan = planAdd(recording, spec, rows, keymapStore.get());
      if (plan.status === "added") {
        void saveOverrides(applyAdd(recording, spec, rows, keymapStore.get(), false));
      } else if (plan.status === "conflict") {
        setPending({ id: recording, spec, conflicts: plan.conflicts });
      } else {
        setProblem(
          plan.status === "reserved"
            ? "keymapReserved"
            : plan.status === "duplicate"
              ? "keymapDuplicate"
              : "keymapFull",
        );
      }
    };
    document.addEventListener("keydown", onKey, true);
    return () => {
      document.removeEventListener("keydown", onKey, true);
    };
  }, [recording]);

  const resolveConflict = () => {
    if (pending) {
      void saveOverrides(applyAdd(pending.id, pending.spec, bindables(), keymapStore.get(), true));
    }
    setPending(null);
  };

  const resetAll = async () => {
    const ok = await askConfirm({
      title: t("keymapResetAllTitle"),
      message: t("keymapResetAllMessage"),
      confirmLabel: t("keymapResetAll"),
      cancelLabel: t("cancel"),
      danger: true,
    });
    if (ok) {
      setPending(null);
      setProblem(null);
      void saveOverrides({});
    }
  };

  const groups = GROUP_ORDER.map((group) => {
    const rows = COMMANDS.filter((command) => {
      return command.group === group;
    }).filter((command) => {
      if (!needle) {
        return true;
      }
      return normalize(t(command.label) + " " + describeKeys(keysOf(command))).includes(needle);
    });
    return { group, rows };
  }).filter((entry) => {
    return entry.rows.length > 0;
  });

  const conflictNames = pending
    ? pending.conflicts
        .map((conflict) => {
          const command = COMMANDS.find((candidate) => {
            return candidate.id === conflict.id;
          });
          return command ? t(command.label) : conflict.id;
        })
        .join(", ")
    : "";

  return (
    <Dialog
      title={t("cmdShortcuts")}
      wide
      onClose={() => {
        patchUi({ shortcutsOpen: false });
      }}
    >
      <div className="shortcut-toolbar">
        <input
          type="search"
          className="dialog-search"
          value={query}
          placeholder={t("shortcutsSearch")}
          aria-label={t("shortcutsSearch")}
          onChange={(e) => {
            setQuery(e.target.value);
          }}
        />
        <button
          disabled={Object.keys(overrides).length === 0}
          onClick={() => {
            void resetAll();
          }}
        >
          {t("keymapResetAll")}
        </button>
      </div>
      {pending && (
        <div
          className="shortcut-notice"
          role="alert"
        >
          <span>
            {formatKeys(pending.spec)} {t("keymapUsedBy")} {conflictNames}.
          </span>
          <button
            className="primary"
            onClick={resolveConflict}
          >
            {t("keymapReplace")}
          </button>
          <button
            onClick={() => {
              setPending(null);
            }}
          >
            {t("cancel")}
          </button>
        </div>
      )}
      {problem && (
        <div
          className="shortcut-notice error"
          role="alert"
        >
          <span>{t(problem)}</span>
          <button
            onClick={() => {
              setProblem(null);
            }}
          >
            {t("close")}
          </button>
        </div>
      )}
      <div className="shortcut-groups">
        {groups.length === 0 && <p className="muted">{t("shortcutsNone")}</p>}
        {groups.map(({ group, rows }) => {
          return (
            <section key={group}>
              <h4>{t(GROUP_LABELS[group])}</h4>
              <dl>
                {rows.map((command) => {
                  const keys = keysOf(command);
                  const customized = overrides[command.id] !== undefined;
                  return (
                    <div
                      key={command.id}
                      className={"shortcut-row" + (customized ? " customized" : "")}
                      data-command={command.id}
                    >
                      <dt>{t(command.label)}</dt>
                      <dd>
                        {keys.length === 0 && recording !== command.id && (
                          <span className="muted">{t("keymapUnbound")}</span>
                        )}
                        {keys.map((spec, index) => {
                          return (
                            <span
                              key={formatKeys(spec)}
                              className="key-chip"
                            >
                              <kbd>{formatKeys(spec)}</kbd>
                              <button
                                className="icon"
                                aria-label={t("keymapRemove") + ": " + formatKeys(spec)}
                                onClick={() => {
                                  void saveOverrides(
                                    applyRemove(command.id, index, bindables(), overrides),
                                  );
                                }}
                              >
                                <Icon
                                  name="close"
                                  size={11}
                                />
                              </button>
                            </span>
                          );
                        })}
                        {recording === command.id ? (
                          <span className="recording">{t("keymapPress")}</span>
                        ) : (
                          <button
                            className="icon"
                            aria-label={t("keymapAdd") + ": " + t(command.label)}
                            title={t("keymapAdd")}
                            onClick={() => {
                              setProblem(null);
                              setPending(null);
                              setRecording(command.id);
                            }}
                          >
                            <Icon name="plus" />
                          </button>
                        )}
                        {customized && (
                          <button
                            className="icon"
                            aria-label={t("keymapReset") + ": " + t(command.label)}
                            title={t("keymapReset")}
                            onClick={() => {
                              void saveOverrides(applyReset(command.id, overrides));
                            }}
                          >
                            <Icon name="reset" />
                          </button>
                        )}
                      </dd>
                    </div>
                  );
                })}
              </dl>
            </section>
          );
        })}
      </div>
    </Dialog>
  );
}
