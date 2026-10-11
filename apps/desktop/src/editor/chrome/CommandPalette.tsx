import { cx } from "../../cx";
import baseStyles from "../../styles/base.module.css";
import styles from "./chrome.module.css";
import { useState } from "react";
import { COMMANDS, shortcutLabel } from "../../commands";
import { t } from "../../i18n";
import { patchUi } from "../../store";
import { Dialog } from "./Dialog";

function normalize(text: string): string {
  return text.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase();
}

/** Type to find any command by name and run it with Enter: the escape hatch for the many
 *  actions that live in menus, so none has to be remembered by location. */
export function CommandPalette() {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const needle = normalize(query.trim());

  const results = COMMANDS.filter((command) => {
    return command.palette !== false;
  }).filter((command) => {
    return !needle || normalize(t(command.label)).includes(needle);
  });
  const current = Math.min(active, Math.max(0, results.length - 1));

  const close = () => {
    patchUi({ paletteOpen: false });
  };
  const runAt = (index: number) => {
    const command = results[index];
    if (!command || (command.enabled && !command.enabled())) {
      return;
    }
    close();
    void command.run();
  };

  return (
    <Dialog
      title={t("cmdPalette")}
      onClose={close}
    >
      <input
        type="search"
        className={cx(styles, baseStyles, "dialog-search")}
        value={query}
        placeholder={t("paletteSearch")}
        aria-label={t("paletteSearch")}
        aria-controls="palette-results"
        onChange={(e) => {
          setQuery(e.target.value);
          setActive(0);
        }}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown") {
            e.preventDefault();
            setActive(Math.min(results.length - 1, current + 1));
          } else if (e.key === "ArrowUp") {
            e.preventDefault();
            setActive(Math.max(0, current - 1));
          } else if (e.key === "Enter") {
            e.preventDefault();
            runAt(current);
          }
        }}
      />
      <ul
        id="palette-results"
        className={cx(styles, baseStyles, "palette-results")}
        role="listbox"
        aria-label={t("cmdPalette")}
      >
        {results.length === 0 && (
          <li className={cx(styles, baseStyles, "muted")}>{t("paletteNone")}</li>
        )}
        {results.map((command, index) => {
          const disabled = Boolean(command.enabled && !command.enabled());
          return (
            <li
              key={command.id}
              role="option"
              aria-selected={index === current}
              aria-disabled={disabled}
              className={cx(styles, baseStyles, index === current ? "active" : "")}
              onMouseEnter={() => {
                setActive(index);
              }}
              onClick={() => {
                runAt(index);
              }}
            >
              <span>{t(command.label)}</span>
              {shortcutLabel(command.id) && <kbd>{shortcutLabel(command.id)}</kbd>}
            </li>
          );
        })}
      </ul>
    </Dialog>
  );
}
