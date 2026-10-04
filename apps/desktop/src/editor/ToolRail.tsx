import { t, type TextKey } from "../i18n";
import { api } from "../ipc";
import { patchUi, run, uiStore, useStore, type EditorTool } from "../store";
import { Icon, type IconName } from "./Icon";

const TOOLS: [EditorTool, TextKey, IconName][] = [
  ["select", "toolSelect", "cursor"],
  ["roll", "toolRoll", "roll"],
  ["slip", "toolSlip", "slip"],
  ["slide", "toolSlide", "slide"],
  ["text", "toolText", "text"],
];

/** Vertical edit-tool strip beside the workspace: the active tool is mode state, split is an action. */
export function ToolRail() {
  const ui = useStore(uiStore);
  return (
    <nav
      className="tool-rail"
      aria-label={t("tools")}
    >
      {TOOLS.map(([tool, label, icon]) => {
        return (
          <button
            key={tool}
            className={`icon ${ui.tool === tool ? "active" : ""}`}
            title={t(label)}
            aria-label={t(label)}
            aria-pressed={ui.tool === tool}
            onClick={() => {
              return patchUi({ tool });
            }}
          >
            <Icon
              name={icon}
              size={18}
            />
          </button>
        );
      })}
      <span className="rail-sep" />
      <button
        className="icon"
        title={t("split")}
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
