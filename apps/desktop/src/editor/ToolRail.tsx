import { cx } from "../cx";
import baseStyles from "../styles/base.module.css";
import styles from "./toolrail.module.css";
import { useState } from "react";
import { withShortcut, shortcutLabel } from "../commands";
import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { keymapStore } from "../keymap";
import { patchUi, run, uiStore, useStore, type EditorTool } from "../store";
import { Icon, type IconName } from "./chrome/Icon";
import { Menu } from "./chrome/Menu";

interface ToolDef {
  tool: EditorTool;
  label: TextKey;
  icon: IconName;
  command: string;
}

const SELECT: ToolDef = {
  tool: "select",
  label: "toolSelect",
  icon: "cursor",
  command: "toolSelect",
};
const TEXT: ToolDef = { tool: "text", label: "toolText", icon: "text", command: "toolText" };
/** The three trim tools look alike and are used rarely, so they share one rail slot. */
const TRIM_TOOLS: ToolDef[] = [
  { tool: "roll", label: "toolRoll", icon: "roll", command: "toolRoll" },
  { tool: "slip", label: "toolSlip", icon: "slip", command: "toolSlip" },
  { tool: "slide", label: "toolSlide", icon: "slide", command: "toolSlide" },
];

function ToolButton({ def, active }: { def: ToolDef; active: boolean }) {
  return (
    <button
      className={cx(styles, baseStyles, "icon " + (active ? "active" : ""))}
      title={withShortcut(t(def.label), def.command)}
      aria-label={t(def.label)}
      aria-pressed={active}
      onClick={() => {
        patchUi({ tool: def.tool });
      }}
    >
      <Icon
        name={def.icon}
        size={18}
      />
    </button>
  );
}

/** Vertical edit-tool strip beside the workspace: the active tool is mode state, split is an
 *  action. */
export function ToolRail() {
  const ui = useStore(uiStore);
  useStore(keymapStore);
  const [lastTrim, setLastTrim] = useState<EditorTool>("roll");
  const activeTrim = TRIM_TOOLS.find((def) => {
    return def.tool === ui.tool;
  });
  const shownTrim =
    activeTrim ??
    TRIM_TOOLS.find((def) => {
      return def.tool === lastTrim;
    }) ??
    TRIM_TOOLS[0];

  return (
    <nav
      className={cx(styles, baseStyles, "tool-rail")}
      aria-label={t("tools")}
    >
      <ToolButton
        def={SELECT}
        active={ui.tool === "select"}
      />
      <div className={cx(styles, baseStyles, "trim-group " + (activeTrim ? "active" : ""))}>
        <Menu
          label={
            <Icon
              name={shownTrim.icon}
              size={18}
            />
          }
          ariaLabel={t("trimTools")}
        >
          {TRIM_TOOLS.map((def) => {
            return (
              <button
                key={def.tool}
                aria-label={t(def.label)}
                data-shortcut={shortcutLabel(def.command)}
                aria-pressed={ui.tool === def.tool}
                onClick={() => {
                  setLastTrim(def.tool);
                  patchUi({ tool: def.tool });
                }}
              >
                {t(def.label)}
              </button>
            );
          })}
        </Menu>
      </div>
      <ToolButton
        def={TEXT}
        active={ui.tool === "text"}
      />
      <span className={cx(styles, baseStyles, "rail-sep")} />
      <button
        className={cx(styles, baseStyles, "icon")}
        title={withShortcut(t("split"), "split")}
        aria-label={t("split")}
        onClick={() => {
          return run(api.splitAtPlayhead(uiStore.get().selectedClipIds));
        }}
      >
        <Icon
          name="scissors"
          size={18}
        />
      </button>
    </nav>
  );
}
