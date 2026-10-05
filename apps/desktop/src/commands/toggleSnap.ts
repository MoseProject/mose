import { patchUi, uiStore } from "../store";
import type { Command } from "./types";

export const toggleSnapCommand: Command = {
  id: "toggleSnap",
  label: "cmdToggleSnap",
  group: "timeline",
  keys: [{ key: "n" }],
  run: () => {
    patchUi({ snapEnabled: !uiStore.get().snapEnabled });
  },
};
