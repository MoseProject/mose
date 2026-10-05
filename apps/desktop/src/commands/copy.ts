import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const copyCommand: Command = {
  id: "copy",
  label: "menuCopy",
  group: "edit",
  keys: [{ key: "c", ctrl: true }],
  enabled: hasSelection,
  run: () => {
    return run(api.copyClip(selected()[0]));
  },
};
