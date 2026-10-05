import { patchUi } from "../store";
import type { Command } from "./types";

export const deselectCommand: Command = {
  id: "deselect",
  label: "cmdDeselect",
  group: "edit",
  keys: [{ key: "Escape" }],
  run: () => {
    patchUi({ selectedClipIds: [] });
  },
};
