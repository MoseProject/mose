import { patchUi } from "../store";
import type { Command } from "./types";

export const shortcutsCommand: Command = {
  id: "shortcuts",
  label: "cmdShortcuts",
  group: "view",
  keys: [
    { key: "?", anyShift: true },
    { key: "/", ctrl: true },
  ],
  run: () => {
    patchUi({ shortcutsOpen: true });
  },
};
