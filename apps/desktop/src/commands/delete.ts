import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const deleteCommand: Command = {
  id: "delete",
  label: "delete",
  group: "edit",
  keys: [{ key: "Delete" }, { key: "Backspace" }],
  enabled: hasSelection,
  run: () => {
    return run(api.deleteClips(selected(), false));
  },
};
