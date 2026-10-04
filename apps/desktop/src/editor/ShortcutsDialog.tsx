import { useState } from "react";
import { COMMANDS, GROUP_LABELS, type CommandGroup } from "../commands";
import { t } from "../i18n";
import { patchUi } from "../store";
import { formatKeys } from "../shortcuts";
import { Dialog } from "./Dialog";

const GROUP_ORDER: CommandGroup[] = ["playback", "edit", "timeline", "tools", "file", "view"];

function normalize(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** The keyboard cheat sheet, generated from the command table so it cannot go stale. */
export function ShortcutsDialog() {
  const [query, setQuery] = useState("");
  const needle = normalize(query.trim());

  const groups = GROUP_ORDER.map((group) => {
    const rows = COMMANDS.filter((command) => {
      return command.group === group && command.keys && command.keys.length > 0;
    }).filter((command) => {
      if (!needle) {
        return true;
      }
      const keys = (command.keys ?? [])
        .map((spec) => {
          return formatKeys(spec);
        })
        .join(" ");
      return normalize(t(command.label) + " " + keys).includes(needle);
    });
    return { group, rows };
  }).filter((entry) => {
    return entry.rows.length > 0;
  });

  return (
    <Dialog
      title={t("cmdShortcuts")}
      wide
      onClose={() => {
        patchUi({ shortcutsOpen: false });
      }}
    >
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
      <div className="shortcut-groups">
        {groups.length === 0 && <p className="muted">{t("shortcutsNone")}</p>}
        {groups.map(({ group, rows }) => {
          return (
            <section key={group}>
              <h4>{t(GROUP_LABELS[group])}</h4>
              <dl>
                {rows.map((command) => {
                  return (
                    <div
                      key={command.id}
                      className="shortcut-row"
                    >
                      <dt>{t(command.label)}</dt>
                      <dd>
                        {(command.keys ?? []).map((spec) => {
                          return <kbd key={formatKeys(spec)}>{formatKeys(spec)}</kbd>;
                        })}
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
