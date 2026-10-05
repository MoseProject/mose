import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const cutCommand: Command = {
  id: "cut",
  label: "menuCut",
  group: "edit",
  keys: [{ key: "x", ctrl: true }],
  enabled: hasSelection,
  run: () => {
    return run(api.cutClip(selected()[0]));
  },
};
