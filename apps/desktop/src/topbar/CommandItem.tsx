import { findCommand, shortcutLabel } from "../commands";
import { t } from "../i18n";

/** A menu entry bound to a command: its label, key binding, enabled state and action all come
 *  from the command table. */
export function CommandItem({ id }: { id: string }) {
  const command = findCommand(id);
  if (!command) {
    return null;
  }
  return (
    <button
      data-shortcut={shortcutLabel(id)}
      disabled={command.enabled ? !command.enabled() : false}
      onClick={() => {
        void command.run();
      }}
    >
      {t(command.label)}
    </button>
  );
}
