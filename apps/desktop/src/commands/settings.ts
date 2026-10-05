import { patchUi } from "../store";
import type { Command } from "./types";

export const settingsCommand: Command = {
  id: "settings",
  label: "cmdSettings",
  group: "view",
  keys: [{ key: ",", ctrl: true }],
  run: () => {
    patchUi({ settingsOpen: true });
  },
};
