import { patchUi, uiStore } from "../store";
import type { Command } from "./types";

export const toggleGuidesCommand: Command = {
  id: "toggleGuides",
  label: "cmdToggleGuides",
  group: "view",
  run: () => {
    patchUi({ monitorGuides: !uiStore.get().monitorGuides });
  },
};
