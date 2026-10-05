import { patchUi } from "../store";
import type { Command } from "./types";

export const exportCommand: Command = {
  id: "export",
  label: "export",
  group: "file",
  keys: [{ key: "e", ctrl: true }],
  run: () => {
    patchUi({ exportDialogOpen: true });
  },
};
