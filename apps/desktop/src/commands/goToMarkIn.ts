import { api } from "../ipc";
import { run, uiStore } from "../store";
import type { Command } from "./types";

export const goToMarkInCommand: Command = {
  id: "goToMarkIn",
  label: "cmdGoToMarkIn",
  group: "timeline",
  keys: [{ key: "i", shift: true }],
  enabled: () => {
    return uiStore.get().markIn !== null;
  },
  run: () => {
    const { markIn } = uiStore.get();
    return markIn === null ? undefined : run(api.seek(markIn));
  },
};
