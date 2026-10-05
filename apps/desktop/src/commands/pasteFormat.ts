import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const pasteFormatCommand: Command = {
  id: "pasteFormat",
  label: "cmdPasteFormat",
  group: "edit",
  keys: [{ key: "v", ctrl: true, shift: true }],
  enabled: hasSelection,
  run: () => {
    return run(api.pasteClipFormatting(selected()[0]));
  },
};
