import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const copyFormatCommand: Command = {
  id: "copyFormat",
  label: "cmdCopyFormat",
  group: "edit",
  keys: [{ key: "c", ctrl: true, shift: true }],
  enabled: hasSelection,
  run: () => {
    return run(api.copyClipFormatting(selected()[0]));
  },
};
