import { api } from "../ipc";
import { run } from "../store";
import type { Command } from "./types";
import { selected, hasSelection } from "./helpers";

export const rippleDeleteCommand: Command = {
  id: "rippleDelete",
  label: "rippleDelete",
  group: "edit",
  keys: [
    { key: "Delete", shift: true },
    { key: "Backspace", shift: true },
  ],
  enabled: hasSelection,
  run: () => {
    return run(api.deleteClips(selected(), true));
  },
};
